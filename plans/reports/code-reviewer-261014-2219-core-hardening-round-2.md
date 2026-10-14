# Core hardening round 2 review (head 61c790c) - 9/10
Gates: fmt clean, cargo test --workspace all green, clippy -D warnings clean. FFI crate diff empty.
Round 1: H1, M2, M3, M4(partly), M5, L items all verified fixed (details in chat reply).
Findings: L1 first read at offset 0 on fresh reader still counts as sequential (range_reader.rs:~123, last_end starts 0) -> one speculative 4 MiB window for single header reads.
L2 concurrent start() on same service: second caller gets Invalid instead of idempotent Ok (lifecycle.rs:13-16, check-then-lock not atomic).
L3 logout(): set_account(None)? / live lock failure returns before take_refresh (login.rs:58-60), tokens stay.
L4 stop() fallback after 50 tries drops engine/lock while engine may still be referenced (lifecycle.rs:90).
L5 on_tokens runs under std Mutex (client.rs:~113): documented, but a delegate re-entering core deadlocks.
L6 try_lock error other than WouldBlock (e.g. Unsupported on some network FS) makes start fail with Io.
Q: does refresh::revoke revoke the whole token family? (discarded rotated token from an aborted refresh relies on it)
