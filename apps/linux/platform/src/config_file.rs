//! Backend/relay/device-name settings: built-in production defaults < `config.toml` < environment.

use crate::paths::EnvFn;
use serde::Deserialize;
use std::path::Path;

/// Packaged default: production backend (the Windows dev default is loopback; Linux ships prod).
pub const DEFAULT_API: &str = "https://api-fsync.10z.one";
pub const DEFAULT_RELAYS: [&str; 2] = [
    "https://relay-fsync.10z.one",
    "https://relay-sg-fsync.10z.one",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid config file {path}: {message}")]
    Parse { path: String, message: String },
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    api: Option<String>,
    relays: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub api: String,
    pub relays: Vec<String>,
    pub device_name: String,
    /// True only for plain-http loopback backends (dev/e2e).
    pub allow_insecure_http: bool,
}

/// Comma-separated list, trimmed, empty entries dropped (same as `FSYNC_RELAY` on Windows/CLI).
pub fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect()
}

/// `http://` URL whose host is `localhost` or a loopback IP.
pub fn is_loopback_http(api: &str) -> bool {
    let Some(rest) = api.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn hostname() -> String {
    let name = rustix::system::uname()
        .nodename()
        .to_string_lossy()
        .into_owned();
    if name.is_empty() {
        "linux".into()
    } else {
        name
    }
}

impl AppConfig {
    /// A missing config file is fine; an unreadable or malformed one is an error (not ignored).
    pub fn load(config_file: &Path, env: EnvFn<'_>) -> Result<Self, ConfigError> {
        let file = match std::fs::read_to_string(config_file) {
            Ok(text) => toml::from_str::<FileConfig>(&text).map_err(|e| ConfigError::Parse {
                path: config_file.display().to_string(),
                message: e.message().to_string(),
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => FileConfig::default(),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: config_file.display().to_string(),
                    source,
                });
            }
        };
        let get = |k: &str| env(k).filter(|v| !v.trim().is_empty());
        let api = get("FSYNC_API")
            .map(|v| v.trim().to_string())
            .or(file
                .api
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty()))
            .unwrap_or_else(|| DEFAULT_API.to_string());
        let relays = get("FSYNC_RELAY")
            .map(|v| split_list(&v))
            .or(file
                .relays
                .map(|r| r.iter().flat_map(|e| split_list(e)).collect()))
            .filter(|r: &Vec<String>| !r.is_empty())
            .unwrap_or_else(|| DEFAULT_RELAYS.map(String::from).to_vec());
        let device_name = get("FSYNC_DEVICE_NAME")
            .map(|v| v.trim().to_string())
            .unwrap_or_else(hostname);
        Ok(Self {
            allow_insecure_http: is_loopback_http(&api),
            api,
            relays,
            device_name,
        })
    }
}
