# Code review: phase 03 (lazy-file latency), branch fix/lazy-file-latency

Range `e21fb8a..51c7463` (9 commits). Score: **9.0 / 10**. No critical or high findings.

## Scope
- Rust: `core/fsync-core/src/net/dialer.rs` (`connect_following_addrs`), `service/pairing.rs` (`RELAY_WAIT` 6 s + unit test), `fsync-cli/src/cmd_clip.rs` (`ts_ms` on `ready`), `tests/dial_follows_addresses.rs`.
- Harness: `lib-cross-os-windows-legs.sh`, `lib-cross-os-gui-legs.sh`, `cross-os-with-windows.sh`, `cross-os-cli-peer.ps1`, `cross-os-gui-peer.ps1`, `PasteHelper/Program.cs`.
- Docs: ADR 014, changelog, roadmap, test-results README + 6 result JSONs.

## Evidence
- `cargo clippy -p fsync-core -p fsync-cli --all-targets -D warnings`: clean.
- `cargo test -p fsync-core`: 48 binaries, 250 passed, 0 failed.
- `dial_follows_addresses` 20x idle: 0.36-0.41 s; 8x under 2x nproc busy loops: 0.40-0.51 s. Limit 1.5 s: about 1 s margin. Not flaky.
- Result JSONs parse; x3-file-app-to-win 2971/2977/3029/2975/2970 ms and x2/x5 text 60/66 ms match the docs.
- iroh 1.3.0: `PROBES_TIMEOUT` 3 s, `OVERALL_REPORT_TIMEOUT` 5 s: the ADR's reasoning holds.
- shellcheck: not installed on this host, so it did not run.

