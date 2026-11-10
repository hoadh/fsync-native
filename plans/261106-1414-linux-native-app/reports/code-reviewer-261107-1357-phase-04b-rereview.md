# Re-review: phase 04b fix round 1

Score: **8.5 / 10** (was 7.5). Critical: 0. High: 0. Medium: 2 (new). Low: 7.

Checks: `cargo test --workspace` green (app 58, platform lib 122, all integration suites ok). Clippy `--all-targets`: 0 warnings / 0 errors on default and `FSYNC_ALLOW_TEST_FEATURES=1 --features fsync-linux/e2e-control`.

## Prior findings: status

| Item | Status | Evidence |
|---|---|---|
| H1 validated ≠ sent | Fixed | `send_files.rs:55-75` returns canonical UTF-8 `String`s, `path-unsupported` otherwise. Core gets these exact strings (`send_to` takes `Vec<String>`). Re-validated in `pick` (`:257-266`) |
| H2 picker hides the set | Fixed | `Staged` id is bumped on every set/clear (`staged_send.rs`). Summary shown, "changed meanwhile" line, click carries the displayed id, engine refuses a mismatch (`:251`) |
| M1 cold-start `--send` | Fixed (see N3) | `main.rs:81-89` → `on_cold_start` |
| M2 sign-out | Fixed | `reset_account_files` → `cancel_pending`. `Main.qml` closes the picker when `page != 2`. An emptied set closes it via `onStageIdChanged` |
| M3 IPC thread fs | Fixed (see N2, N5) | `stage_bounded`. `ipc::protocol` no longer stats |
| M4 SpinBox clamp | Fixed | `resolve_mib` against the loaded baseline. `to: i32::MAX`. Save is refused before the first load |
| M5 URL decode/trim | Fixed | `QList<QUrl>` → `to_encoded()`, split on `\n` with only `\r` stripped, no trim. Strict decoder |
| L1 set lost on failure | Fixed | Set kept, picker reopened (see N4) |
| L2 hardlinks | Documented (ADR 014) | |
| L3 re-validate at send | Fixed | `pick` op thread |
| L4 unpaired devices listed | Open, **acceptable** | Core `send_files` checks `is_trusted` first (`clips.rs:111`), so no data leaves. Cost is one error sentence. The label "No other paired device." is slightly inaccurate. Revisit when `Device` gets a paired flag |
| L5 click-outside | Fixed | `closePolicy: Popup.CloseOnEscape` |
| L6 MAX dup | Fixed | `MAX_PATHS` reused, `too-many` |
| L7 `%+1` | Fixed | |
| L8 sizes / e2e assert | Partly | e2e asserts "Files sent.". `devices_model.rs` is 165 lines. **`send_files.rs` grew to 353 lines** (target 200, limit 500) |
| Tester: refusal / unknown pick clears set | Fixed | `stage` Err → `cancel_pending`. `e2e_control.rs:132-136` |
| Tester: publish clobbers newer status | **Partial**: see N1 | |

Removing the server-side `exists()` check is safe. The only consumer of `IpcCommand::Send` is `ipc_handler.rs:32` → `stage_bounded` → `validate` (absolute path, canonicalize, allowlist/denylist, UTF-8), and again in `pick`. The protocol still refuses NUL bytes and relative paths. No path reaches core without passing `validate`: drops go through `validate` in `dropped`/`stage`, picks through `pick`, and cold start through `stage`.

## New findings

### Medium

**N1. The status-clobber fix covers only `StatusCell::event` writers. The inbound messages it was meant to protect bypass the counter.** Files: `platform/src/sync/status.rs:55-82`, `shared_inbound.rs:60-63,76-82,120-123`, `engine.rs:170-174`.
`event_if_unchanged` compares against `events`, which only `event()` bumps. These lines write `last_event` through `update()` directly without bumping it:
- "Received {kind} from {name}." (auto apply)
- "Received ...: waiting for Paste." (manual/held)
- "The clipboard connection was lost. Restart fsync."
- "Pasted the received clip."

Scenario: a large file publish is running, and a text clip arrives from a phone in Manual mode. The status reads "waiting for Paste", then the publish finishes and `event_if_unchanged(seq)` still sees the old seq, so it overwrites it with "Sent to 2 devices.". This is the same class as the 04a flake. The unit test passes only because it injects the newer event through `engine.event()` (`tests_outbound.rs:155`).
Fix: bump the counter whenever `last_event` changes, for example a `StatusCell::update_event(|s| ...)` helper, or compare old and new `last_event` inside `update` and bump on change. Add a test that drives a real inbound `apply` during a slow publish.

