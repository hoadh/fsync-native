//! Which remote IPs are on this machine's local network (same on-link subnet as a real interface).
//!
//! Generic on purpose, with no knowledge of any VPN vendor. An interface counts as LAN only when it
//! is up, physical (netdev: not a tunnel/bridge/VM adapter), not point-to-point, and routes to a
//! gateway. Overlay VPNs (Tailscale, WireGuard, ZeroTier, OpenVPN TAP) fail at least one of these:
//! they are tun/utun/Wintun devices or virtual adapters, and none carries the default gateway in a
//! split-tunnel setup.
//!
//! Known limits, all generic: a full-tunnel VPN whose adapter looks physical and owns the default
//! gateway counts as LAN; any fe80:: peer matches a LAN interface's link-local /64; a very wide
//! on-link prefix (a flat 10/8) also covers routed peers. A wrong verdict only costs the LAN
//! preference (both paths land in the same tier, i.e. iroh's plain RTT ordering); it never blocks
//! a connection.

use netdev::{Interface, interface::types::InterfaceType, ipnet::IpNet};
use std::net::IpAddr;

/// The on-link subnets of this machine's LAN interfaces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalSubnets {
    nets: Vec<IpNet>,
}

impl LocalSubnets {
    pub fn from_nets(nets: impl IntoIterator<Item = IpNet>) -> Self {
        let mut nets: Vec<IpNet> = nets.into_iter().map(|n| n.trunc()).collect();
        nets.sort_unstable();
        nets.dedup();
        Self { nets }
    }

    /// Reads the OS interface table. Blocking (syscalls), so never call it on a hot path.
    pub fn read() -> Self {
        Self::from_nets(
            netdev::get_interfaces()
                .iter()
                .filter(|i| is_lan_interface(i))
                .flat_map(|i| {
                    let v4 = i.ipv4.iter().copied().map(IpNet::V4);
                    let v6 = i.ipv6.iter().copied().map(IpNet::V6);
                    v4.chain(v6)
                }),
        )
    }

    /// `ip` is reachable on-link: it lies inside one of our subnets (loopback/unspecified never are).
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        !ip.is_loopback() && !ip.is_unspecified() && self.nets.iter().any(|n| n.contains(&ip))
    }
}

fn is_lan_interface(i: &Interface) -> bool {
    IfaceFacts {
        up: i.is_up(),
        loopback_or_p2p: i.is_loopback() || i.is_point_to_point(),
        virtual_or_tunnel: matches!(i.if_type, InterfaceType::Tunnel | InterfaceType::Ppp)
            || !i.is_physical(),
        has_gateway: i.gateway.is_some(),
    }
    .is_lan()
}

/// What the LAN rule looks at, kept apart from netdev so the rule is tested without real interfaces.
#[derive(Clone, Copy)]
struct IfaceFacts {
    up: bool,
    loopback_or_p2p: bool,
    virtual_or_tunnel: bool,
    has_gateway: bool,
}

impl IfaceFacts {
    fn is_lan(self) -> bool {
        self.up && !self.loopback_or_p2p && !self.virtual_or_tunnel && self.has_gateway
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subnets(nets: &[&str]) -> LocalSubnets {
        LocalSubnets::from_nets(nets.iter().map(|n| n.parse().unwrap()))
    }

    #[test]
    fn same_subnet_is_lan_others_are_not() {
        // Wi-Fi /24, IPv6 /64 and link-local, as a laptop reports them; a VPN /32 host prefix.
        let s = subnets(&[
            "192.168.1.239/24",
            "2001:db8:1::5/64",
            "fe80::1/64",
            "100.66.219.107/32",
        ]);
        assert!(s.contains("192.168.1.43".parse().unwrap()));
        assert!(s.contains("2001:db8:1::99".parse().unwrap()));
        assert!(s.contains("fe80::abcd".parse().unwrap()));
        assert!(
            s.contains("::ffff:192.168.1.43".parse().unwrap()),
            "v4-mapped is canonicalised"
        );
        // A peer behind the overlay VPN or across the internet is not on-link.
        assert!(!s.contains("100.127.164.17".parse().unwrap()));
        assert!(!s.contains("192.168.2.10".parse().unwrap()));
        assert!(!s.contains("203.0.113.7".parse().unwrap()));
        assert!(!s.contains("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn empty_table_has_no_lan() {
        assert!(!LocalSubnets::default().contains("192.168.1.43".parse().unwrap()));
    }

    #[test]
    fn only_up_physical_gatewayed_interfaces_are_lan() {
        let wifi = IfaceFacts {
            up: true,
            loopback_or_p2p: false,
            virtual_or_tunnel: false,
            has_gateway: true,
        };
        assert!(wifi.is_lan(), "Wi-Fi / Ethernet with a router");
        assert!(!IfaceFacts { up: false, ..wifi }.is_lan(), "down");
        let utun = IfaceFacts {
            loopback_or_p2p: true,
            ..wifi
        };
        assert!(!utun.is_lan(), "loopback or point-to-point (utun, wg)");
        let tap = IfaceFacts {
            virtual_or_tunnel: true,
            ..wifi
        };
        assert!(!tap.is_lan(), "tunnel / virtual adapter (Wintun, TAP)");
        let overlay = IfaceFacts {
            has_gateway: false,
            ..wifi
        };
        assert!(!overlay.is_lan(), "no gateway: overlay or host-only net");
    }

    #[test]
    #[ignore = "diagnostic: prints this machine's interface verdicts"]
    fn print_interface_verdicts() {
        for i in netdev::get_interfaces() {
            println!(
                "{} {:?} up={} p2p={} phys={} gw={} lan={} v4={:?}",
                i.friendly_name.as_deref().unwrap_or(&i.name),
                i.if_type,
                i.is_up(),
                i.is_point_to_point(),
                i.is_physical(),
                i.gateway.is_some(),
                is_lan_interface(&i),
                i.ipv4
            );
        }
    }
}