## Verified OK
- **Cancellation:** no task is spawned. `connect_following_addrs` is a plain future, so the outer `timeout` drops both the `connect` and the poll loop. Dropping a pending `Endpoint::connect` drops the noq `Connecting`, which closes the connection, so nothing leaks.
- **Budget:** the restart loop runs inside each stage's `timeout(FIRST_ATTEMPT)` and inside the final `timeout(left)`. Total time stays capped at `DIAL_TIMEOUT` 10 s, and the stage escalation is unchanged.
- **Busy loop:** none. The worst case (addresses flap at 100 ms or faster) gives about 100 restarts in 10 s. That is bounded.
- **Equality check:** `EndpointAddr.addrs` is a set, and `MemoryLookup::set_endpoint_info` replaces the entry. A presence refresh with the same addresses therefore does not restart the dial.
- **CLI JSON:** the new field is additive. Every consumer (tests/e2e/*, apps/linux/tests/e2e/*) does a substring grep for `"event":"ready"`, and serde_json puts `event` first in both key orders. The app parsers do not read this event.
- **RELAY_WAIT:** the wait applies only when `relay_urls` is non-empty, and `ep.online()` returns as soon as the relay is up. All core tests use `relay_urls: vec![]`, so they get no extra time. The UX cost is +3 s, only when a relay is configured but unreachable.
- **PasteHelper `--warm`:** the COM object is created and used on the same STA thread. If the caller exits, `ReadLine() is null` and the helper exits. The non-warm path behaves as before.

## Findings

### Medium
**M1. A dial restarts on any address change, including losing addresses** (`dialer.rs:146`).
- What happens: `is_ok_and(|a| a != addr)` is also true when the peer goes offline. `Discovery::feed` then calls `remove_endpoint_info` (discovery.rs:93), `addr_for` returns a bare `EndpointAddr::new(id)`, and the in-flight handshake is cancelled. It is also true when the set only shrinks.
- How it fails:
  - (a) Presence flaps offline, then online, around the peer's websocket reconnect. The in-flight dial to a still-valid address is cancelled and restarted with no addresses. If iroh's remote state has no path for the peer, `resolve_remote` fails with `NoAddress`. `Ok(res) => return res` (dialer.rs:111) then ends the whole `dial_with_recovery` at once, skipping the remaining stages. The send drops into the offline retry schedule (2 s + 5 s), which is the latency this phase removes.
  - (b) A booting peer: `node_addrs.rs:30` re-sends Hello on every iroh address change, without debounce, so presence pushes several updates in its first seconds. Each update cancels a relay handshake that may be nearly finished.
- Not reproduced. (a) depends on iroh's remote-state behaviour, which I did not trace to the end.
- Fix: restart only when the new address set has an address the old one lacks:
  ```rust
  if let Ok(a) = self.discovery.addr_for(peer)
      && a.addrs.iter().any(|x| !addr.addrs.contains(x)) { return; }
  ```
  Optionally keep the old attempt running and race the new one instead of cancelling it. Add a test where the hint is removed mid-dial.

### Low
**L1. `select!` is unbiased** (`dialer.rs:151`). When the connection completes on the same poll where the change fires, tokio picks a branch at random. It can drop a finished `Connection`: one wasted handshake, plus an aborted connection logged on the peer. Fix: add `biased;` with the `connect` branch first.

**L2. Docs say "presence / mDNS" feed the restart** (`dialer.rs:24`, ADR 014 "Latency follow-up", changelog). mDNS is a separate iroh lookup (`node_builder.rs:114-120`), not the `MemoryLookup` that `addr_for` reads. Only presence (and `pair_with` hints) can trigger a restart. Reword the docs.

**L3. `win_wait_cli_ready` can stall or fall through** (`lib-cross-os-windows-legs.sh:160-166`).
- (a) `ConnectTimeout` covers only the connect. If the remote `powershell` wedges after connecting, the loop hangs, and the `kill -0` check is never reached.
- (b) When all 240 tries run out, the loop falls through to `peer_online` without calling `fail`.
- (c) Under `pipefail`, `ssh | grep -q` can return 141 (ssh writes after grep exits). The output is small, so this is rare and costs one extra try.
- Fix: `timeout 20 ssh ...`, use a `ready=1` flag, and `fail "CLI never printed ready"` after the loop.

**L4. The X3 hotkey waits on a log string** (`cross-os-gui-peer.ps1:280-282`). The trigger is the app.log text `clipboard: received files`, with a 20 s fallback. The leg gate is 10 s, so a change to the log wording becomes a "latency over gate" failure instead of a clear error. The stage line does show that `clip_logged` is missing. Suggest putting `fallback_press=true` into the result, or failing with an explicit "log line not seen" message.

**L5. The `RELAY_WAIT` test pins a copied constant** (`pairing.rs:198`). iroh exports the timeout as `iroh::NET_REPORT_TIMEOUT` (u64 seconds, lib.rs:294). Assert `RELAY_WAIT > Duration::from_secs(iroh::NET_REPORT_TIMEOUT)` so an iroh upgrade that raises it breaks the test.

### Informational
- I1. X2/X5 now wait for readiness, so the gate no longer covers the offline-peer dial path (only `NO_READY_WAIT=1` does). The core test covers the follow-address logic, but there is no e2e regression guard for "copy to a peer that is coming online".
- I2. `fsync-cli files` `ready` (`cmd_files.rs:99`) has no `ts_ms`. This is a small inconsistency, not a break.
- I3. The `RELAY_WAIT` fix rests on one passing pairing against a 4-of-8 failure rate. The ADR states this honestly; loop the Windows-offer pairing before calling it closed.

## Plan
Phase todos 1-4 are ticked and supported by evidence. "Review >= 9.5, PR, CI green" stays open: M1 (and L1) need fixing to reach 9.5.

## Unresolved questions
- Does iroh 1.3 `resolve_remote` fail with `NoAddress` for an empty `EndpointAddr` when the remote-state actor still knows paths from the cancelled attempt? This decides whether M1(a) fails fast or just restarts.

**Status:** DONE_WITH_CONCERNS
**Summary:** 9.0/10. Clippy clean, 250/250 core tests pass, and the new dial test is stable (0.36-0.51 s against a 1.5 s limit, including under CPU load). Cancellation and the 10 s budget hold, and the CLI JSON change breaks nothing. Main concern: the dial also restarts when addresses are removed (peer goes offline or the set shrinks). That can cancel a working handshake and possibly fail the dial at once.
**Concerns/Blockers:** M1: restart only on new addresses. L1: `biased;`. L3: harness ssh timeout and fail after the ready loop. Not blocking merge on correctness grounds.

## Re-review (845373d, e90929e; head e90929e)

Score: **9.5 / 10**. No open medium findings.

### Evidence
- `cargo fmt --all --check`: clean. `cargo clippy -p fsync-core -p fsync-cli --all-targets -D warnings`: clean.
- `cargo test -p fsync-core`: 251 passed, 0 failed (250 before, plus `dial_ignores_addresses_removed_mid_dial`).
- `dial_follows_addresses` (2 tests): 15/15 passed with no load (binary 0.77-0.87 s); 8/8 passed under 2x nproc busy loops (0.81-0.89 s). Not timing-flaky.
- Target dir `~/.cache/fsync-p03-rereview-target` deleted.

### Findings, status
- **M1: fixed.** `dialer.rs` restarts only when `!new.addrs.is_subset(&old.addrs)`. Removals, a shrinking set and an offline (empty) entry no longer cancel a handshake. A relay URL change still counts, because the relay is a `TransportAddr` in the same set. A peer restart on a new port adds an address it did not know, so that still restarts the dial. The coordinator's finding that iroh 1.3 does not fail a dial given empty addresses (the old `!=` check passes the new test) answers my unresolved question: M1(a) would not have failed fast. Only a wasted restart remained, and that is now gone.
- **L1: fixed.** `biased;` is set, with the connect branch first.
- **L2: fixed.** The dialer comment, ADR 014 and the changelog no longer mention mDNS.
- **L3: fixed.** `timeout 20 ssh`, a string match (no `pipefail` / SIGPIPE risk), and an explicit `fail` after 240 tries. `|| true` inside `$(...)` keeps `set -e` safe.
- **L5: fixed.** The test asserts against `iroh::NET_REPORT_TIMEOUT`.
- **L4: accepted as is.** The coordinator's reason is reasonable: the per-stage line shows the missing `clip_logged`.

### Remaining (informational, non-blocking)
- `dial_ignores_addresses_removed_mid_dial` does not tell the subset check from the old `!=` check: both pass. It guards that "the dial survives a removal", not that "a removal does not restart the dial". If a discriminating guard is wanted, count restarts (for example a `#[cfg(test)]` counter, or assert on the debug log line).
- `dialer.rs:134`: the doc comment now has two parentheticals in a row ("(removals never restart it) (a clip sent ...") and a line over 100 characters. This is wording only.

**Status:** DONE
**Summary:** All requested fixes are verified. M1, L1, L2, L3 and L5 are resolved, and L4 is reasonably accepted. fmt and clippy are clean, 251/251 core tests pass, and the dial tests are stable under load. 9.5/10.
**Concerns/Blockers:** None. Optional: make the removal test discriminating, and tidy the dialer doc comment.
