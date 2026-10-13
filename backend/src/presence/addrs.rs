//! Validation of peer-announced addresses (`Hello`) and per-socket message rate limits.

use crate::util::relay_url_ok;
use fsync_proto::presence::Addrs;
use std::time::{Duration, Instant};

pub const MAX_DIRECT_ADDRS: usize = 16;
pub const HELLOS_PER_MIN: u32 = 10;
/// Clients heartbeat every 20 s (3/min); 6/min leaves headroom without allowing a flood.
pub const HEARTBEATS_PER_MIN: u32 = 6;
const WINDOW: Duration = Duration::from_secs(60);

/// `None` = protocol violation (too many addrs, bad relay URL). Unroutable direct addresses
/// (loopback, unspecified, multicast, port 0) are dropped so peers never get told to dial
/// themselves or a local service.
pub fn sanitize(mut a: Addrs, dev_mode: bool) -> Option<Addrs> {
    if a.direct.len() > MAX_DIRECT_ADDRS {
        return None;
    }
    if a.relay_url
        .as_deref()
        .is_some_and(|u| !relay_url_ok(u, dev_mode))
    {
        return None;
    }
    a.direct.retain(|s| {
        let ip = s.ip().to_canonical();
        s.port() != 0 && !ip.is_loopback() && !ip.is_unspecified() && !ip.is_multicast()
    });
    Some(a)
}

/// Fixed one-minute-window message counter for one socket.
pub struct MsgLimiter {
    started: Instant,
    count: u32,
    max: u32,
}

impl MsgLimiter {
    pub fn new(max: u32) -> Self {
        Self {
            started: Instant::now(),
            count: 0,
            max,
        }
    }

    pub fn allow(&mut self) -> bool {
        if self.started.elapsed() >= WINDOW {
            *self = Self::new(self.max);
        }
        self.count += 1;
        self.count <= self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addrs(relay: Option<&str>, direct: &[&str]) -> Addrs {
        Addrs {
            relay_url: relay.map(Into::into),
            direct: direct.iter().map(|s| s.parse().unwrap()).collect(),
        }
    }

    #[test]
    fn drops_unroutable_direct_addrs_and_checks_relay() {
        let a = addrs(
            Some("https://r.example.com"),
            &[
                "127.0.0.1:1",
                "0.0.0.0:9",
                "[::1]:5",
                "10.0.0.2:0",
                "[::]:7",
                "10.0.0.2:4433",
            ],
        );
        let out = sanitize(a, false).unwrap();
        assert_eq!(out.direct, vec!["10.0.0.2:4433".parse().unwrap()]);
        assert!(sanitize(addrs(Some("http://r.example.com"), &[]), false).is_none());
        assert!(sanitize(addrs(Some("http://r.example.com"), &[]), true).is_some());
        assert!(sanitize(addrs(Some("ftp://r"), &[]), true).is_none());
        let many: Vec<String> = (1..=17).map(|i| format!("10.0.0.{i}:1")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(sanitize(addrs(None, &many), false).is_none());
    }

    #[test]
    fn msg_limiter_caps_per_window() {
        let mut l = MsgLimiter::new(HELLOS_PER_MIN);
        assert!((0..10).all(|_| l.allow()));
        assert!(!l.allow());
        let mut hb = MsgLimiter::new(HEARTBEATS_PER_MIN);
        assert!((0..6).all(|_| hb.allow()));
        assert!(!hb.allow());
    }
}
