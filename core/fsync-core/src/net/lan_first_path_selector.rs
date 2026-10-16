//! iroh path selection that prefers the local network: LAN, then any other direct path, then relay.
//!
//! iroh's default ranks every direct path by RTT alone. On one LAN, an overlay VPN path (Tailscale,
//! ZeroTier, WireGuard…) can have about the same RTT as the plain LAN path yet move data 34–56% slower,
//! because it encrypts a second time with a smaller MTU, and fsync already encrypts end to end. So
//! the tier decides first and RTT only breaks ties inside a tier, with the same IPv6 bias and
//! anti-flap margin as iroh's default.
//!
//! Uses iroh's unstable `PathSelector` API (feature `unstable-custom-transports`); iroh is pinned.

use super::{local_subnets::LocalSubnets, subnet_cache::SubnetCache};
use iroh::endpoint::transports::{
    Addr, FourTuple, PathSelection, PathSelectionContext, PathSelectionData, PathSelector,
};
use std::time::Duration;

/// Same values as iroh's `BiasedRttPathSelector`.
const IPV6_RTT_ADVANTAGE: Duration = Duration::from_millis(3);
const RTT_SWITCHING_MIN: Duration = Duration::from_millis(5);

/// Lower is better.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Remote IP is on one of our on-link subnets.
    Lan,
    /// Any other direct IP path: overlay VPN, routed private network, internet hole-punch.
    Direct,
    /// Through the relay server.
    Relay,
}

/// (tier, biased RTT in ns): compared lexicographically.
pub(crate) type Key = (Tier, i128);

pub(crate) fn tier_of(addr: &Addr, lan: &LocalSubnets) -> Tier {
    match addr {
        Addr::Ip(sa) if lan.contains(sa.ip()) => Tier::Lan,
        Addr::Relay(..) => Tier::Relay,
        _ => Tier::Direct,
    }
}

pub(crate) fn key(tier: Tier, ipv6: bool, rtt: Duration) -> Key {
    let bias = if ipv6 {
        IPV6_RTT_ADVANTAGE.as_nanos() as i128
    } else {
        0
    };
    (tier, (rtt.as_nanos() as i128).saturating_sub(bias))
}

/// Move from the current path to the best one? Across tiers always; inside a tier only when the
/// best is clearly faster, so jitter does not flip paths.
pub(crate) fn should_switch(best: Key, current: Option<Key>) -> bool {
    match current {
        None => true,
        Some(cur) if cur.0 != best.0 => true,
        Some(cur) => best.1 + RTT_SWITCHING_MIN.as_nanos() as i128 <= cur.1,
    }
}

#[derive(Debug, Default)]
pub struct LanFirstPathSelector {
    subnets: SubnetCache,
}

impl LanFirstPathSelector {
    fn key_of(&self, path: &FourTuple, lan: &LocalSubnets, rtt: Duration) -> Key {
        let remote = path.remote();
        let ipv6 = matches!(&remote, Addr::Ip(sa) if sa.is_ipv6());
        key(tier_of(&remote, lan), ipv6, rtt)
    }
}

/// Index of the path to select, given each candidate's key and whether it is the current path;
/// `None` keeps the current one. Pure, so the policy is tested without iroh types.
pub(crate) fn choose(candidates: &[(Key, bool)]) -> Option<usize> {
    // One path per connection: the current address can appear more than once; keep its best key.
    let current = candidates
        .iter()
        .filter(|(_, cur)| *cur)
        .map(|(k, _)| *k)
        .min();
    let (best, k) = candidates.iter().enumerate().min_by_key(|(_, (k, _))| *k)?;
    should_switch(k.0, current).then_some(best)
}

