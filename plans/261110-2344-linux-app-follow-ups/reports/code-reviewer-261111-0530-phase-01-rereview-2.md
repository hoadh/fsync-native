# Code re-review 2: phase 01 fix round 2 (4238d72, 10d37ed, 9aaa39f)

Score **9.5/10**, APPROVED (critical, high and medium all 0). The merge waits on the cross-OS X1+X2+X5 run. Persisted by the controller from the reviewer's hand-back.

## Verified (archive copy on disk)
- `fmt` is clean. Production clippy (core lib, cli, ffi without test-hooks) and all-targets clippy are clean.
- Pairing tests pass: lib 65, `pairing_done_delivery` 3/3, `pairing_hardening`, `pairing_iroh`, `e2e_pair_flow`, `pairing_repair_after_reset`, `pair_kdf`.
- **M-1:** a non-empty `PAIR_LOOP_OUT` is refused (exit 2) before the trap is set. The script deletes only its own mktemp dir.
- **L-a:** the guard's pin and `set_pinned` are set together. Treating a post-pin error as success is right: N holds a list E has published and both sides trust each other. Dropping the post-pin Abort is also right. The case where E fails is the documented M2. Minor gap: no test forces a Done write failure; accepted.
- **L-b:** the hooks are under `cfg(feature = "test-hooks")`, so production always reaches `conn.close(0, b"done")`.
- **L-c:** the ADR now says "timing-dependent".
- **shellcheck:** the reviewer could not run it and relies on the implementer's container run.

## Remaining (low, optional)
- The ADR's "5 of 6 without the guard" figure comes from the round-1 responder. The reviewer measured 12/12 at HEAD minus the guard.
- When it detects a regression, the M3 test still depends on iroh's cancel/close race. A fully deterministic variant would abort the task.
