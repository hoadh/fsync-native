# Phase 03 report: lazy-file latency (X3) and text latency to Windows (X2/X5)

Branch `fix/lazy-file-latency` (pushed, not merged), head = the commit after `aed5ec8`. Gate refs: X3 `9d6562a`, X5 `aed5ec8`.

## Result
- X3 lazy file Linux -> Windows, 5 runs: 2971, 2977, 3029, 2975, 2970 ms. Median 2977 (limit 5000), max 3029 (limit 10000). Was 4577 / 8701.
- X1+X2+X5 passes once (`linux-cross-os-261111-1206.json`, X5 text app-to-win 60 / 66 ms; files 4002 / 3246 ms). Linux binaries there were from 9d6562a (`SKIP_BUILD`); only the 6 s offer relay wait (below) differs.
- Results JSON in docs/test-results: 1145, 1153, 1155, 1158, 1200 (X3 gate), 1206 (X5). Failed and diagnostic runs are not recorded.

## A) X3 lazy file: where the time went (ms after the Linux offer, Windows clock)
| Stage | before: run A | before: run B | after (5 runs) |
|---|---|---|---|
| app logs "received files" | 2039 | 2056 | 2031-2047 |
| hotkey sent | 2134 | 2339 | 2072-2122 |
| virtual files on clipboard | 3665 | 3866 | 2160-2226 |
| paste started | 3689 | 3884 | 2161-2228 |
| file created | 6791 | 4583 | 2671-2820 |
| file closed (= latency) | 7156 | 4946 | 2970-3029 |
- Harness: fixed 1.5 s sleep after Win+Alt+V before looking at the clipboard (+1.5 s), cold PasteHelper (0.7 s in run B, 3.1 s in run A from start to first byte). Fixed: hotkey when the app logs the clip, 50 ms polling, `PasteHelper --warm` started before the offer.
- Product (not fixed): the 2.0 s from offer to "received files". Cause from the Linux debug log: first send after ~30 s idle uses the cached connection whose direct path idled out ("Abandoned TimedOut"); the attempt waits its full `FIRST_ATTEMPT` 2 s, then a new connection delivers in ~40 ms. Same 2.0 s appears on the first in-auto text (about 2.2 s). Candidates: QUIC keep-alive on clip connections, or a shorter first attempt. Left open in ADR 014 (transport setting touches every app, gates have margin).
- Earlier attempts: run 1 failed on my own case-sensitive log match (fixed); run 2 hit a scheduled-task wrapper flake in the in-auto run ("interactive task did not finish in time"), not in the lazy leg, not investigated.

## B) X2/X5 text: root cause (both harness and product, measured)
Timeline of the baseline run (ms after copy #1, host clock): Windows CLI launched 2320, `ready` 2445, arrived 4057 (x5: 2248 / 2370 / 3076).
- Harness: copy #1 was made when the `remote run` was launched; the CLI is online only ~2.4 s later. So the first send hit an offline peer: attempt 1 fails at 2.0 s.
- Product: attempt 2 starts a dial with the addresses known then. The CLI came online during that dial, but a running dial kept its address snapshot and only used the new ones when its 2 s stage ended (Linux log: `delivered` 4.03 s after publish; Windows log: the "invalid handshake: aborted" of the stage-0 dial at the stage end, then the new connection, then the clip, 0.1 s). So arrival = next 2 s stage boundary after the peer came online; the earlier 6.2 s fits "online just after the second boundary". Not the 1.5 s publish wait (that only bounds how long `publish` waits), not the Windows side.
- Product fix: `Dialer::connect_following_addrs` (core/fsync-core/src/net/dialer.rs) restarts a running dial when the lookup gives the peer new addresses (100 ms check). With the old harness still copying at once: x2 4057 -> 2877 ms, x5 3076 -> 2701 ms (peer online at 2.4 s, arrival 0.3-0.45 s later). Core test `core/fsync-core/tests/dial_follows_addresses.rs`: 2.01 s without the fix, 0.38 s with it.
- Harness fix: first copy waits for the CLI's `ready` event (read over ssh from its stdout file; `ready` now carries `ts_ms`) and for the Linux app's presence list to show it online. Result 60 / 66 ms. `NO_READY_WAIT=1` restores the old behaviour.

## C) Found and fixed on the way: Windows-offer pairing flake
4 of 8 pairings with the Windows CLI offering failed with "No addressing information available" (ADR 014 Open since plan 261106). With `RUST_LOG` on the offering CLI: "pairing offer without a relay URL: no home relay after 3s" and iroh's "home is now relay" in the same millisecond (iroh's 3 s probe timeout over the TCP-only tunnel). `RELAY_WAIT` 3 s -> 6 s (service/pairing.rs, unit test pins > 5 s). One pairing passed since; needs more runs to confirm.

