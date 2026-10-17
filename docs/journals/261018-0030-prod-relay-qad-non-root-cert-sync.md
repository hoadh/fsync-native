# QUIC Address Discovery on Both Production Relays, Without Running the Relay as Root

**Date**: 2026-10-18 00:30
**Severity**: Medium
**Component**: deploy (relay compose, entrypoint, new `scripts/sync-relay-cert.sh`), prod relays SG and Ohio
**Status**: Done. Pushed 30ba963. A WAN direct-path check between two NATed devices is still open.

## What Happened

Owner approved the plan to enable QAD on the prod relays, on two conditions: ask before touching the VPS, and leave Caddy alone. Rollout went SG first, then Ohio. The owner opened UDP 7842 in Lightsail for each host. Only the relay container restarted; backend, Postgres and Caddy were untouched.

Time until online, measured from Vietnam with the spike `listen`:

| Relay | Before | After | Public address learned |
|---|---|---|---|
| SG | 3.2-3.3 s | 0.58-0.64 s | every run |
| Ohio | 6.1 s (1 run) | 2.1-4.1 s (15 runs) | 14 of 15 |

## The Brutal Truth

The old README said "set `RELAY_USER=0`" because Caddy's key is 0600. That would have put a root process on a public UDP port, on a shared host, just to read one file. The fix was not clever: copy the one cert we need to a place the relay can read.

The first version of the copy script scored 7/10 in review, and it deserved that. It took `find | head -1`, so with two issuer dirs it could pick a stale cert and flip between runs. It failed silently under cron. Cron ran it as root from `/opt/fsync/scripts`, a directory the deploy user owns, which amounts to a root escalation path. The destination dir was owned by the relay uid. None of that was exploitable that day, and all of it was wrong.

Ohio stays slow, and that is the path, not QAD. TCP RTT is about 0.26 s with occasional 1 s retransmits, TLS takes 0.5-2.3 s, and the times cluster in 1 s steps. The temptation was to keep tuning; we stopped instead. Users in Asia get SG.

## Technical Details

- `sync-relay-cert.sh`:
  - Validates the hostname.
  - Scans every issuer dir for the host. It skips symlinks, certs with less than 1 day left, and pairs where the crt and key public keys differ. It picks the latest expiry.
  - Stages the copies, then re-validates the staged pair, so a renewal between scan and copy cannot slip through.
  - Writes `root:10001`: dir 0550, files 0440. On an unchanged run it repairs owner and mode.
  - Logs to syslog tag `fsync-relay-cert`. Exit 3 means the copy expires within 14 days.
- Installed root-owned at `/usr/local/sbin/fsync-sync-relay-cert`. Daily cron `/etc/cron.d/fsync-relay-cert` at 04:23.
- Relay: `RELAY_QAD=1`, `RELAY_QUIC_BIND=0.0.0.0`, `RELAY_CERT_DIR=/opt/fsync/relay-cert`. It still runs as uid 10001 with `cap_drop ALL`. The log shows `QUIC server listening on 0.0.0.0:7842`.
- Docker test `deploy/scripts/test-sync-relay-cert.sh` (PASS):
  - Four issuer dirs: 40 d, 60 d (the expected pick), 1 d stale, and 90 d with a mismatched key.
  - Modes, idempotence and mode repair.
  - A bad hostname and a missing cert are refused.
  - Near-expiry gives exit 3.
  - The relay running as uid 10001 binds udp/7842.
- Rollback: each host keeps `.env.pre-qad`. Set `RELAY_QAD=0` and restart the relay, then close UDP 7842 and remove the cron file and `relay-cert`.

## What We Tried

- `RELAY_USER=0`: rejected. Root on a public UDP port.
- Mounting Caddy's cert dir: rejected. Caddy's data dir holds every site's keys.
- Docker bind mounts in the local test: these fail on Docker Desktop, because container root cannot bypass permissions there. A named volume reproduces real Linux semantics.
- First test run: the "QUIC server listening" check failed because `docker run` lacked `RUST_LOG=info`. It was a test bug, not a relay bug.

## Root Cause Analysis

The ~3 s first-connect delay came from iroh's net report waiting for QAD from relays that never answered it. Enabling QAD fixed it where the path is short (SG). The remaining Ohio delay is RTT and packet loss on a VN to US path.

## Lessons Learned

- Never do "first match" over a directory whose ordering you do not control. Rank by an explicit property, here expiry plus key match.
- A root cron job is only as safe as the ownership of the file it runs and of the directory it writes.
- Validate what you copied, not what you looked at.
- Docker Desktop bind mounts lie about root. Use named volumes when a test checks ownership.

## Next Steps

- WAN direct-path check between two NATed devices. The public port changed on every run, so the home router may be symmetric NAT, and then traffic stays relayed.
- Any future `deploy.sh --env-file` must carry the three QAD variables, or it turns QAD off again.
- Nothing watches syslog yet. Alert on `fsync-relay-cert` errors and on exit 3.
- Not yet verified that iroh keeps the old cert when a `Reloading` attempt fails on a mismatched pair.
- Optional review lows: `flock` around concurrent runs, and an expiry check on the die path.
