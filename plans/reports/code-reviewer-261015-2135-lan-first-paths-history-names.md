# Review: LAN-first path selector + Mac history device names

Score: 7.5/10. Scope: uncommitted diff (Rust net/*, Cargo, Swift history). Not re-run: tests/clippy (user verified). `cargo` not on PATH here, so `cargo fmt --check` not run.

## Verdict vs requirements
- (a) Tier order Lan < Direct < Relay is correct, generic, no vendor ranges. Semantics match iroh: same single-pass min-key logic as BiasedRttPathSelector, empty PathSelection = keep current, stats()==None skipped, duplicate addr aggregated by min. With empty/failed interface read, every IP is Direct, so behaviour degrades to iroh default (relay still last). Never makes a peer unreachable: selector only reorders already-open paths.
- (b) Done, matches Windows (fallback "another device", sanitised). No Critical issues.

## Critical
None.

## Major
1. local_subnets.rs:57-62 `lan_capable` misclassifies L2-style overlays as LAN. ZeroTier (tap, broadcast, e.g. 10.147.17.0/24), OpenVPN-TAP / corporate VPN virtual-ethernet adapters, and WireGuard-on-Windows with a /24 address are not point-to-point and (Windows) not necessarily `Tunnel`. netdev Windows sets POINTOPOINT only for IfType Ppp/Tunnel/Atm (netdev os/windows/interface.rs:280-287). Wintun adapters' IfType is not guaranteed to be 131, so a /24 WG adapter likely lands in Lan tier. Peers there tie with the real LAN and win on RTT, which breaks "plain LAN first". Tailscale only works because it is /32 (Windows) or p2p (mac/Linux). Fix: also reject `if_type.is_known_virtual()` / non-Ethernet/Wi-Fi types as a whitelist (Ethernet, Wireless80211, plus Unknown on Linux), and optionally prefer the default-route interface. Document the residual (Docker/Hyper-V/VM bridges are also "LAN" but harmless: no peer advertises addresses there). Unverified on a real Windows box; needs a test with `ipconfig`/netdev dump from Windows + ZeroTier.
2. local_subnets.rs:38 + :86-98 blocking `netdev::get_interfaces()` in `select()` holding the cache mutex. Acceptable cadence (<=1 per 5 s), but cold first call and each refresh can take 10s-100s ms on Windows (GetAdaptersAddresses) and macOS (SystemConfiguration type lookup), run on the per-remote actor, and the lock serialises all remotes (the selector is shared) so a slow read stalls every peer's actor. Also if `read` panics, the mutex poisons; the fallback then re-reads on EVERY call (no caching) while still holding no lock, i.e. permanently slow. Fix: refresh on a background task/thread (or `spawn_blocking`) and have `get()` return the last snapshot lock-free (ArcSwap or RwLock<Arc>); or release the lock before reading; use `into_inner()` on poison. Pre-warm in NodeBuilder.
3. lan_first_path_selector.rs:71-99 the actual `select()` has zero tests. `PathSelectionContext::for_test` is `pub(crate)` in iroh so only helpers are tested; the tier/margin logic is tested but wiring (current_key, psd.stats, set) is not. Add an integration test with two local endpoints, or at least note the gap. Also local_subnets.rs:144-147 `reads_real_interfaces_without_panicking` is vacuous: `contains()` rejects loopback regardless of table content. Assert on a non-loopback own address instead (e.g. every `lan_capable` interface's own ipv4 is contained).

## Minor
1. Cargo.toml:26 `iroh = "=1.3.0"` blocks patch/security updates, plus the unstable feature. Intentional and commented; add a tracking note/CI check to revisit on each iroh release. Also confirm Cargo.lock only added `netdev` (stat shows +1 line; plausible, netdev was already transitive).
2. local_subnets.rs:52 IPv6 link-local: `fe80::/64` from any up interface makes ANY fe80 remote Lan (scope id ignored). Harmless in practice (path must validate; fe80 only validates on-link), but a tunnel-free rule is cleaner: contain fe80 only if you also keep scope, or skip fe80 nets entirely and treat link-local explicitly. IPv4 169.254/16 from unplugged/self-assigned adapters same (harmless).
3. local_subnets.rs:49-52 wide prefixes: a /8 or /16 (10.0.0.0/8 corporate, Docker 172.17/16) makes routed-but-far peers "Lan"; and an IPv6 global /64 only matches same-/64 (correct). Acceptable generic limitation; mention in doc comment.
4. local_subnets.rs:7-11 rustfmt would likely collapse the `use netdev::{...}` block; run `cargo fmt`.
5. lan_first_path_selector.rs:22-33,35,43,50: `Tier`, `Key`, `tier_of`, `key`, `should_switch` are `pub` and exposed via `pub mod`; make `pub(crate)` (or keep only the selector pub) to avoid widening API. `Tier::Direct` also lumps internet hole-punch with overlays, per owner spec (fine).
6. lan_first_path_selector.rs:44-45 bias applied as subtract with saturating_sub: RTT can go negative (v6 rtt<3 ms). Same ordering as iroh's saturating_add of negative bias; fine. `as i128` casts on as_nanos are safe (u128 -> i128 only overflows after ~5e21 years).
7. lan_first_path_selector.rs:94 debug log fires every switch with remote tier only; add current tier for diagnosability.
8. Code duplication: switching logic copies iroh constants (3 ms / 5 ms); drift is possible on iroh upgrade. Comment at :17 says so; fine.
9. Swift HistoryView.swift:37-40: `PeerText.deviceName` scans device list per row per render (O(n*m), negligible). Observation is correct: `@ObservedObject DevicesModel` with `@Published devices` re-renders on merge/reload, so renames update live. Window shows "another device" until `reload()` completes after sign-in (EngineBridge+Models.swift:49-50); brief and same as Windows.
10. PeerText.swift:15-21 sanitising is applied (name sanitised, empty falls back). Good. A removed device (and any row whose peer is nil) yields "another device"; ambiguous if two peers are removed, acceptable and parity with Windows. Device name is looked up via `nodeId ==` exact; verify case/format parity with `item.peer` (tests use same strings only).
11. OnboardingRenderTests.swift:121 builds `DevicesModel(engine: engine) {}` with the same engine fixture as other tests (not a new mock). Fine.

## Repo rules
- LOC: local_subnets.rs 148, selector 145: both under 200 incl. tests. OK.
- No mocks: `with_reader(fn)` injects a plain fn; test uses static counter. Acceptable.
- Style matches surrounding code.

## Edge/failure matrix
- macOS: utun (Tailscale, WG) = IFF_POINTOPOINT and Tunnel by name; awdl/llw/bridge are up and non-p2p so their fe80/64 count (harmless). OK.
- Linux: tailscale0, wg0, tun* set IFF_POINTOPOINT: excluded. Docker/virbr/br-: counted (harmless). ZeroTier (zt*, tap, broadcast): counted = Major 1.
- Windows: Tailscale /32: safe by prefix only. WG/TAP /24: likely misclassified. Hyper-V vEthernet: counted (harmless).
- No interfaces / read error: all Direct, equals iroh default. Poisoned mutex: handled without panic but uncached (Major 2).
- Interface change: tier re-evaluated within 5 s, but only when iroh calls select (on path events), so a stale tier may persist until then. Acceptable.

## Unresolved questions
1. What IfType does Windows report for Wintun (WireGuard, Tailscale)? Needs a real-machine netdev dump.
2. Should ZeroTier-style L2 overlays count as Direct (owner spec says VPN overlays are last-resort)? If yes, need a type/name whitelist or default-route-based rule.
3. Is the per-remote actor really the caller (blocking only that peer), or the shared socket actor? The user note says socket actor; if so Major 2 is more severe.

**Status:** DONE_WITH_CONCERNS
**Summary:** Score 7.5/10. Correct against iroh semantics and generic; no critical bugs; falls back to iroh default on failure. Needs fixes for overlay misclassification and blocking read, plus select() test coverage.
**Concerns:** Major: ZeroTier/L2 and Windows /24 WG adapters counted as LAN (local_subnets.rs:57-62); blocking get_interfaces under mutex in select (local_subnets.rs:38,86-98); select() untested (lan_first_path_selector.rs:71-99) and a vacuous test (local_subnets.rs:144-147).
