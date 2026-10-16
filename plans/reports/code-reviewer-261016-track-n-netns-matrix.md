# Review: track-n/netns-matrix (68d7ae6..HEAD, 3 commits)
Score 8.8/10. 0 critical, 3 major, 9 minor. Gate (>=9.5) NOT met.

Checked: shellcheck, bash 3.2 syntax, committed JSON, and a live run. The live run was `N1_EXPECT_PATH=relay NETNS_MIB=16 NETNS_SCENARIOS=n1,n5`. N1 FAILed with "expected relay, got lan", so the path assertion bites. N5 PASSed (relay 39.6 MB for 16 MiB). Cleanup left no fsync-netns-* containers or networks, and mystifying_euclid was untouched.

## Major
1. Coverage is thinner than "exit 0" suggests.
   - The committed run has 4 of 11 QUARANTINED (n2, n3, n4, n9) and 1 SKIP (sec). Only n1, n5, n3b, n7, n6 and n8 really pass.
   - n2 and n9 are gating=1 but never exercise the scenario. They quarantine at `probe_offline_start`, so offline-start behaviour, mDNS with the WAN down and the backend-down path are untested.
   - Quarantine (rc 78) never sets `overall`, so the run exits 0. The runner prints no PASS/QUARANTINED/SKIP count.
   - Fix: print counts at the end. Optionally add `STRICT=1` so quarantine exits non-zero on gating rows. Every quarantine does carry a reason: `metrics-write.py` raises on a SKIP/QUARANTINED row without notes.
2. N8 is nondeterministic, and its notes contradict the recorded JSON.
   - The quarantine text says "clip put exited 0 ... reproduces in the full matrix, not when N8 runs alone". The committed run shows N8 PASS with B's put refused in 20 s.
   - So the same code yields PASS or QUARANTINED depending on ordering or state, and the cause is not diagnosed. That is a possible product bug (silent-success sender) or a test race.
   - The negative checks are sound: A's watch is `ready` first, a 2 s late-delivery window follows, and a file push is also checked.
   - Fix: find the cause, or make the outcome stable, e.g. record `put_rc` in notes on both branches.
   - Weak spots: `history --query ... || true` passes vacuously if the history call errors. `--query` does exist (args.rs:167). Assert rc==0.
3. Provenance and schema accuracy.
   - `netns-261016-1411.json` has `git_sha: 68d7ae6`, the base commit. The run was on a dirty tree and not on the commit that adds the matrix.
   - `cli_path` is null in every row, yet README documents it ("Lan is printed for any IP path"). The `path` field on `sent` lives in track-k (6f13d0a), not in this base.
   - Fix: re-record at HEAD, or document `cli_path` as "null until the CLI emits path". Also document the clip-latency and `mixed` semantics (see Minor).

## Minor
- netns-matrix.sh:32: `exit $rc` is a real SC2086 (info). Quote it. SC2329 on the trap-invoked `cleanup` is a false positive.
- run-in-container.sh: the SC2034 warnings (API, RELAY, CLI, EXTRA) are false positives because the variables are consumed by sourced libs. p0-exit-demo.sh:113 has the same kind of false positive. A scoped `# shellcheck disable=SC2034` would make the run clean.
- run-in-container.sh:81: `$(cat "$PIDFILE")` gives SC2013 (info).
- No host-side bound on the privileged runner when netns-matrix.sh is run directly.
  - Only run-matrix.sh adds CELL_TIMEOUT_S (1800), and the trap handles TERM.
  - The per-scenario SCN_TIMEOUT_S (300) does not cover the build or N6 (1024 MiB).
  - A SIGKILL leaves a privileged `fsync-netns-run-*` container behind. `docker run` has no `--rm` or `--stop-timeout`.
  - Suggest `timeout`-style self-limit inside the container (a background `sleep N; kill 1`).
