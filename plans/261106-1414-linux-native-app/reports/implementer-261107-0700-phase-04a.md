# Phase 04a report: files inbox, retention, offers, history copy-back

Status: DONE_WITH_CONCERNS (X11 stage of e2e flaked 2 of 4 runs, pre-existing code; see below).

## Files
Created (apps/linux): platform/src/files/{mod,inbox,inbox_tests,retention,download_dir,transfer_wait}.rs; platform/src/sync/{shared_files,tests_files}.rs;
app/src/{core_state/files_state,engine/{offers,history_ops,inbox_runtime,xdg_open},bridge/{history_model,transfers_object}}.rs; app/qml/{HistoryView,OffersView}.qml.
Modified: clipboard/{uri_list (build fns + tests), formats (files_entry)}.rs, sync/{mod,engine,engine_types,inbound,shared,shared_inbound,tests_rig,tests_fakes}.rs (SyncEnv.files replaces on_files; ClipService::fetch_file default method),
ipc/protocol_e2e.rs (+offer-accept, history-copy, history-download), platform tests (2 adversarial files: `files: None`; ipc e2e tests), lib.rs;
app: clip_service_adapter (fetch_file via TransferWaiter), delegate_adapter, clipboard_runtime, e2e_control (+status offers/history), tray (History entry), hub, gate (OpKind::History), core_state/mod, engine_object (sectionRequested), bridge/mod, build.rs, Main.qml (tabs History + Offers);
tests/e2e/clipboard-with-cli.sh; docs/adr/014 (files section); phase file todo + plan.md status.
Deviation: outbound uri-list builders live in clipboard/uri_list.rs (next to the parser, DRY), not files/uri_list.rs.

## Verification
- cargo fmt --check: clean (fmt=0).
- clippy --all-targets (default) and with FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control: no warnings.
- cargo test --manifest-path apps/linux/Cargo.toml --workspace: green (platform lib 110, app 32, integration suites all ok); platform with e2e-control green.
- e2e clipboard-with-cli.sh: PASS twice in a row (e2e7, e2e8): history copy-back text, CLI push + offer-accept (sha256 in Downloads/fsync, 0700), file + folder L1->L2 (inbox 0700, uri-list + gnome-copied-files, marker, sha256), file copy-back reuse, 21 MiB clip not downloaded then history-download (sha256).
- build-in-container.sh ubuntu-2204: exit 0, "smoke ok" (both smoke runs; Qt 6.2 loads the new QML).
- No leftover Xvfb/sway/app/docker processes.

## Concerns
- X11 stage flaked in 2 of 4 runs reaching it: `cannot connect to X11: Connection reset by peer` in X11Backend::start (phase 03 code, untouched; Xvfb alive). Two later full runs passed. Suggest a short connect retry in x11_conn::connect (not done: outside this phase's file list).
- On a settings change the retention hook (`inbox_runtime::retention_soon`) has no production caller until 04b's settings screen; only the e2e `clip-mode` command calls it.
- Offers are not cleared on sign-out/logout.
- Not done by design: Omarchy file-manager paste, GNOME Nautilus LIVE (phase stays In Progress).
- cxx-qt: second `extern "C++Qt"` QAbstractListModel declaration collides at link time; new models alias the one in devices_model.rs (documented in ADR).

Unresolved: none beyond the above.

## Fix round 1
Fixed: H1, M1-M5, L2, L3, L4, L5 plus tests. Kept the coordinator's x11 retry and tester tests.
- H1: `Inbox` holds a shared in-flight set (`begin_download` -> `DownloadGuard`, `fetch_item_guarded`); one `Inbox` lives on `Core`. Duplicate (eager, History, e2e) refused "Already downloading."; History rows get `downloading` role, Download button disabled.
- M1: retention keeps in-flight ids; explicit downloads get `.explicit` marker: age limit only, excluded from size budget (ADR 014 updated). `.complete*`/`.explicit*` reserved names (also fixes `.complete.tmp`, L2).
- M2: `admit_offer` (pure): cap 20 active, 50 rows, extra pushes booked as Declined row with note on the callback thread (no thread/notification); offer notifications coalesced (5 s).
- M3: `FilesState::reset()` (cancel flags + epoch bump + clear offers/large/history rows) on logout and on SelfRemoved.
- M4: Offers role `state` -> `offerState`; no other Item-shadowing roles.
- M5: `cancel_all()` right after `gate.close()` in shutdown; `download_blocking` watches the epoch; cancelled/failed inbox download removes its partial dir.
- L3: transfer failures show "The transfer failed." (no raw core text); nothing with names logged. L4: GNOME copy-back message is set by the GUI write itself ("Copied." / failure), no race. L5: Offers and History models emit `dataChanged` when row ids are unchanged (reset only otherwise).
- Split: offers.rs 177, offers_accept.rs, offers_tests.rs; all files < 500.
- New tests: should_auto_accept 1 GiB boundary; admit_offer cap/dup/trim; reset/epoch; notify coalescing; duplicate download refusal + guard release; reserved names; retention skip (explicit + keep); `is_transient`. `on_incoming` itself is not unit-tested (it would raise a real desktop notification); its booking logic is covered through `admit_offer`.
- e2e race fixed (test only): after L2 `wl-copy other`, wait until L1 wrote that text before L1 offers the large clip (a late write replaced it; seen once as "L2 should report the large clip").

Verification: fmt --check 0; clippy --all-targets default and e2e-control: no warnings; `cargo test --workspace`: green (platform lib 119, app 35, all suites ok); smoke rc=0 offscreen; build-in-container ubuntu-2204: "smoke ok", exit 0.
e2e clipboard-with-cli.sh, 5 runs on the final script: PASS x5 (exit 0 each). Earlier 5 runs on the pre-race-fix script: 4 PASS, 1 FAIL (the race above). No X11 connect reset in any run (logs grepped); the retry is silent, so a retry that succeeded would not show. No leftover processes.
Open: GNOME/Omarchy manual checks as before; History delete does not delete inbox copies (documented).

## Fix round 2
- L-N1/L-N2: one helper `inbox_runtime::reset_account_files` (FilesState.reset + `gate.invalidate(OpKind::History)` + pokes) used by logout, SelfRemoved and SessionExpired. Tests: SessionExpired/SelfRemoved/helper clear offers/held clips, set cancel flags, make an in-flight History load stale.
- L-N3: History download shows the real refusal text (invalid id vs "Already downloading.").
- L-N4: `sweep` takes a `claim` closure; retention claims the item (`begin_download`) right before removal and skips it if the claim fails. Test: item claimed mid-sweep is skipped, claim released after removal.
- L-N5: note in phase-04b Key Insights; ADR 014 round-2 note.
- Splits: inbox.rs 325 -> 258 (names.rs, inbox_claims.rs), files_state.rs 323 -> 182 (offers_state.rs). inbox.rs still > 200 (core fetch logic, no clean seam left); all < 500.
- Verification: fmt --check 0; clippy --all-targets default and e2e-control: no warnings; cargo test --workspace green (platform lib 120, app 37, suites ok); offscreen --smoke rc 0 (rebuilt binary); clipboard-with-cli.sh 2 runs: PASS, PASS. No leftover processes.
