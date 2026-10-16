# Round 2 review: Track K (6fed9d9, 3ab3888) — score 9.2/10, 0 critical, 0 major, gate (>=9.5) NOT met

Round 1 (8.6): majors 1-3 addressed (see below). Remaining items are minor/medium.

## Round-1 majors
1. abandon closing shared conn: FIXED. Evicts cache entry only if stable_id matches; closes only if conn-wide udp_rx unchanged since mark taken before the request. noq aggregates per-path rx at datagram handling (transport-agnostic, so relay bytes count). Check-then-act (lock released before stats read) is benign: worst case a conn is spared and stays uncached/idle. Cannot cut healthy shared conn unless it received zero bytes for >=5 s incl. keepalive/acks; then a 2nd, genuinely silent conn is indistinguishable from dead. Residual: slow server (>5 s, no other traffic, keepalive not yet due) is abandoned and the request re-sent once (idempotent). Accepted.
2. close_stale_inbound over-kill: PARTLY fixed (narrowed, see item 2 below).
3. 2 s false-dead: FIXED (5 s, only Timeout or Net+conn closed retries). Test with 3 s serve_delay proves it.

## 2. Redial-stall inbound close (deviation)
Trigger: only after the first 2 s dial attempt times out (not on the fetch handshake). It does NOT prove the peer restarted: slow hole-punch, relay hiccup, network switch, peer asleep also stall a dial for 2 s. So it fires in more cases than "peer restarted". Weaknesses:
- Sparing is `ALPN_FILE` only. An inbound clip receive / any other moving stream is cut. Cheap narrowing: spare any inbound conn whose rx advanced, regardless of ALPN (presence idle conns have no growth, still closed).
- Marks are taken before the dial; a conn opened after the mark (new pull during the 2 s window) is not in `marks` -> counted "not moving" and closed. Fix: treat unmarked conns as moving if their opened time > mark time (or mark everything lazily at close time with a short sample).
- Closed conns reconnect on next use, so cost is bounded; acceptable as a second-stage recovery given the measured iroh behaviour (stale path persists while ANY conn exists). Narrower trigger idea: only after the retried second attempt (DIAL_TIMEOUT remainder) also stalls, or when the stalled dial's cached outbound conn had zero rx since mark. Medium priority, not blocking.

## 3. 3ab3888 cached-list fallback
- Gate is right: only CoreError::Net|Http, needs pinned list that trusts self; Api errors (auth/4xx), nothing-pinned still error. Test covers both. FFI uses same Inner::ensure_account: identical. 
- Security: serving from the cached list does not weaken revocation beyond status quo (revocations are learnt only by sync anyway). But it is fail-open to an attacker who can block/forge the backend: Http includes TLS config/handshake failures, "bad response body", "response too large"; a MITM or blocked backend freezes revocation indefinitely and the peer set keeps working. Mitigations (medium/minor): restrict Http fallback to connect/timeout errors (not bad body/TLS), bound staleness (e.g. warn/limit after N days), surface "stale list" state in status. Warn log exists. Also sync_device_list partial-failure path falls back after possibly already-ingested data: fine.
- Tests are real backend stopped, deterministic.

## 4. Tests determinism
- fetch_shared_connection: 300 ms sleep + 1500 ms "settle" sleeps are timing-dependent. The silent-conn assert relies on no packet (PATH probes, tokens, keepalive) arriving in the window between mark and abandon; it is called immediately after `stats()` so window is ~0 ms and the sleep is only to let the tail drain BEFORE the mark; a slow CI could still deliver a late packet between settle and mark but the mark is read at call time, so race is only inside the same expression: practically deterministic, but the sleep is a smell. Better: loop until `udp_rx.bytes` is stable for 200 ms (poll), then abandon. Same for 300 ms ack wait (poll until bytes grow, with deadline).
- fetch_after_peer_restart now takes 5.7-7.7 s with 10 s bound: margin 2-4 s, may flake on loaded CI; consider asserting < 15 s (outer 25 s already guards the hang).
- Sibling-survives-abandon is tested at Dialer level, not through two real concurrent fetches (justified).

## 5. ~4 s first connect "not the dialer"
Credible and well measured: relay dial 5-14 ms, delay is iroh publishing relay URL after net report (3 s without QAD on the local test relay); forced relay skips it. Agree no dialer change. Caveat: derived on the in-process relay; confirm once against a production relay with QAD (expected ms) before closing N4. Early-relay-URL-in-Hello is a risky change, defer.

