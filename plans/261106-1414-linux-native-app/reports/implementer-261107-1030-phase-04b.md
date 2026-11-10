# Phase 04b report (implementer)

Status: code + automated checks done; phase stays In Progress (LIVE GNOME item and Omarchy manual items are not mine).

## Delivered
- Settings: `SettingsView.qml` (ScrollView, extended), `SettingsClipboardForm.qml`, `ShortcutHelp.qml`; `bridge/settings_object.rs`; `engine/settings_ops.rs` (Qt-free form <-> core Settings, tested). Save runs on an op thread, calls `refresh_mode` on mode change and `inbox_runtime::retention_soon` on retention change (cfg_attr dead_code removed). Help text covers inbox retention + age-only large clips. Secrets-in-file warning, backend/reason/capability via ClipboardStatus.
- Send: `engine/send_files.rs` (stage/validate with PathDenylist, pick, drop handling, Send-event status text), `DevicesModel` invokables (pickDevice, cancelPick, dropOnDevice, dropAnywhere), `DevicePicker.qml`, `SendDropArea.qml` (row drop + window drop), `Core.pending_send`, IPC `Send` now stages (errors: path-protected / path-missing / nothing-staged), section "send" opens the picker.
- Hotkeys: `engine/shortcut_help.rs` single source (Settings JSON, `--help`, README); `--help` flag added; README section; Super+Alt+C/V, no Ctrl+Alt.
- e2e-control: `pick-device` (platform protocol + app handler; `clip-mode` now reuses settings_ops::save). Test added in platform/tests/ipc_e2e_control.rs.
- Platform: `clipboard::uri_list` made `pub` (reuse parse_uri_list for drops).
- ADR 014 "Phase 04b decisions" appended; phase todos ticked; plan.md row In Progress.

## Verification
- cargo fmt --check: ok
- cargo clippy --all-targets (default) and FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control: no warnings (besides linker gold notice)
- cargo test --workspace: app 45 passed, platform 120 + integration all ok; e2e-control protocol test ok
- clipboard-with-cli.sh: PASS twice consecutively (runs 2 and 3), incl. `--send` + `pick-device` -> CLI fetch sha256 match, ~/.ssh / outside-HOME / missing path refused (exit non-zero, path-protected), nothing staged after refusal. Run 1 (before those two) failed once in the pre-existing 04a LARGE step ("L2 should report the large clip", got 'Sent to 2 devices.'), before any 04b step; not reproduced in 2 further runs -> flaky, not investigated.
- build-in-container.sh ubuntu-2204: smoke ok (RC 0, QML loads on Qt 6.2). Host `--smoke` offscreen: exit 0.
- No leftover processes (sway/Xvfb/app) after the runs.

## Concerns
- Drop from real file managers (Nautilus/Dolphin/Thunar) and the DropArea/DevicePicker UI were not exercised interactively (offscreen only); picker opening is covered via IPC staging, not a visual check.
- Allowlist means `--send /tmp/x` is refused (ADR 014 policy), may surprise users.
- Settings tab shows all four bind blocks (no desktop auto-detect).
- Possible flake in 04a LARGE e2e step (see above).

# Fix round 1

Status: DONE_WITH_CONCERNS. All items addressed except L4 (see below).

## Changes
- H1: `send_files::validate` returns canonical UTF-8 `String`s, refuses non-UTF-8 (`path-unsupported`); no `to_string_lossy` on the send path; paths re-validated in `pick` right before core (L3).
- H2/L1/L5: new `SendObject` singleton (`bridge/send_object.rs`, summary + stageId; pick/cancel/drops moved out of DevicesModel -> devices_model.rs back under 200 lines, L8). `core_state::staged_send::Staged` (id changes on every stage/clear). Picker shows summary, warns when the set changed while open, closes when the set is emptied; click sends the id on display, engine refuses a different current set (`stage-changed`). Failed send keeps set and reopens picker. `closePolicy: CloseOnEscape` (no click-outside dismissal) so `rejected` always means Escape/Cancel.
- M1: cold-start `--send` stages after Ready (`on_cold_start`, 120 s max); signed out / error -> banner "Sign in first, then send the files again.". Documented in ADR 014. e2e covers it (X11 restart carries `--send`, pick-device -> CLI fetch sha256).
- M2: `inbox_runtime::reset_account_files` calls `send_files::cancel_pending` (all sign-out paths); Main.qml closes the picker when page != 2.
- M3: IPC thread never touches fs: staging on an op thread, `stage_bounded` waits 2 s then replies ok ("queued"); server-side `exists()` removed from `ipc::protocol` (platform test updated). Refusals also show in the window.
- M4: `settings_ops::resolve_mib` keeps the loaded value while the field is untouched; SettingsObject keeps a baseline; SpinBox `to` = i32::MAX.
- M5/L7: drops pass `QList<QUrl>`, Rust takes `to_encoded()`, one strict decode (`%+1` refused), no trimming of names; tests for trailing-space / `#` / `%` names.
- Tester: `StatusCell::event_seq/event_if_unchanged`, used by `shared.rs publish`; unit tests (slow publish + concurrent event; normal outcome still shown). Pending set cleared on refusal and on unknown `pick-device` name. Pure fns + tests: `send_event_text`, `needs_retention_sweep`, `clip_mode_changed`, `resolve_mib`, `summarize`.
- L2: hardlink limit in ADR 014. L6: `MAX_PATHS` reused (`too-many` code). L8: e2e now asserts "Files sent." (fail, not log).
- ADR 014 "Phase 04b fix round 1" section added. e2e lib: `APP_EXTRA_ARGS` for app_start.