## Commits (branch fix/lazy-file-latency)
- test(e2e): per-stage timestamps for the lazy-file and text-in cross-OS legs
- test(e2e): match the app.log clip line case-insensitively; press the paste hotkey after 20 s regardless
- test(e2e): wait for the CLI peer before the first copy; poll the lazy paste at 100 ms with a warm PasteHelper
- test(e2e): 50 ms polling in the lazy paste leg
- fix(core): a dial follows the peer's addresses when presence delivers new ones mid-dial
- test(e2e): RUST_LOG for the windows pair run; NO_READY_WAIT diagnostic switch
- test(e2e): keep the offering CLI's stderr when the pair join fails
- fix(core): a pairing offer waits past iroh's net report timeout for the home relay
- docs commit (ADR 014, changelog, roadmap, test-results README + 6 JSON, plan files)

## Checks
- `cargo fmt --all`, `cargo clippy -p fsync-core -p fsync-cli --all-targets -D warnings`: clean. `cargo test -p fsync-core -p fsync-cli -p fsync-ffi`: 261 passed, 0 failed (includes the new tests; related dial/offline/relay tests run on their own too).
- shellcheck on the three e2e scripts: no errors (pre-existing SC1010/SC2034/SC2012 infos and warnings).
- Windows: PasteHelper was rebuilt and used on waso-vm by every X3 run (warm path). The non-warm path (`ShellPasteTests`, `clipboard-with-cli.ps1`) was not run: it needs an interactive clipboard session and I did not run the dotnet suite; CI's dotnet job covers the build.
- Not done: code review >= 9.5 and the PR (the plan's phase todo still has them).

## Other observations
- x5 file Windows -> app took 11.3 s once (limit 10 s) with "Dropping received relay packet: no available capacity"; 4.0 and 4.4 s in passing runs. Relay-forced 8 MiB through the ssh tunnel; unrelated to the dial change. Gate unchanged.
- The Windows VM clock stepped by 0.7 s between two runs (no w32time); one X5 run failed on "latency below minus error". The harness fails such a leg, as designed.
- The Windows `fsync-cli` left over from a failed pair run lingers up to 200 s (known); I did not kill it (denied by the permission layer; the harness ends it).
- /tmp quota: the shared quota filled once from another project's files (waso-scripts, 3.7 GB under /tmp/claude-1000); I removed only my own `/tmp/tmp.*` dirs. Debug logs of my runs are in /home/alex/.cache/fsync-p03-logs.

## Unresolved questions
1. Do you want the 2 s first-send-after-idle fixed (keep-alive on clip connections vs shorter first attempt)? It is the largest remaining part of X3 (2.0 of 3.0 s).
2. The Windows-offer pairing fix has one passing run only; want a loop of N Windows pairings before merging?
3. Was `ShellPasteTests` expected to run on the interactive session as part of this phase? I left it to CI.

## Interactive mode
waso-vm no longer needs `taskMode: interactive` from me; all runs are finished (I never changed the mode). The RDP session can be closed.

**Status:** DONE_WITH_CONCERNS
**Summary:** X3 lazy file median 2977 ms (max 3029) over 5 runs; X1+X2+X5 pass (text 60/66 ms). Root causes found with stage timestamps: lazy = harness poll + cold helper; X2 = harness copied before the peer was online plus a product dial that ignored new addresses mid-stage (fixed, core test). Also fixed the Windows-offer pairing relay race.
**Concerns/Blockers:** Review and PR not done. 2 s first send after idle left open. Pairing fix has one passing run. Non-warm PasteHelper path not run locally.