## Other
- DRY fixed: PathTier = selector Tier re-export (made pub); loopback pre-check only. Selector semantics unchanged. Clippy/fmt/workspace tests reported green by dev; I did not rerun them this round.
- CLI help documents path vs path_tier: ok.

Status: DONE_WITH_CONCERNS
Summary: Round-1 majors fixed; abandon heuristic sound; K1 redial-stall inbound cleanup still broader than needed; fallback is fail-open on forged/blocked backend.
Concerns: ALPN_FILE-only sparing + marks miss conns opened after mark; Http fallback includes TLS/body errors with no staleness bound; sleep-based settle in test; restart test margin.
Score: 9.2/10 (0 critical, 0 major) — gate not met by 0.3

---
# Round 3 review (8f2c6e7, 1bad0ac) — score 9.3/10, 0 critical, 0 major, gate NOT met (0.2 short)

Re-ran: fetch_shared_connection 3/3 (6.4 s), service_backend_down 3/3, fetch_after_peer_restart 1/1 (11.6 s), lib reachability+list_freshness 8/8. Not re-run: workspace, clippy.

## 1. Staged escalation
Justified. Stage 0 (after 2 s) own outbound; stage 1 (4 s) idle inbound; stage 2 (6 s) in-use non-file inbound; file/1 with traffic always spared. Evidence (dev measured 5/5 failing with the narrower rule: fresh conn from restarted peer keeps stale path) is consistent with iroh behaviour. Unmarked (post-mark) conns are treated as in use: spared at idle stage, closed at last stage unless file/1. That is exactly the fresh restarted-peer conn, so the deviation is necessary. Bounded: only after ~6 s of consecutive stalled dials, never on a fetch handshake timeout. It is evidence of "peer unreachable at known addresses", not proof of restart; cost = clip/presence conns reconnect on next use, in-flight non-file receive from that peer is cut. Accepted. Side effect: restart recovery now ~11.6 s (was ~6 s); test bound 20 s is OK but user-visible hiccup grew. Minor: unmarked file/1 conns are spared even if idle (cannot be distinguished), harmless.

## 2. Unreachable classification
Sound and fail-closed: timeout, or is_connect + io kind refused/reset/aborted/timeout/unreachable/addr-unavail, or "dns error". TLS/cert, bad body, too large, HTTP status (Api) cannot enter it; garbage-TLS test proves Http. All requests pass through `check`, so classification is uniform (client.rs Http uses are builder/refresh-conflict only). Residuals:
- An on-path attacker can still produce RST/blackhole/timeout (and DNS failure) -> Unreachable; unavoidable, the 7-day bound is the real defence. `is_timeout` also covers a mid-body read timeout; same reasoning.
- Availability trade-off (minor): 502/503/504 from a live load balancer with the app down is Api, so start fails offline although it is "backend down". Consider allowing 502/503/504 inside the bound (attacker gains nothing over dropping packets).
- String match "dns error" is brittle across hyper-util upgrades; covered by a test (unresolvable host) so a regression would be caught.

## 3. Stamp freshness — REAL GAP (medium, fail-closed so not a security bug)
Stamp written only by ensure_account (Created/Ready) and remove_device. sync_device_list also runs from the presence pump on DeviceListChanged (and ListBackend::sync) without stamping. ensure_account is called by apps at start/login, not periodically. So a daemon/menubar app online 8+ days (list kept current by pushes, backend reachable) then restarted while offline has a stamp older than 7 d -> refuses to start though the list was confirmed minutes ago. Fix: stamp on every successful sync_device_list (pump + remove_device + pairing), and ideally while the presence WS is connected/authenticated (a connected push channel means the list is current), e.g. stamp on Connected and on each successful Ingest incl. Unchanged. Rate: medium (availability regression for exactly the long-running users the 7 d bound targets); not a gate-blocker alone but it is what keeps this at 9.3.
Related stamp semantics:
- Old install, no stamp: clock starts at first offline use, granting a fresh 7 d on a list of unknown age (could be months old). One-time grace, acceptable but weaker than needed; safer lower bound = mtime of the trust store/pinned list file, or refuse and require one online start.
- Clock skew: `now.saturating_sub(last)` -> a future-dated stamp (garbage large number, or clock set back) yields age 0 forever. Reject last > now + small slack (treat as unreadable/never). Minor (needs local clock/file control, which is already high privilege).

