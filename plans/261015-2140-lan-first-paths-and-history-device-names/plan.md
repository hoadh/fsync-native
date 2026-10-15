---
status: completed
created: 2026-10-15
---
# LAN-first path selection + device names in history

Owner decisions:
- Prefer LAN for speed. Use any other direct path (Tailscale or other VPN, internet hole-punch) only when no LAN path exists; the relay comes last.
- Must be generic and not depend on Tailscale (most users never install it).
- Show device names, not node ids, in history.

Evidence: `plans/reports/tester-261015-2130-tailscale-vs-lan-throughput.md`. On the same LAN, Tailscale ran 34–56% slower than plain LAN, and fsync is already E2E encrypted.

## Phase 1: core LAN-first `PathSelector` (status: completed)
- `core/fsync-core/Cargo.toml`: set `iroh = { version = "=1.3.0", features = ["unstable-custom-transports"] }`. The API is unstable, so the pin is intentional and needs re-checking on upgrade. Add `netdev` (default-features off, feature `gateway`; already in the lock via netwatch).
- New `net/local_subnets.rs`:
  - LAN rule: up && !loopback/p2p && !tunnel/ppp && netdev `is_physical()` && has gateway. Snapshot the subnets of matching interfaces.
  - `is_lan(ip)` returns true for a link-local address or an address inside one of those subnets.
  - Generic rule: VPN overlays (Tailscale /32, utun, wg, ZeroTier tun) are excluded by interface kind, missing gateway or host prefix, never by vendor address range.
- New `net/subnet_cache.rs`:
  - Non-blocking `get()`; background std-thread refresh every 5 s.
  - `catch_unwind` with backoff, so a refresh panic cannot kill the thread.
- New `net/lan_first_path_selector.rs`:
  - Tiers: LAN < other direct < relay.
  - Within a tier, use biased RTT (IPv6 −3 ms) with 5 ms stickiness, the same as iroh's default.
  - Switching across tiers is immediate.
  - Pure `choose()`, unit-tested without iroh types.
- `net/node_builder.rs`: install the selector via `.path_selector(...)`.
- Tests: unit tests for tier ordering, stickiness and subnet classification. Existing net e2e tests stay green.

## Phase 2: Mac history shows device names (status: completed)
- Windows already maps `peer` to a device name, falling back to "another device" (HistoryViewModel.cs:85).
- Add `PeerText.deviceName(peer:in:)` in FsyncMacCore.
- `HistoryView` observes `DevicesModel`, and `HistoryRowView` takes the resolved name.
- Unit test for the mapping (known, unknown, nil, sanitised).

## Phase 3: verify (status: completed)
- Run cargo tests plus swift build/test. Build the signed app and confirm live:
  - The path is LAN while Tailscale is up.
  - History shows the device name.
- Then code review (≥9.5: done, 9.7), docs (done), journal (done), and ask before commit.

## Results
- `cargo test --workspace`: 321 passed, 0 failed, 3 ignored. `clippy -D warnings`: clean.
- `swift test`: 231 passed, 18 skipped, 0 failures.
- Interface verdicts:
  - Mac: only Wi-Fi is LAN; `utun*` excluded.
  - Windows: only "Ethernet 2" is LAN. Tailscale = ProprietaryVirtual, no gateway, /32 excluded.
- Live, Tailscale up:
  - 256 MiB each way: Mac→Win 58 MiB/s, Win→Mac 64 MiB/s. Both SHA256 match, both via Lan.
  - en0 and Ethernet 2 carried ~285 MB each way; Tailscale adapters carried ~45 KB.
- Signed Mac app: history shows "From WASO", 0 hex node ids.
- Code review rounds: 7.5 → 9.0 → 9.4 → 9.7 (gate ≥9.5 met). Round 1: `plans/reports/code-reviewer-261015-2135-lan-first-paths-history-names.md`; later rounds inline.

## Unresolved
- The UI "On LAN" label still means "any direct path"; a separate Direct state is out of scope.
- The iroh `=1.3.0` pin must be re-checked on upgrade.
- Known generic limits: a full-tunnel VPN that looks physical, fe80 matches, wide on-link prefixes. Each degrades to iroh's default RTT ordering only.
- `select()` wiring is covered only by the live test.
