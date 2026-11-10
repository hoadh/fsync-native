# Code review: phase 02 re-review

Score 8.5/10 (was 7). Critical 0, High 0, Medium 1, Low 10. clippy clean, 59 tests pass. H1-H3, M1-M9, L1, L3 verified fixed.

## Medium
1. Keyring locked after boot cannot be unlocked from UI (secrets_thread.rs:239-248, unlock_policy.rs:207-211, Main.qml:118-123); rotated refresh tokens only in memory, lost at quit. Fix: StatusBanner "Unlock keyring" action when secrets_unsaved || waiting_keyring; mirror worker status into UiState after each job; retry persist after unlock.

## Low
2. qputenv/qunsetenv activation token races getenv on other threads (app_identity.cpp:25-30); only do it when runtime Qt >= 6.3.
3. boot.rs:153-156 leaves state WaitingForKeyring after unlock during long start; set Starting.
4. may_create_keys uses Path::exists (fails open on EACCES); use try_exists, Err = history present; also treat history.keycheck, history.corrupt-*, blobs/ as history.
5. ops_session.rs:97-105 gate check vs start() TOCTOU; go-online op threads not awaited by shutdown.
6. reset_history/retry guarded only in QML (ops_recovery.rs:197-235); use boot.try_begin as mutual exclusion / refuse when busy.
7. pending_revoke resume clears field on stale snapshot; decide inside persist closure.
8. Late ConfirmRequest after cancel reopens SAS dialog (ops_pairing_events.rs:177); Idle guard.
9. retry: boot thread spawn failure leaves UI on Starting; call fail().
10. finish_after_event_loop returns early if begin running; main may exit mid-stop.
11. secrets_thread.rs 205 lines; move Worker out.

## Unresolved
1. Does dropping oo7 unlock() future dismiss the prompt (5 s to type password)?
2. After reset_history, does ensure_account return NeedsPairing or Ready?
3. Can core emit ConfirmRequest after pairing_cancel?
4. Min glibc (2.35 floor) setenv hazard.
