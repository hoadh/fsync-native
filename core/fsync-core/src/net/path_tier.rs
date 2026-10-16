//! Coarse quality class of the path a peer is reached over, for display (CLI `path` field).
//!
//! Core-only and separate from [`crate::PathState`], which is mirrored over FFI and stays
//! Offline | Lan | Relay. Sampled on demand from the connections' selected paths; no state.

use super::{
    lan_first_path_selector::{Tier, tier_of},
    local_subnets::LocalSubnets,
};
use iroh::{
    TransportAddr,
    endpoint::{Connection, transports::Addr},
};

/// The path selector's tier (Lan < Direct < Relay, lower is better, so `min` is the best one).
pub use super::lan_first_path_selector::Tier as PathTier;

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lan => "lan",
            Self::Direct => "direct",
            Self::Relay => "relay",
        }
    }
}

/// Tier of one path address: the selector's rule, plus loopback counts as `Lan` (a real network
/// never offers it as a candidate, but two nodes on one host do).
pub fn tier_of_addr(addr: &TransportAddr, lan: &LocalSubnets) -> PathTier {
    match addr {
        TransportAddr::Ip(sa) if sa.ip().to_canonical().is_loopback() => Tier::Lan,
        TransportAddr::Ip(sa) => tier_of(&Addr::Ip(*sa), lan),
        TransportAddr::Relay(_) => Tier::Relay,
        _ => Tier::Direct,
    }
}

/// Tier of the connection's selected path; `None` while no path is selected (closing, not yet up).
pub fn connection_tier(conn: &Connection, lan: &LocalSubnets) -> Option<PathTier> {
    conn.paths()
        .iter()
        .find(|p| p.is_selected())
        .map(|p| tier_of_addr(p.remote_addr(), lan))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> TransportAddr {
        TransportAddr::Ip(s.parse().unwrap())
    }

    fn lan() -> LocalSubnets {
        LocalSubnets::from_nets([
            "192.168.1.239/24".parse().unwrap(),
            "2001:db8:1::5/64".parse().unwrap(),
        ])
    }

    #[test]
    fn ipv4_on_subnet_is_lan_other_private_or_public_is_direct() {
        assert_eq!(tier_of_addr(&ip("192.168.1.43:1"), &lan()), PathTier::Lan);
        assert_eq!(
            tier_of_addr(&ip("100.127.164.17:1"), &lan()),
            PathTier::Direct
        );
        assert_eq!(tier_of_addr(&ip("203.0.113.7:1"), &lan()), PathTier::Direct);
    }

    #[test]
    fn ipv6_follows_the_same_rule() {
        assert_eq!(
            tier_of_addr(&ip("[2001:db8:1::99]:1"), &lan()),
            PathTier::Lan
        );
        assert_eq!(
            tier_of_addr(&ip("[2001:db8:2::1]:1"), &lan()),
            PathTier::Direct
        );
    }

    #[test]
    fn loopback_is_lan_even_without_a_matching_subnet() {
        let none = LocalSubnets::default();
        assert_eq!(tier_of_addr(&ip("127.0.0.1:1"), &none), PathTier::Lan);
        assert_eq!(tier_of_addr(&ip("[::1]:1"), &none), PathTier::Lan);
        assert_eq!(
            tier_of_addr(&ip("[::ffff:127.0.0.1]:1"), &none),
            PathTier::Lan
        );
    }

    #[test]
    fn relay_address_is_relay() {
        let r = TransportAddr::Relay("https://relay.example/".parse().unwrap());
        assert_eq!(tier_of_addr(&r, &lan()), PathTier::Relay);
    }

    #[test]
    fn best_tier_is_the_smallest_and_names_are_stable() {
        let all = [PathTier::Relay, PathTier::Lan, PathTier::Direct];
        assert_eq!(all.iter().copied().min(), Some(PathTier::Lan));
        assert_eq!(all.map(PathTier::as_str), ["relay", "lan", "direct"]);
    }
}
