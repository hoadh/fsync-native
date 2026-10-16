# Netns Matrix, Core Hardening, and the N8 "Delivered" Lie

**Date**: 2026-10-16 17:10
**Severity**: High
**Component**: tests/e2e (netns matrix), fsync-core (clip/2, fetch, lifecycle, path tier), fsync-cli (send), FFI semantics for `Published`
**Status**: Ongoing. N8, Track K and the CLI send fix are merged to main 026fb5a. N3 and N4 are still quarantined. Windows app not rebuilt for clip/2.

## What Happened

Phase 15 ran as three parallel tracks on main 68d7ae6. Track N built an 11-scenario netns matrix in a privileged Linux container. Track K hardened core. N8 fixed the sender being told "delivered" for a clip the receiver had refused. Final gates on 026fb5a: cargo fmt and clippy clean, `cargo test --workspace` 354 pass / 0 fail, run twice. FsyncCoreKit 14/0. FsyncMac 231 executed, 18 skipped, 0 fail. p0 MIB=16 PASS. Netns non-strict rc 0: PASS n1 n2 n5 n3b n6 n7 n8 n9, QUARANTINED n3 n4, SKIP sec. Strict rc 1, red only from n3 and n4.

## The Brutal Truth

The worst one was N8, and it was a lie in our feedback path. After A resets trust, B's `clip put` reported `delivered` in 2 of 3 tries. A stored nothing, so security held, but the sender was told a clip arrived when it was dropped. We treated transport ack as app acceptance. That is wrong, and I should have seen it when the 403 close was written after the handshake.

The netns "exit 0" also misled us. Non-strict mode returns 0 with two gating scenarios quarantined. Track N's own reviewer scored it 8.8 because of that. Read the JSON, not the exit code.

## Technical Details

- N8 root cause: `clip/send.rs` `attempt()` treated `tx.stopped() == None` as delivered. `net/gate.rs` closes untrusted peers with 403 only after the handshake, and QUIC has already acked the stream data.
- Fix 6f2402e: clip/2 ALPN, app-level ack byte. 403 (`CLOSE_UNTRUSTED`) maps to `CoreError::PeerRefused`, surfaced as `NotPaired` for single-target pushes. Broadcast `Published` carries `refused`.
- 842d9b2: per-offer trust re-check on the receiver. This fixed review H1, where `removed_sender_is_told_refused_on_every_try` was nondeterministic: the close ran in an async enforcer task, so a warm connection could still deliver. Review 9.0 with one High, then 9.7.
- Track K K1: `fetch_after_peer_restart` RED at `Elapsed` after 25.37 s on unfixed code, GREEN at ~2.5 s. Closing the redial alone was not enough. Dead inbound conns from the crashed peer kept routing dead; redial took 45+ s until `Gate::close_stale_inbound` ran.
- 1ab0594: CLI send dials instead of waiting on passive presence. Before it, N2 failed with `send failed: error: devB did not come online` after 140 s. It is now bounded by max(`--wait`, 10 s).
- K backend-down start: offline fallback only on connect, timeout or DNS errors (`CoreError::Unreachable`), within 7 days of the last list sync. Stamp rejects future dates beyond 5 min.
- Review scores: Track K 8.6, 9.2, 9.3, 9.6 (gate 9.5 met on round 4).

## Decisions

- **No clip/1 shim.** Pre-release, so back-compat is YAGNI. Mac and Windows apps must update together. Mixed-version peers fail fast through ALPN, covered by `old_clip_alpn_peer_fails_fast_not_hangs`.
- **Reuse `NotPaired`.** No new FFI error variant, so no FFI enum change. The cost is that the UI cannot say "removed on the other device".
- **Quarantine, don't fake green.** N3 and N4 stay quarantined with reasons in the JSON. We did not tune the test to pass.

## Lessons Learned

- "Delivered" means the receiver accepted at app level. Transport ack is not that.
- Do not gate sends on presence. It is advisory and goes stale.
- A test that says "deterministic, no sleeps" must be checked. H1 existed because a race was hidden behind async enforcement.
- Non-strict green with gating quarantines is not green. Print the counts and fail on gating quarantine.
- The negative check paid off: N1 expecting relay FAILed with `expected relay, got lan`.

## Next Steps

- **Lead**: commit `docs/test-results/netns-261016-1643.json` and `-1649.json`. They are still untracked and the git_sha is `026fb5a-dirty`. Before next merge.
- **Track N owner**: N3 needs a test relay CA and QAD hook. N4 needs early relay URL in Hello, ~4 s first connect vs 1.5 s spec. Before the next netns run.
- **Windows owner**: rebuild the app and run `dotnet` tests against clip/2. Not run since the wire change.
- **Core owner**: decide whether to add a dedicated `Refused` FFI error for UI text. Needs an FFI contract bump, so schedule it deliberately.
- **Lead**: sign off K `FIRST_ATTEMPT` 5 s (spec 2 s) and the 20 s regression bound (measured 11.6 s, spec <10 s).

**Status:** DONE_WITH_CONCERNS
**Summary:** Track K, N8 clip/2 refusal ack and the CLI send fix are merged to 026fb5a with all gates green. Netns matrix has 8 PASS, 2 quarantined, 1 skip.
**Concerns:** N3 and N4 remain quarantined, so strict netns is red by design. Windows clip/2 is unverified. Netns JSON is not committed. Open questions: N3 relay CA and QAD hook, N4 first-connect latency, dedicated `Refused` FFI error, K timing sign-off, Windows rebuild for clip/2.
