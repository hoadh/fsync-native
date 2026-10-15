# Four-track hardening round 2: merged and pushed, CI still unverified

**Date**: 2026-10-15 07:59
**Severity**: High
**Component**: core FFI, Windows, macOS, deploy/infra
**Status**: Ongoing

## What Happened

Round 2 ran four tracks in parallel, each in its own worktree: A core, B Windows, C macOS, D infra. Everything is on `main` at `f00980d` and pushed. cms-node is redeployed at `f00980db3978`; public smoke and probe-health are green. GitHub Actions jobs do not start (billing), so no CI has run on final main.

During the rollback rehearsal, the API returned 502 for about 5 minutes. We caused it.

## The Brutal Truth

We broke production while rehearsing the rollback that was supposed to protect production. The script swapped images and never checked whether the target release understood the database schema. Then we declared "pushed" while the only real release gate, a green GitHub run on final main, cannot run. The plan's own risk table rates this H. That rating is accurate.

## Technical Details

- Rollback target `d9e5e9b7ef77` predated migration 0003. The DB was already at 3. The backend crash-looped with `migration 3 was previously applied but is missing`. 502 from about 03:08 to 03:13 (+07).
- Fix: `deploy/scripts/rollback-guard.sh` refuses targets whose migrations lack the DB's max version. `deploy.sh --force-rollback` overrides it.
- Redial after a peer restarts on a new port: 15-30 s down to about 2 s (`tests/dial_after_restart.rs`).
- `FsyncError::EngineAlreadyRunning` is typed. String matching is gone. `history_item_reps` added (additive, contract v2).
- Gates: cargo 308 pass / 0 fail; swift 222 / 0 failures; mac integration PASS; dotnet 156/156.
- Backups: nightly gpg-encrypted, Ohio to Singapore, 30-day retention, restore drilled.

## What We Tried

- Mid-run relay failover test: removed. In-process relays lack QUIC address discovery, so iroh never re-homes (45 s). Behaviour is unverified, not fixed.
- `logout_revocation` hang: not reproduced in 12 runs. Added timeouts so it fails loudly. Root cause unknown.
- Swift 6.1 CI fixes: done locally. The last green macOS run predates the refactor.

## Root Cause Analysis

Rollback was treated as "swap the images and compose file." Our migrations are forward-only and stateful. We never asked what schema the DB expected before rehearsing against a release it had already outrun. Separately, we leaned on local gates as a stand-in for CI, with no fallback when billing blocked it.

## Lessons Learned

- A rollback must check migration compatibility before it swaps anything. Compare the target's max migration with the DB's applied max.
- Rehearse rollbacks against a copy, or at a time when a 5-minute 502 costs nothing. Never on the live host.
- Local gates do not replace CI. When CI is down, write "unverified" in the status table, not "pushed."
- Don't keep the backup private key on one Mac.

## Next Steps

- Owner: restore GitHub billing. Re-dispatch `ci.yml` and `health-probe.yml` on `f00980d`. Done when rust, macOS and Windows are green in one run. Until then the hourly probe cannot alert.
- Owner: copy `~/.fsync-backup/fsync-backup-secret.asc` into the password manager. Backups are unreadable without it.
- Owner: enable DKIM (`google._domainkey` TXT). DMARC is still `p=none`.
- Owner: set `BACKUP_PING_URL` or accept log-only failure visibility.
- Track A: add retry for file/rep fetch on a dead cached connection. Decide on relay mesh, since peers on disjoint relays cannot connect.
- Lead: the changelog says cargo 307 and dotnet 155. Final gates are 308 and 156. Reconcile before the next release note.
