# Code review — M5 trust-reset API (round 1)

**Score: 7.5/10 · Critical: 0 · High: 0 · Medium: 6 · Low: 7** — below auto-approve gate (9.5) → fix loop.

## Security invariants (verified OK)
- No pin change without explicit `confirm_trust_reset`; account-id + `verify_chain_from_genesis` retained (`trust/reset.rs`).
- Step-up: 401 `reauth_required` → `ReauthRequired` mapped before generic 401; FFI round-trip; tested.
- Pin-drop on self-removal safe: no pin → `NeedsPairing`, no TOFU.

## Medium
- M1 prompt info thin (count + hash only) → add device names to `TrustResetInfo`/event.
- M2 id binds root, confirm pins latest head → server can extend chain after prompt. Bind confirm to announced head or re-prompt on change.
- M3 pending/prompted/declined cap = 2 → junk roots evict declined (re-prompt) or legit pending. Enlarge/separate declined cap.
- M4 `reset_account_trust` no local account pre-check before server PUT; local failure after server success leaves desync. Pre-check + retryable local step.
- M5 (plausible) pinned initiator may see own reset announced via DeviceListChanged push between PUT and replace.
- M6 tests missing: confirm `Ready` branch, WrongAccount, declined eviction, sync.rs unit, confirm/decline race → `Invalid` instead of `NotFound`.

## Low
- L1 sync.rs full-fetch correct/loop-free; declined devices pay 2 req/sync forever → skip when server root already declined. Note Rollback-now-surfaces in changelog.
- L2 in-memory pending/declined acceptable (fails closed); CLI decline doesn't persist.
- L3 bump `contract_version` to 2 (event field + error variant = Swift source break).
- L4 `parse_reset_id` overbuilt, silently trims.
- L5 `is_self_removed` returns true on poisoned lock.
- L6 `pending_reset_infos` silently drops undecodable entries.
- L7 test uses `mdns: true` unnecessarily.

## Unresolved questions
- Does backend push DeviceListChanged to initiator on reset?
- Is the 3/day reset limit consumed on failed/lost response?
- Persist declined hashes for CLI?

---
# Round 2 — 8.5/10 · Critical 0 · High 0 · Medium 1 · Low 5
All round-1 findings verified fixed (M1–M6, L1, L3–L7); 21 trust-reset tests pass; head-hash id gives no consent bypass.
- N1 (Med) superseded pending roots never pruned → cap 8 can silently drop a legit reset; stale roots confirmable. Fix: drop pending entries of other roots on new announce.
- N3 is_own_fresh_root lacks account_id check.
- L1 unthrottled re-announce events on head growth (UI churn only).
- L2 own-reset flag is bool → concurrent resets clear early; use counter / serialize.
- L4 confirm_reset clears all pending — undocumented.
- L5 restart_presence error after successful pin returns Err → retry gets NotFound.
- T1 repeated-reset test mostly hits no-PUT shortcut; no pin-failure/retry test; no wrong-account/multi-device test for is_own_fresh_root.
- Note: mDNS test lock is per-binary (nextest could collide) — comment it.

---
# Round 3 — 9/10 · 0 Crit/High/Med · 3 Low (overlapping-guard test, pending_resets naming, trust_manager.rs 304 lines) → fixed by lead.
# Round 4 — 9.5/10 · 0 Crit/High/Med · 4 doc Lows → AUTO-APPROVED
- L1 ADR 012 bullet nesting, L2 changelog stale "root hash" wording, L3 ffi-contract "lowercase" vs parser accepting uppercase, L4 foreign roots ignored during own reset undocumented → all fixed in docs.