- N6 address restore lives only in the scenario's EXIT trap. The runner's between-scenario reset (wan_up, nat_mode, use_pair) does not restore h-la's address. If the subshell is SIGKILLed or the trap fails, N8 runs on 192.168.10.12. Add the restore to the runner reset as well. In n6 the `[[ $i -lt 300 ]]` check misfires if the threshold is hit at exactly i=300.
- `poll_b_online` (N7) has no sleep, so it relies on `cli devices` latency (it works, but it is a busy loop).
- N7's "put must not report delivery" treats any non-zero exit as proof (a crash passes). N8 treats any `cli B send` failure as proof of refusal, with the same weakness. Match on the specific error or event.
- classify_path thresholds are sound: relay>=80% of size is relay, <10% relay and <10% WAN with host>=size is lan, <10% relay and >=80% WAN is direct, everything else is `mixed` (which fails every expectation). They are unreasonably tight for tiny files (noise ~30 KB vs the 10% threshold on size >= 1 MiB; fine at 16-64 MiB). Document the thresholds in the README.
- N3 uses `path_round n3 -` plus a case. A "mixed" outcome gives FAIL, not a quarantine. Correct, but undocumented.
- p0 refactor: behaviour is preserved (same sequence: clip, file, via assertion). Small differences: the login OTP wait is now a retry loop instead of `sleep 0.5` (an improvement), and the p0 result line lost `send_mib_s` and `blake3`. A leftover double blank line remains. `lib/` is bash 3.2 safe (the `${!v}` indirect expansion works).
- Cache volumes `fsync-netns-cargo/rustup/build` have fixed names, so concurrent runs share them (the names are still in scope). Docs: the README says `duration_s` excludes build, which matches the code.

## Verified OK
- Docker safety: `rm -f -v` only on the `fsync-netns-(pg|run)-$RUN` names, `network rm` on `fsync-netns-$RUN`, and `volume rm` only on the 3 named cache volumes under CLEAN_CACHE. No prune or wildcard. Labels are set on the image, network and containers. The repo mount is `:ro`. The only credential is `postgres:e2e`, a local test default. All namespace and iptables changes stay inside the container.
- The `--privileged` runner is scoped to one named container whose cleanup is trapped on EXIT/INT/TERM.
- The per-scenario watchdog (SCN_TIMEOUT_S) works. Between scenarios the runner resets: CLI processes, WAN links, NAT mode, backend restart, and `EXTRA` and `PIDS` (per-subshell). `rc>128` maps to FAIL. n9 stops the backend and the runner restarts it. N8 runs last because it consumes the pairing.
- Honesty: every PASS goes through sha_eq, `assert_eq` on the observed path, and clip text/timestamp checks. SKIP/QUARANTINED without a reason aborts the JSON write. The JSON is validated with `json.load` before copying.
- bash 3.2: only netns-matrix.sh and run-matrix.sh run on the host. Both pass `/bin/bash -n` (3.2.57). `${PIPESTATUS[0]}`, `read -a` and arrays are used compatibly. Everything else runs in the container.
- Sizes: all files <=125 lines. DRY: the shared helpers were extracted properly.

## Unresolved
- Why does N8 differ between a standalone run and the full matrix?
- Should QUARANTINE on a gating scenario fail CI, or is a visible count enough?
- Is it a product bug that the CLI cannot start offline (n2/n9)? That should be a ticket with an owner.

**Status:** DONE_WITH_CONCERNS
**Summary:** No critical issues. Docker safety, path classification and the p0 refactor are sound. The path assertion and cleanup held in a live rerun. The gate fails on three majors: weak coverage reporting (4 quarantined, 1 skipped), unexplained N8 nondeterminism, and stale or incomplete provenance and schema (`git_sha` is the base commit, `cli_path` is always null).
**Concerns:** see Major 1-3.
**Score:** 8.8/10, gate not met.

---
# Round 2 (0140eae, d8331a5, 4920606)
Score 9.5/10. 0 critical, 0 major, 5 minor. Gate met (>=9.5, 0 critical), at the threshold.

