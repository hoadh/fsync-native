# Code review: N8 refusal ack + CLI send (round 1)

**Status:** DONE_WITH_CONCERNS (gate not met)
**Score:** 9.0/10, 0 critical, 1 high. The gate needs ≥9.5, so this fails until the flaky test is fixed.

I did not write the report file. This run's rules say to return findings as the final message and not to create report files. Copy this text to `/Users/alex/Code/fsync-native/plans/reports/code-reviewer-261016-1609-n8-refusal-ack.md` if you want it on disk.

## Verification
- **Clippy:** `cargo clippy --workspace --all-targets -- -D warnings` is clean in the worktree.
- **Clip tests:** `clip_refusal_feedback`, `clip_protocol`, `clip_security` and `security_limits` all pass on the first run (23 tests).
- **Change B:** `cargo test -p fsync-cli` passes 6 tests, and clippy on `fsync-cli` is clean.
- **Flake:** I ran `clip_refusal_feedback` about 90 times. `removed_sender_is_told_refused_on_every_try` failed twice, about 2%, at try 0. The assertion was "reported delivered: delivered:[A], refused:[]".

## High
**H1. `removed_sender_is_told_refused_on_every_try` is nondeterministic.** The dev report says it is "Deterministic, no sleeps". It is not.
- The test calls `a.trust.ingest(...)` and then publishes straight away.
- The connection close only happens in the async enforcer task (`node_builder.rs:113-117`: `changes.changed()` then `g.enforce()`).
- If `b` publishes on the warm connection before that task runs, `a` has no trust gate on the stream and no per-offer trust check. So `a` accepts the offer, runs `on_offer`, and acks 1. The sender then correctly reports delivered.
- The cold test passes because there is no cached connection.
- Fix, either of these:
  - (a) In the test, wait until `a`'s warm connection to `b` is closed before publishing. Poll `Gate::live` for a bounded time, or send one probe and wait for the close.
  - (b) Add `if !self.trust.is_trusted(&from)` in `ClipHandler::accept` or `on_offer`. On failure, close with `CLOSE_UNTRUSTED` before any app processing, so the window disappears in production too.
- I recommend (b), and (a) if you want to keep the test as a pure sender-side test. The window was already there with clip/1 and is milliseconds wide. It matters more now because the ack makes "accepted from a just-removed peer" explicit.

## Medium
- **M1. Rate-limit ack can be lost.** The receiver calls `rx.stop(0)` and then sends the ack.
  - On the sender, `write_all` or `finish` can fail with a stopped-stream error if the stop arrives first.
  - That error maps to `CoreError::Net`. The sender then closes and forgets the connection and retries on a fresh dial. Before, it surfaced as `Invalid` with no retry.
  - Frames are small, so this is rare. The flood test passes, and the effect is only an extra dial.
  - Suggestion: in `push_and_wait_ack`, treat a `Stopped` write or finish error as "read the ack anyway", or map it to `Invalid`.
- **M2. Change B hides the real error when no pull starts.** Every `wait_event` error maps to "did not start fetching the offer". That includes "event channel closed". Use `.context(...)`, or only rewrite the timeout case.
- **M3. Change B can fail while the peer's user is still deciding.** If a receiver needs a manual accept before pulling, `send` fails after `max(wait, 10 s)`, and the offer stays outstanding. Confirm the CLI receiver auto-accepts. Document the 10 s floor.
- **M4. Release note for the interop break.** `docs/protocol-spec.md` and the changelog already say "no compatibility; clip/1 peer fails the handshake". Add one user-facing line that the Mac and Windows apps must update together, because clip stops working across versions. Pre-release this is low cost.

