//! User settings persisted in the history database (`settings` table, one row per key).

use crate::{
    error::{CoreError, Result},
    history::History,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipMode {
    /// Apply incoming clips to the system clipboard.
    #[default]
    Auto,
    /// Only record them; the user picks what to paste.
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub clip_mode: ClipMode,
    pub retention_days: u32,
    pub retention_bytes: u64,
    pub max_clip_bytes: u64,
    /// App bundle ids / executable names whose clipboard changes are never sent or stored.
    pub denylist: Vec<String>,
    pub auto_accept_files: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            clip_mode: ClipMode::Auto,
            retention_days: 30,
            retention_bytes: 1 << 30,
            max_clip_bytes: 50 << 20,
            denylist: vec![],
            auto_accept_files: false,
        }
    }
}

const MAX_DENY: usize = 256;
const MAX_DENY_LEN: usize = 256;

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !(1..=3650).contains(&self.retention_days) {
            return Err(CoreError::Invalid("retention_days must be 1..=3650"));
        }
        if self.retention_bytes == 0 || self.max_clip_bytes == 0 {
            return Err(CoreError::Invalid("byte limits must be positive"));
        }
        if self.denylist.len() > MAX_DENY
            || self
                .denylist
                .iter()
                .any(|d| d.is_empty() || d.chars().count() > MAX_DENY_LEN)
        {
            return Err(CoreError::Invalid("invalid denylist entry"));
        }
        Ok(())
    }

    fn pairs(&self) -> Result<Vec<(&'static str, String)>> {
        Ok(vec![
            (
                "clip_mode",
                serde_json::to_string(&self.clip_mode).map_err(json)?,
            ),
            ("retention_days", self.retention_days.to_string()),
            ("retention_bytes", self.retention_bytes.to_string()),
            ("max_clip_bytes", self.max_clip_bytes.to_string()),
            (
                "denylist",
                serde_json::to_string(&self.denylist).map_err(json)?,
            ),
            ("auto_accept_files", self.auto_accept_files.to_string()),
        ])
    }
}

fn json(e: serde_json::Error) -> CoreError {
    CoreError::Store(format!("settings encoding: {e}"))
}

/// Stored values over defaults; an unparsable value falls back to its default.
pub(crate) fn load(conn: &Connection) -> Result<Settings> {
    let mut st = conn.prepare("SELECT key, value FROM settings")?;
    let kv: HashMap<String, String> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let d = Settings::default();
    fn get<T: std::str::FromStr>(kv: &HashMap<String, String>, k: &str, d: T) -> T {
        kv.get(k).and_then(|v| v.parse().ok()).unwrap_or(d)
    }
    Ok(Settings {
        clip_mode: kv
            .get("clip_mode")
            .and_then(|v| serde_json::from_str(v).ok())
            .unwrap_or(d.clip_mode),
        retention_days: get(&kv, "retention_days", d.retention_days),
        retention_bytes: get(&kv, "retention_bytes", d.retention_bytes),
        max_clip_bytes: get(&kv, "max_clip_bytes", d.max_clip_bytes),
        denylist: kv
            .get("denylist")
            .and_then(|v| serde_json::from_str(v).ok())
            .unwrap_or(d.denylist),
        auto_accept_files: get(&kv, "auto_accept_files", d.auto_accept_files),
    })
}

impl History {
    pub async fn settings(&self) -> Result<Settings> {
        self.read(|c| load(c)).await
    }

    pub async fn set_settings(&self, s: Settings) -> Result<()> {
        s.validate()?;
        let pairs = s.pairs()?;
        self.write(move |c| {
            let tx = c.transaction()?;
            for (k, v) in pairs {
                tx.execute(
                    "INSERT INTO settings(key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![k, v],
                )?;
            }
            Ok(tx.commit()?)
        })
        .await
    }
}