impl PathSelector for LanFirstPathSelector {
    fn select(&self, ctx: &PathSelectionContext<'_>) -> PathSelection {
        let lan = self.subnets.get();
        let current = ctx.current();
        // Stats vanish when a path closes concurrently with selection; skip those paths.
        let paths: Vec<(PathSelectionData<'_>, (Key, bool))> = ctx
            .paths()
            .filter_map(|psd| {
                let rtt = psd.stats()?.rtt;
                let path = psd.network_path();
                let kc = (self.key_of(path, &lan, rtt), Some(path) == current);
                Some((psd, kc))
            })
            .collect();
        let keys: Vec<(Key, bool)> = paths.iter().map(|(_, kc)| *kc).collect();
        let mut selection = PathSelection::none();
        if let Some(i) = choose(&keys) {
            let (psd, (k, _)) = &paths[i];
            let from = keys.iter().filter(|(_, cur)| *cur).map(|(k, _)| k.0).min();
            tracing::debug!(path = %psd.network_path(), tier = ?k.0, from = ?from, "path selected");
            selection.set(psd);
        }
        selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn ip(s: &str) -> Addr {
        Addr::Ip(s.parse::<SocketAddr>().unwrap())
    }
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn tiers_follow_local_subnets_not_vendor_ranges() {
        let lan = LocalSubnets::from_nets(["192.168.1.239/24".parse().unwrap()]);
        assert_eq!(tier_of(&ip("192.168.1.43:41641"), &lan), Tier::Lan);
        assert_eq!(tier_of(&ip("100.127.164.17:41641"), &lan), Tier::Direct);
        assert_eq!(
            tier_of(&ip("10.0.0.5:1"), &lan),
            Tier::Direct,
            "private but not on-link"
        );
        assert_eq!(tier_of(&ip("[2001:db8::1]:1"), &lan), Tier::Direct);
    }

    #[test]
    fn lan_beats_faster_direct_and_direct_beats_relay() {
        let lan = key(Tier::Lan, false, ms(25));
        let vpn = key(Tier::Direct, false, ms(3));
        let relay = key(Tier::Relay, false, ms(1));
        assert!(lan < vpn && vpn < relay);
        assert!(
            should_switch(lan, Some(vpn)),
            "LAN appears: leave the VPN path at once"
        );
        assert!(should_switch(vpn, Some(relay)));
    }

    #[test]
    fn same_tier_switch_needs_clear_margin() {
        let cur = key(Tier::Lan, false, ms(20));
        assert!(!should_switch(key(Tier::Lan, false, ms(16)), Some(cur)));
        assert!(should_switch(key(Tier::Lan, false, ms(15)), Some(cur)));
        assert!(should_switch(cur, None));
    }

    #[test]
    fn ipv6_gets_small_advantage_inside_tier() {
        assert!(key(Tier::Lan, true, ms(10)) < key(Tier::Lan, false, ms(9)));
        assert!(key(Tier::Lan, true, ms(10)) > key(Tier::Lan, false, ms(6)));
    }

    #[test]
    fn choose_moves_off_overlay_to_lan_and_then_sticks() {
        let vpn = key(Tier::Direct, false, ms(3));
        let lan = key(Tier::Lan, false, ms(25));
        let relay = key(Tier::Relay, false, ms(1));
        // Connected over the overlay; a LAN path appears: switch to it.
        assert_eq!(
            choose(&[(relay, false), (vpn, true), (lan, false)]),
            Some(2)
        );
        // Already on LAN: nothing to do even though the overlay has lower RTT.
        assert_eq!(choose(&[(vpn, false), (lan, true)]), None);
    }

    #[test]
    fn choose_handles_empty_first_and_duplicate_current() {
        assert_eq!(choose(&[]), None);
        let relay = key(Tier::Relay, false, ms(40));
        assert_eq!(
            choose(&[(relay, false)]),
            Some(0),
            "no current path: take the best"
        );
        // The current address listed twice: its best key counts, so a 3 ms-better LAN path is jitter.
        let cur_slow = key(Tier::Lan, false, ms(30));
        let cur_fast = key(Tier::Lan, false, ms(10));
        let other = key(Tier::Lan, false, ms(7));
        assert_eq!(
            choose(&[(cur_slow, true), (other, false), (cur_fast, true)]),
            None
        );
    }
}
