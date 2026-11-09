# Code review: phase 02 (engine bridge, app shell, Platform codec)

Score: 7/10. Critical: 0. High: 3. Medium: 9. Low: 7.

## High
- H1 secrets/mod.rs:108-117, boot.rs:203,225-272: `secret-service` marker is written before the first key save; a failed first save (locked keyring, prompt dismissed, timeout) bricks a pristine data dir ("stored keys are missing" forever). Fix: write marker only after first successful save, or allow creation when marker exists AND no core history AND store empty.
- H2 boot.rs:223-238: `Timeout`/`Unavailable` from `load` after open go to terminal `fail`; must be treated as WaitingForKeyring; worker resets `store = None` on Unavailable.
- H3 Main.qml:138-142, ops_session go_online: "Try again" after boot failure does nothing (service None); Quit only on signed-in page; no-tray GNOME cannot quit. Fix: retry re-runs boot (single-flight), Quit on every page, "Reset history" for HistoryKeyMismatch.

## Medium
- M1 secrets_sync.rs:285-294: persist_lock held across save wait; callback thread can block 5 s + 3 s.
- M2 boot.rs:289-326, ops_session.rs:262: quit during boot leaves engine started after stop; gate checks before new/start.
- M3 pending_revoke.rs:126-149: lost update + single slot; use Vec, remove only attempted entry under lock.
- M4 ops_devices.rs:297-305: on_status read-modify-write race; resurrects removed devices; patch under lock, existing rows only, ignore when signed out.
- M5 QML message labels use AutoText (StatusBanner 163,178; Main 136; PairingView 209,285; SignInView 223): set Text.PlainText.
- M6 Main.qml:52 activation token ignored; setDesktopFileName not set (app_id fsync-linux vs StartupWMClass app.fsync.Fsync). Fix via small C++ helper.
- M7 main.rs:299-309: exec() returning without hub.quit skips ordered shutdown; run it synchronously after exec.
- M8 pairing: double-click offer starts second offer; confirm reads request_id at click time, no phase guard, dialog re-pressable; late confirm error overwrites Idle (ops_pairing.rs:151-155).
- M9 secret_service.rs:50-51: every 10 s retry calls unlock(None) → stacked GNOME prompts; prompt at most once per launch or on user action.

## Low
- L1 guard keys on debug_assertions only; add compile_error in platform crate e2e-control; --build-info lists app features only; phase 05 must assert empty features.
- L2 Platform::Unknown(n<=3) constructible, unequal to known variant; JSON name "Unknown" not decodable (latent).
- L3 DevicesModel roles start at 0 (use UserRole+n); rowCount ignores parent.
- L4 autostart uses current_exe (AppImage /tmp mount path); prefer $APPIMAGE (phase 05).
- L5 IPC server single-threaded stall 2 s/conn; stop() connect-to-wake hangs if socket removed; is_held probe races.
- L6 ops_session::Online unused; autostart stat on GUI thread per poke; tray PNG decoded per call.
- L7 unnamed notification threads, ignored errors.

## Verified good
e2e-control compile guard + emit gate; oo7 objects all dropped inside block_on (after orchestrator fix); IPC hardening (0700, SO_PEERCRED, bounds, lock-holder-only bind); secrets file O_NOFOLLOW/atomic/zeroize; Qt 6.2-safe QML; codec golden bytes, Unknown round trip, successor, FFI wire_compat.

## Unresolved questions
1. Does core put peer/server text into FsyncError Invalid/Network/Internal or PairingEvent::Failed reason?
2. Phase 02 Error page: history reset for HistoryKeyMismatch or deferred?
3. Will phase 05 check --build-info; any custom profile with debug assertions?
4. Does core pairing_confirm reject stale request ids?
