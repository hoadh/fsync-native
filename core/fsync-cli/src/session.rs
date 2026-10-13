//! Private files in the data dir: device secret, history key, session tokens (all 0600).

use anyhow::{Context, Result, bail};
use fsync_core::{
    service::SessionTokens,
    {ensure_private_dir, write_private_atomic},
};
use std::{fs, path::Path, sync::Mutex};
use zeroize::Zeroizing;

pub fn ensure_dir(dir: &Path) -> Result<()> {
    ensure_private_dir(dir).with_context(|| format!("create {}", dir.display()))
}

/// Token refreshes (callback thread) and login write the same file; serialize them.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    write_private_atomic(path, bytes).with_context(|| format!("write {}", path.display()))
}

/// 32 random bytes kept in a 0600 file, created on first use.
pub fn load_or_create_key(path: &Path) -> Result<Zeroizing<[u8; 32]>> {
    match fs::read(path) {
        Ok(raw) => {
            let raw = Zeroizing::new(raw);
            match <[u8; 32]>::try_from(raw.as_slice()) {
                Ok(k) => Ok(Zeroizing::new(k)),
                Err(_) => bail!("{} must hold exactly 32 bytes", path.display()),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut k = Zeroizing::new([0u8; 32]);
            getrandom::fill(k.as_mut()).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
            write_private(path, k.as_ref())?;
            Ok(k)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn save(path: &Path, t: &SessionTokens) -> Result<()> {
    let v = serde_json::json!({
        "access": t.access, "refresh": t.refresh, "account_id": t.account_id,
    });
    write_private(path, &serde_json::to_vec(&v)?)
}

pub fn load(path: &Path) -> Result<Option<SessionTokens>> {
    let raw = match fs::read(path) {
        Ok(r) => Zeroizing::new(r),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let v: serde_json::Value = serde_json::from_slice(&raw).context("session file is not json")?;
    let field = |k: &str| {
        v[k].as_str()
            .map(str::to_owned)
            .with_context(|| format!("session file lacks {k}"))
    };
    Ok(Some(SessionTokens {
        access: field("access")?,
        refresh: field("refresh")?,
        account_id: field("account_id")?,
    }))
}