## Checked, no problem
- **Ack ordering:** the receiver acks only after `on_offer` finishes. That covers the log keep, `record_received` and the event emit. Duplicates ack 1, and any error acks 3.
- **Gate order:** the gate still runs in `Gated::accept`, before the handler.
- **Information to a refused sender:** it learns only the 403 close, which it already saw. A rejected offer returns a bare byte (3), with no detail.
- **Deadlines:** one deadline covers connect, send and ack. A misbehaving peer is bounded by `FIRST_ATTEMPT` plus `send_timeout`. A clip/1 peer fails the ALPN handshake fast, and the test asserts that.
- **Refusal retry:** a refusal on the cached connection is retried once on a fresh dial, and the first connection is closed and forgotten.
- **Receiver ack write:** it is one byte, sent inline in the per-connection accept loop. A peer that withholds flow-control credit could stall only its own connection, so I'm not flagging it.
- **`NotPaired` reuse:**
  - This is acceptable and avoids FFI churn. The Swift `ErrorText` shows "Something went wrong (not-paired)" for it. That text is generic but unchanged from before, and Windows has only the generated `NotPaired` exception.
  - No Swift or C# changes are needed, and the Swift generated file is unchanged.
  - A dedicated `Refused` variant would let the apps show "removed on the other device". Defer it, because it needs regenerated bindings and a Windows build.
- **FFI fold:** `refused` folds into `failed` in the FFI `Published`, which is fine since no app reads it.
- **Regression proof:** the dev reports 2 of 3 tests fail without the fix. I did not repeat that, but the flaky failure message matches the old-behavior symptom.
- **Change B `wait_event`:** reusing `&mut on` across calls is correct, and it compiles.
  - `Started` returns `None`, and a fast `Done` or `Failed` is queued in the channel, so it returns `Some(Some(..))` on the first call. No event is missed.
  - Progress means the peer is pulling. The loop then waits up to 3600 s per call for the final state.
  - With `wait_online` removed, `send` retries on `PeerOffline` until `--wait`. `NotPaired` propagates immediately.
- **File sizes:** all touched Rust files are under 200 lines except `security_limits.rs` (295) and `clip_security.rs` (204). Both are tests and well under the 500 hard limit.

## Required fixes
1. Fix H1: add the trust check in the receiver and/or make the test wait for the enforcer to close the connection. Then re-run the test at least 100 times without a failure.
2. Optional but quick: M1 and M2. Add the M4 changelog line.

With H1 fixed I would rate this about 9.6.

**Unresolved questions**
- Should `on_offer` re-check trust per offer, as defence in depth against the enforcer lag?
- Does the CLI receiver always auto-accept pushed files, so the 10 s start deadline is safe?
## Round 2 (commit 842d9b2 + change B tweaks)

### Verified
- clippy `--workspace --all-targets -D warnings`: clean (worktree); `fsync-cli` clippy clean (main).
- `clip_refusal_feedback` (4 tests) x60: 0 failures. `rate_limited_offers_are_failed_not_refused` x25: 0 failures. `offer_flood_from_one_peer_is_throttled` x25: 0 failures. `clip_protocol`, `clip_security`: pass. `cargo test -p fsync-cli`: 6 pass.
- H1 fixed: `ClipHandler::accept` checks `trust.is_trusted(&from)` right after `accept_bi`, before the rate limiter, any read/parse, `on_offer` or ack. On failure it closes with 403 and breaks, so the sender sees `PeerRefused`. The enforcer-lag window is closed in production too.
- M1 fixed: `WriteError::Stopped` no longer maps to Net, `finish` error ignored, ack still read, so a rate limit maps to `Invalid` with no redial. New test is race tolerant (asserts delivered>0, failed>0, no refused, sum=60); it asserts no timing.
- M2 fixed (only the "timed out" message is rewritten). M3 comment added. M4 "Upgrade" line added to changelog.

### Remaining (low, non-blocking)
- L1: M2 matches the wait_event error by the string "timed out" (brittle if that message changes). A typed error or a `Duration`/deadline check would be sturdier.
- L2: rate-limit test never proves the Stopped branch was hit (stop may not race); it guards the outcome, not the path. Acceptable.
- L3: the new trust check closes the whole connection on the first untrusted offer (right behavior), `break` drops the in-flight `tx` without ack, covered by the refused tests.

### Score: 9.7/10, 0 critical, 0 high.
### Status: DONE (gate met)
