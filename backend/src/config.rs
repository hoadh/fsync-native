//! Runtime configuration from environment variables.

use crate::email::config::{EmailMode, resolve as resolve_email};
use anyhow::{Context, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use ipnet::IpNet;
use std::{net::SocketAddr, str::FromStr, time::Duration};
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// ed25519 seed for JWT signing; also the root for the code-hash key.
    pub signing_seed: Zeroizing<[u8; 32]>,
    /// Dev mode logs OTP codes at debug level, allows an ephemeral signing key and http relays.
    pub dev_mode: bool,
    /// Number of reverse proxies in front of us. 0 = use the socket peer address; N > 0 = the
    /// client IP is the Nth entry from the right of `X-Forwarded-For`.
    pub trust_proxy_hops: usize,
    /// Peers (CIDR ranges) allowed to set `X-Forwarded-For`; the header is ignored from anyone
    /// else. Must be non-empty when `trust_proxy_hops > 0`.
    pub trusted_proxies: Vec<IpNet>,
    pub heartbeat_timeout: Duration,
    /// Cap on concurrent presence sockets that have not authenticated yet.
    pub max_unauth_ws: usize,
    /// Same, per client IP.
    pub max_unauth_ws_per_ip: usize,
    /// Cap on concurrently processed HTTP requests (excess gets 503).
    pub max_in_flight: usize,
    /// How often a pending (pairing) presence socket re-checks its offer / membership.
    pub pending_recheck: Duration,
    /// Max time a client may take to send request headers (slowloris guard).
    pub header_read_timeout: Duration,
    /// OTP delivery: log sink or SMTP (see `email::config`).
    pub email: EmailMode,
    /// Global cap on OTP emails per rolling 24 h (Gmail SMTP allows about 500 a day).
    pub otp_daily_send_cap: u32,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("dev_mode", &self.dev_mode)
            .field("trust_proxy_hops", &self.trust_proxy_hops)
            .field("trusted_proxies", &self.trusted_proxies)
            .field("email", &self.email)
            .finish_non_exhaustive()
    }
}

fn flag(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1" | "true" | "TRUE" | "yes")
    )
}

/// Parse an optional numeric env var.
fn num<T: FromStr>(name: &str, default: T) -> anyhow::Result<T> {
    match std::env::var(name) {
        Ok(v) => v
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("{name} must be a non-negative integer")),
        Err(_) => Ok(default),
    }
}

/// Comma-separated CIDRs (a bare address is a single-host range). Empty input = empty list.
pub fn parse_cidrs(raw: &str) -> anyhow::Result<Vec<IpNet>> {
    raw.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<IpNet>()
                .or_else(|_| p.parse::<std::net::IpAddr>().map(IpNet::from))
                .map_err(|_| anyhow::anyhow!("TRUSTED_PROXIES: invalid CIDR {p:?}"))
        })
        .collect()
}

impl Config {
    /// All tunables at their defaults.
    pub fn with_defaults(database_url: String, signing_seed: [u8; 32]) -> Self {
        Self {
            database_url,
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 8080)),
            signing_seed: Zeroizing::new(signing_seed),
            dev_mode: false,
            trust_proxy_hops: 0,
            trusted_proxies: Vec::new(),
            heartbeat_timeout: Duration::from_secs(60),
            max_unauth_ws: 1000,
            max_unauth_ws_per_ip: 10,
            max_in_flight: 512,
            pending_recheck: Duration::from_secs(30),
            header_read_timeout: Duration::from_secs(10),
            email: EmailMode::Log,
            otp_daily_send_cap: 450,
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is required")?;
        let dev_mode = flag("FSYNC_DEV_MODE");
        if dev_mode && std::env::var("FSYNC_ENV").as_deref() == Ok("production") {
            bail!("FSYNC_DEV_MODE is not allowed when FSYNC_ENV=production");
        }
        let seed = match std::env::var("JWT_SIGNING_KEY").map(Zeroizing::new) {
            Ok(b64) => {
                let raw = Zeroizing::new(
                    STANDARD
                        .decode(b64.trim())
                        .context("JWT_SIGNING_KEY must be base64")?,
                );
                <[u8; 32]>::try_from(raw.as_slice())
                    .map_err(|_| anyhow::anyhow!("JWT_SIGNING_KEY must decode to 32 bytes"))?
            }
            Err(_) if dev_mode => {
                tracing::warn!("JWT_SIGNING_KEY unset: using ephemeral key (dev mode)");
                let mut seed = [0u8; 32];
                getrandom::fill(&mut seed).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
                seed
            }
            Err(_) => bail!("JWT_SIGNING_KEY is required (base64 of 32 random bytes)"),
        };
        let mut cfg = Self::with_defaults(database_url, seed);
        cfg.dev_mode = dev_mode;
        let production = std::env::var("FSYNC_ENV").as_deref() == Ok("production");
        cfg.email = resolve_email(&|k| std::env::var(k).ok(), production)?;
        if let Ok(addr) = std::env::var("BIND_ADDR") {
            cfg.bind_addr = addr.parse().context("BIND_ADDR must be host:port")?;
        }
        cfg.trust_proxy_hops = num("TRUST_PROXY_HOPS", 0)?;
        cfg.trusted_proxies = parse_cidrs(&std::env::var("TRUSTED_PROXIES").unwrap_or_default())?;
        if cfg.trust_proxy_hops > 0 && cfg.trusted_proxies.is_empty() {
            bail!("TRUST_PROXY_HOPS > 0 requires TRUSTED_PROXIES (comma-separated CIDRs)");
        }
        cfg.heartbeat_timeout = Duration::from_secs(num("PRESENCE_TIMEOUT_SECS", 60)?);
        cfg.max_unauth_ws = num("MAX_UNAUTH_WS", cfg.max_unauth_ws)?;
        cfg.max_unauth_ws_per_ip = num("MAX_UNAUTH_WS_PER_IP", cfg.max_unauth_ws_per_ip)?;
        cfg.max_in_flight = num("MAX_IN_FLIGHT", cfg.max_in_flight)?;
        cfg.otp_daily_send_cap = num("OTP_DAILY_SEND_CAP", cfg.otp_daily_send_cap)?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_cidrs;

    #[test]
    fn cidr_list_parsing() {
        let v = parse_cidrs(" 10.0.0.0/8, 192.0.2.1 ,fd00::/8,").unwrap();
        assert_eq!(v.len(), 3);
        assert!(v[1].contains(&"192.0.2.1".parse::<std::net::IpAddr>().unwrap()));
        assert!(parse_cidrs("").unwrap().is_empty());
        assert!(parse_cidrs("10.0.0.0/33").is_err());
        assert!(parse_cidrs("nope").is_err());
    }
}