## Not done / notes
- L4: `Device` has no paired flag; picker lists all other account devices, a click on an unpaired one shows "That device is not paired." (core re-checks). Noted in ADR.
- L5/Qt 6.2: container smoke loads the QML (picker, `QList<QUrl>` invokables); Dialog behaviour not exercised interactively.
- Status clobber (product): fixed as above; the tester's LARGE-step wait stays (harmless).

## Verification
- fmt --check ok; clippy --all-targets clean (default and FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control).
- cargo test --workspace: app 58, platform lib 122, all integration ok (ipc test adapted); e2e-control protocol test ok.
- clipboard-with-cli.sh: PASS 3 times (runs 1, 3, 4 of 4); run 2 failed during initial pairing ("Pairing failed: network: connection lost", L1 pairing with devA) before any 04b step = the known environment pairing stall the tester saw, not reproduced.
- build-in-container.sh ubuntu-2204: smoke ok (RC 0).
- No leftover processes of mine.

# Fix round 2

Status: DONE. N1-N8 done, N9 note only (resolve_mib keeps the loaded baseline; a value changed elsewhere since the screen loaded would be overwritten by the stale baseline on save; accepted, core is the only other writer).

- N1: `StatusCell::update` now bumps the event counter whenever `last_event` changes, so every writer (received clip, waiting for Paste, Pasted, lost connection) counts. New test `a_slow_publish_outcome_does_not_overwrite_a_received_clip_status` drives a real `on_remote_clip` apply during a slow publish.
- N2: `Staged::begin_request/is_latest/void_requests`; request taken when staging starts; `stage_with` discards (`superseded`, silent) a verdict older than the latest request, success or refusal. Sign-out voids running checks (`reset_for_sign_out`). Tests: later-finishing older request neither replaces nor clears; refused latest empties; sign-out voids.
- N3: pure `send_validate::cold_start(state, expired)`: signed out/awaiting code refuse at once; NeedsPairing refuses ("Pair a device first..."); Error -> not-ready; keyring/offline/starting wait up to 120 s then refuse with their own sentence (keyring: "Unlock the keyring...", offline, not-ready). Tested.
- N4: reopen picker after a failed send only if the staged id still matches. N6: STALE pick reopens the picker with the current set.
- N5: dropped channel (gate closed / spawn declined) -> `shutting-down`; at most 4 stage checks in flight (`Core.stage_inflight`, RAII slot), beyond that `busy`. Tests for busy and closed gate (slot freed).
- N7: split: `send_validate.rs` (164, pure rules), `send_stage.rs` (179), `send_files.rs` (133, pick/send/drop/events; re-exports); tests in `send_files_tests.rs`.
- N8: IPC client `to_line` refuses non-UTF-8 send paths (`path-unsupported`); `forward` prints it and exits 1. Platform test added.

Verification: fmt ok; clippy clean (both feature sets); cargo test --workspace green (app 64, platform lib 123, ipc 10, all others ok; e2e-control protocol test ok); clipboard-with-cli.sh PASS twice (2 of 2 runs); build-in-container.sh ubuntu-2204 smoke ok. No leftover processes.

# Fix round 3

Status: DONE.
- P1: `StatusCell::update_event` (always bumps) used by the four inbound/paste writers and by `event()`; `update` keeps the change-detecting bump for other writers. New test: a second identical "Received text from ..." during a slow publish is not overwritten by the outcome.
- P4: launcher pre-check runs on `std::path::absolute(p)` (`send_paths_are_utf8`); unit test incl. a non-UTF-8 cwd + relative path.
- P2/P3: documented in ADR 014 ("Round 3 notes"): 4 stuck checks make later `--send`/drops answer busy until restart; row drops and pick re-checks uncapped by design; late-latest stage may open the picker out of context (summary keeps consent informed).
- Verification: fmt ok; clippy clean (both feature sets); cargo test --workspace green (app 65, platform lib 124, ipc 10, rest ok); clipboard-with-cli.sh PASS once. No leftover processes.