**N2. A late verdict from an earlier `--send` overwrites a newer staged set.** `send_files.rs:187-219`.
After the 2 s wait, the IPC reply is "queued", but the op thread keeps going. Scenario:
1. `--send /mnt/nas/a` blocks on a slow mount and is answered "queued".
2. The user drops `b.pdf` on the window; `b` is staged and the picker shows it.
3. The mount recovers, so `a` is staged, replaces `b` and pops the picker.

The summary and "changed meanwhile" line prevent an unseen send (H2 holds), but `b` is silently lost and a picker appears minutes later out of context.
Fix: take a request sequence number when a stage request *starts* (an atomic in `Core`) and, under the `pending_send` lock, drop a verdict whose sequence is older than the latest request. Optionally ignore verdicts older than about 30 s.

### Low

- **N3** `on_cold_start` (`send_files.rs:223-241`) treats `Offline`, `NeedsPairing` and `WaitingForKeyring` as "keep waiting", then after 120 s refuses with "Sign in first, then send the files again." That text is wrong for those states: the user is signed in. A keyring unlock prompt left open for more than 2 min also loses the files. Use a separate "fsync is not ready yet: send the files again" text for the timeout and non-SignedOut states, and refuse `NeedsPairing` at once ("Pair a device first").
- **N4** A failed send reopens the picker unconditionally (`:309-311`), even when the set was cleared (sign-out) or replaced since. The picker then opens empty, or with another set and no "changed" warning, because `onOpened` resets the flag. Reopen only if `staged_id == current id`.
- **N5** `stage_bounded`: if `spawn` declines (gate closed) or the thread spawn fails, `tx` is dropped, `recv_timeout` returns `Disconnected`, and the launcher is told "ok" (queued) for work that will never run. Map `Disconnected` to `Err("shutting-down")`. Hung-mount threads are unbounded: one blocked op thread per `--send` (at most one new one about every 2 s, because IPC is serial). Quit is unaffected, since op threads are not joined and the gate closes. Optional: allow only one stage in flight and reply `busy` otherwise.
- **N6** `STALE` refusal (`:251-254`): QML has already `accept()`ed and closed the picker, and the text says "check the list and pick again", but nothing reopens it. Call `show_section(None, "send")` on STALE.
- **N7** `send_files.rs` is 353 lines (target 200). Split staging, cold start and summary into, for example, `send_stage.rs`, leaving pick/send/events in `send_files.rs`.
- **N8** The IPC client still lossy-encodes non-UTF-8 paths (`protocol.rs:144`). `--send` of such a file reports `path-missing` instead of `path-unsupported`. Refuse on the client side with the same code.
- **N9** `resolve_mib` keeps the *baseline* when untouched. If the value changed elsewhere since the screen loaded, a save writes the stale baseline back. Rare (core is the only other writer); acceptable, note only.

## Verified OK (new code)
- `SendObject` singleton: it is instantiated at startup (the `DevicePicker` `Connections` target), so `register_send` runs before the first poke. All property writes happen in `refresh` on the GUI thread via `hub.poke_send` → `queue`. `pick_device` on the GUI thread only clones under a lock and spawns.
- Stage-id: the id is monotonic within the process and is bumped on clear as well, so a stale id can never match an emptied-then-restaged set. `send_to` clears only when the id still matches, so a set staged during an in-flight send survives.
- Picker: `close()` on a page change does not emit `rejected`, and the set is cleared by the reset path anyway. `Popup.opened` and `closePolicy` are available in Qt 6.2.
- `QList<QUrl>` invokables: the QML conversion of `drop.urls` is standard. The container smoke only loads the QML and does not exercise the drop at runtime (as stated in the report).
- No file names are logged. The summary is GUI-only.

## Recommended actions
1. N1: make every `last_event` write bump the counter, and add a test with an inbound `apply`.
2. N2: request sequence so late verdicts are discarded.
3. N3, N4, N6: correct cold-start text, conditional reopen, reopen on STALE.
4. N5, N7, N8 as convenient.

## Unresolved questions
- Should a cold-start `--send` keep waiting through `WaitingForKeyring` beyond 120 s? This is a product call.