## Previous majors
1. Coverage reporting: FIXED.
   - The summary line `PASS=.. FAIL=.. QUARANTINED=.. SKIP=..` and a WARNING naming the quarantined gating scenarios are printed.
   - JSON has `counts`, `quarantined_gating` and `strict`.
   - Live check with `NETNS_STRICT=1 NETNS_SCENARIOS=n1,n2,n8`: the run failed as designed (n2 quarantined, warning and "failing the run" printed). `docker ps` shows no fsync-netns leftovers.
   - Quarantined n2, n3, n4, n9 are still not exercised. That is honestly reported, not hidden.
2. N8: FIXED as test design.
   - The diagnosis is plausible and documented: `delivered` comes from the transport ack, and the gate closes the connection only after the handshake. I did not verify the core claim. The live run showed 2 of 3 puts "delivered" and 1 refused with "peer is offline", which matches the dev report's "2/3 in the matrix".
   - The result no longer flips PASS/QUARANTINED on timing. The A-side security checks still gate: nothing received, stored or fetched, and refusal times under 30 s.
   - The finding is recorded in notes on every run.
   - History query: now `|| fail`. Refusal text is asserted: "peer is offline" is a real error (`core/fsync-core/src/service/error.rs:10`) and "did not come online" is in `core/fsync-cli/src/ctx.rs:141`.
   - Caveat: the text assertions couple to those strings. A rewording in core fails N8 loudly. That is acceptable and intended, but worth a comment pointing at the source.
   - Caveat: N8 can now never go QUARANTINED. If core fixes the ack, the finding is not auto-detected as resolved; the note just changes counts.
3. Provenance: FIXED.
   - `git_sha` = tested commit, with a `-dirty` suffix when anything outside `docs/test-results` differs. The committed JSON is `0140eae` (non-dirty), and `rev-parse` agrees. My live run recorded `4920606`.
   - `cli_path` and `cli_path_tier` are populated when emitted. They are null in all rows because this base lacks the track-k fields. The README says so.
   - A recorded result is for 0140eae, and HEAD adds only a lint-comment commit. That is fine.

## Minors checked
- Host watchdog: a background `sleep NETNS_MAX_S; remove_run_objects`, which removes exact `$RUNNER`, `$PG` and `$NET` names only. It is killed in `cleanup`, and survives a SIGKILL of the parent as intended. Default 2400 s bounds the privileged runner.
- `hosts_reset`: a correct idempotent per-host reset to the built address and default route. It runs in the runner between scenarios and in N6's EXIT trap. It is not a replacement for a restart after a half-failed `ip addr` change, but it is adequate.
- N6 loop edge fixed (`started` flag). N7 poll has `sleep 0.5`. PIDFILE kill now uses `while read` (SC2013 gone). `exit "$rc"` is quoted.
- p0: only the shellcheck directive moved; behaviour unchanged. `/bin/bash -n` passes on 3.2.

## Remaining minor
- shellcheck still reports 4 info-level items (SC2329 x2 for trap-only `cleanup`, SC1091 for sourced libs, SC2153 RELAY). They are false positives. A scoped disable would make it fully clean, but the dev report says "clean", which overstates slightly.
- Quarantined n2 and n9 still cover none of the offline-start behaviour. NETNS_STRICT is off by default, so a default run still exits 0 with 4 quarantined. That is a deliberate, visible tradeoff; run strict after the track-K merge (as the dev plans).
- N8 notes are never failures, so a regression where A receives something is still caught, but a regression in sender feedback is not. Acceptable as documented.
- The N8 "slowest put 5 s" uses `--wait 5`; the earlier 20 s refusal behaviour is no longer measured.
- Open question from the dev (keep n3b?): yes, it exercises the real punch path and passes at 16 MiB and 64 MiB with direct bytes verified.

**Status:** DONE_WITH_CONCERNS
**Summary:** All three majors are fixed or reclassified honestly. Strict mode, counts and provenance were verified live, and cleanup is exact-name only.
**Concerns:** 4 gating scenarios remain quarantined (product gap: no offline CLI start, no QAD on the local relay, N4 latency). N8's text assertions are coupled to core error strings. Four info-level shellcheck false positives remain.
**Score:** 9.5/10, gate met.
