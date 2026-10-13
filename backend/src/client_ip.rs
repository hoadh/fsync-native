//! Client identity for per-IP limits: socket peer, or a trusted proxy hop of `X-Forwarded-For`.

use crate::config::Config;
use axum::http::HeaderMap;
use ipnet::IpNet;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

/// Rate-limit bucket key: IPv4 as-is, IPv6 collapsed to its /64 (a single host owns a whole /64).
fn bucket(ip: IpAddr) -> String {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let mut o = v6.octets();
            o[8..].fill(0);
            format!("{}/64", Ipv6Addr::from(o))
        }
    }
}

/// Whether `ip` (IPv4-mapped IPv6 canonicalized) lies inside one of the trusted proxy ranges.
fn is_trusted(trusted: &[IpNet], ip: IpAddr) -> bool {
    let ip = ip.to_canonical();
    trusted.iter().any(|n| n.contains(&ip))
}

/// With `hops == 0`, an untrusted `peer`, or no usable header (`xff == None`) returns the peer.
/// Otherwise the entry `hops` positions from the right of the (concatenated) `X-Forwarded-For`
/// values; anything missing or malformed falls back to the peer.
pub fn resolve(hops: usize, trusted: &[IpNet], xff: Option<&[&str]>, peer: IpAddr) -> String {
    let Some(xff) = xff.filter(|_| hops > 0 && is_trusted(trusted, peer)) else {
        return bucket(peer);
    };
    let entries: Vec<&str> = xff
        .iter()
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .collect();
    let picked = entries
        .len()
        .checked_sub(hops)
        .and_then(|i| entries.get(i))
        .and_then(|s| s.parse::<IpAddr>().ok());
    bucket(picked.unwrap_or(peer))
}

/// `X-Forwarded-For` lines as text; `None` if any line is not valid UTF-8 (dropping it would
/// shift the from-the-right index, so the whole header is distrusted instead).
fn xff_lines(headers: &HeaderMap) -> Option<Vec<&str>> {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .map(|v| v.to_str().ok())
        .collect()
}

pub fn client_key(cfg: &Config, headers: &HeaderMap, peer: SocketAddr) -> String {
    let xff = xff_lines(headers);
    resolve(
        cfg.trust_proxy_hops,
        &cfg.trusted_proxies,
        xff.as_deref(),
        peer.ip(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn peer() -> IpAddr {
        "10.0.0.9".parse().unwrap()
    }

    fn proxies() -> Vec<IpNet> {
        vec!["10.0.0.0/24".parse().unwrap(), "fd00::/8".parse().unwrap()]
    }

    fn r(hops: usize, xff: &[&str]) -> String {
        resolve(hops, &proxies(), Some(xff), peer())
    }

    #[test]
    fn zero_hops_ignores_header() {
        assert_eq!(r(0, &["1.2.3.4"]), "10.0.0.9");
    }

    #[test]
    fn counts_from_the_right_so_client_prepended_entries_are_ignored() {
        // client forged "6.6.6.6"; the proxy appended the real peer it saw.
        assert_eq!(r(1, &["6.6.6.6, 203.0.113.7"]), "203.0.113.7");
        // two proxies: [forged, real client, proxy1-seen]
        assert_eq!(r(2, &["6.6.6.6, 203.0.113.7, 198.51.100.2"]), "203.0.113.7");
        // multiple header lines are one list
        assert_eq!(r(1, &["6.6.6.6", "203.0.113.7"]), "203.0.113.7");
    }

    #[test]
    fn untrusted_peer_cannot_set_the_client_ip() {
        let outsider: IpAddr = "192.0.2.50".parse().unwrap();
        let got = resolve(1, &proxies(), Some(&["203.0.113.7"]), outsider);
        assert_eq!(got, "192.0.2.50");
        // empty trust list trusts nobody
        assert_eq!(resolve(1, &[], Some(&["203.0.113.7"]), peer()), "10.0.0.9");
        // IPv4-mapped peer matches an IPv4 range
        let mapped: IpAddr = "::ffff:10.0.0.9".parse().unwrap();
        let got = resolve(1, &proxies(), Some(&["203.0.113.7"]), mapped);
        assert_eq!(got, "203.0.113.7");
    }

    #[test]
    fn non_utf8_header_line_falls_back_to_peer_without_shifting() {
        let mut h = HeaderMap::new();
        h.append("x-forwarded-for", HeaderValue::from_static("203.0.113.7"));
        h.append(
            "x-forwarded-for",
            HeaderValue::from_bytes(b"\xff\xfe").unwrap(),
        );
        h.append("x-forwarded-for", HeaderValue::from_static("198.51.100.2"));
        let peer = SocketAddr::new(peer(), 4000);
        let mut cfg = Config::with_defaults(String::new(), [0; 32]);
        (cfg.trust_proxy_hops, cfg.trusted_proxies) = (1, proxies());
        assert_eq!(client_key(&cfg, &h, peer), "10.0.0.9");
        h.remove("x-forwarded-for");
        h.append("x-forwarded-for", HeaderValue::from_static("203.0.113.7"));
        assert_eq!(client_key(&cfg, &h, peer), "203.0.113.7");
    }

    #[test]
    fn missing_short_or_malformed_falls_back_to_peer() {
        assert_eq!(resolve(1, &proxies(), None, peer()), "10.0.0.9");
        assert_eq!(r(1, &[]), "10.0.0.9");
        assert_eq!(r(3, &["1.1.1.1, 2.2.2.2"]), "10.0.0.9");
        assert_eq!(r(1, &["1.1.1.1, not-an-ip"]), "10.0.0.9");
        assert_eq!(r(1, &[""]), "10.0.0.9");
    }

    #[test]
    fn ipv6_is_bucketed_by_64() {
        let none: &[IpNet] = &[];
        let a = resolve(0, none, None, "2001:db8:1:2:aaaa::1".parse().unwrap());
        let b = resolve(0, none, None, "2001:db8:1:2:bbbb::9".parse().unwrap());
        let c = resolve(0, none, None, "2001:db8:1:3::1".parse().unwrap());
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, "2001:db8:1:2::/64");
        // IPv4-mapped addresses share the IPv4 bucket.
        let mapped = resolve(0, none, None, "::ffff:10.0.0.9".parse().unwrap());
        assert_eq!(mapped, "10.0.0.9");
        // header entries are bucketed too
        assert_eq!(r(1, &["2001:db8:1:2::77"]), a);
    }
}