## 4. Stamp file write/errors
write_private_atomic: 0600 create_new temp, write, fsync, rename, dir fsync, tmp cleanup on error. Good. Failure only warns and shortens the window (fail-closed): correct. Unreadable/garbage -> None -> restarts clock (see grace above). offline_ok writes inside a read path; fine. Concurrent stamps race harmlessly (unique tmp names).

## 5. Test determinism
until() (20 ms poll, 10 s deadline) and quiet() (250 ms unchanged window, 10 s cap) are condition-based; remaining timing: quiet assumes no packet within a 250 ms window which a loaded CI could violate only to loop again (retries until cap) — fine. Service tests write the stamp file directly with now±N days: deterministic. Restart test is wall-clock (11.6 s of 20 s): acceptable margin. 5/5 stability claimed by dev; 1 run each confirmed by me.

## Recommended before merge
1. Stamp from presence-pump sync success / connected push channel (item 3).
2. Reject future-dated stamps.
3. Optional: 502/503/504 within bound; mtime-based start for no-stamp installs.

Status: DONE_WITH_CONCERNS
Summary: Escalation and Unreachable classification are sound and evidence-backed; stamp refresh coverage is too narrow for long-running apps restarted offline.
Concerns: stamp not refreshed by pushes/pairing (8+ day uptime then offline restart fails); no-stamp grace and future-dated stamp; 5xx treated as non-offline.
Score: 9.3/10 (0 critical, 0 major) — gate not met

---
# Round 4 review (3de20ec) — score 9.6/10, 0 critical, 0 major, gate MET

Verified: service_backend_down 4/4 (incl. presence_resync_refreshes_the_stamp...), lib list_freshness 8/8, `clippy --workspace --all-targets -D warnings` clean, FFI diff empty.

## Round-3 items
- Stamp gap: FIXED. `sync_device_list` = fetch_and_ingest + `trust.mark_synced()` on success only (errors propagate before stamping). Single choke point covers presence push, startup, pairing initiator, remove_device. `mark_synced` runs after ingest returns, outside trust locks (no deadlock). OnceLock callback, first registration wins: fine. Remaining nit: stamp does a sync_all file write on the async thread per push; push rate is low, negligible.
- Future-dated stamp: FIXED (>5 min ahead -> invalid; boundary tested both sides; future db mtime refused too).
- No-stamp grace: FIXED (no fresh window; clock from history.db mtime; no db -> fail closed; fallback does not write a stamp, tested).
- 502/503/504 documented fail-closed with a sound reason (intercepting proxy can answer them). Accept.

## Dev's own concerns
(a) db mtime moves with history writes: acceptable, minor. Only a stampless legacy install that has never completed a backend sync since upgrade, and the first successful sync replaces it with a real stamp; SQLite in WAL mode moves the main file's mtime only at checkpoint, so it errs conservative or lags. Exploiting it needs local write access (already game over). Optional hardening: on first fallback use persist the mtime as the stamp so the clock is frozen (one write, no new grace).
(b) Joiner paired via a peer, no backend sync yet: falls back to db mtime (~pairing time), so it works offline up to 7 d from pairing, then fails closed until one online start. Correct and conservative; a peer-verified chain is not backend confirmation, so do not stamp on pairing. Accept.
(c) manager.rs exactly 200 lines: not a violation (current rule is 500 hard limit; the 200 guideline is "consider modularizing"). set_on_synced/mark_synced could live in a small trust module later; not required.

## Residuals (informational)
- Test relies on wait_for polling of the stamp file; condition-based, deterministic.
- Overall Track K now: K1 recovery bounded and evidence-backed, K2 warning ok, K3 display fields reuse selector Tier, offline start fail-closed and bounded.

No required fixes.

Status: DONE
Summary: Stamp now refreshed at one choke point; legacy/future/no-db cases fail closed; tests, clippy green.
Concerns: none blocking (optional: freeze legacy mtime into the stamp on first fallback; blocking fsync write in async context).
Score: 9.6/10 (0 critical, 0 major) — gate met
