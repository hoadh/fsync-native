# Linux app follow-ups: packaging, offline publish, lazy-file latency, LIVE checks

**Date**: 2026-11-11 06:17 to 2026-11-12 01:37 (merges of PRs #2, #4, #5, #6)
**Severity**: Medium
**Component**: `apps/linux` packaging, `core/fsync-core` publish lanes, `core/fsync-core` dial, `apps/linux/tests/e2e`
**Status**: Resolved (one open product item: first send after idle)

## What Happened

Plan `261110-2344-linux-app-follow-ups` closed the review lows and open items from PR #1. Merged work, by PR:

- **#2 packaging (`7dd4ca4`)**: repo licensed `MIT OR Apache-2.0`; SQLCipher's BSD-3 notice was missing from THIRD-PARTY-LICENSES (`about.toml` clarify for `libsqlite3-sys`); PKGBUILD git sha pinned via `FSYNC_GIT_SHA`; `$CARGO_HOME` and `$srcdir` remapped through `CARGO_ENCODED_RUSTFLAGS` and `-ffile-prefix-map`; one asset list in `scripts/lib/assets.list`.
- **#3 pairing flake**: see its own entry.
- **#4 offline publish (`e21fb8a`)**: each peer lane keeps one "latest" slot; `contract_version` bumped to 3 because `Published.pending` changed the FFI record layout. Windows and macOS startup checks now refuse a mismatch.
- **#5 lazy-file latency (`5b257a9`)**: X3 lazy file Linux -> Windows went from 8701 ms to a median of 2977 ms (limit 10 s). X2/X5 text went from 4-6 s to about 60 ms.
- **#6 LIVE checks (`82d8481`)**: `docs/how-to/linux-live-checks.md`, with results on Omarchy (Hyprland) and the GNOME VM. Three LIVE failures led to PR #7.

## Impact

An offline device no longer stalls every publish: `publish_clip` returns after every answering peer or 1.5 s (`EngineConfig::publish_wait`). Before, each publish waited up to 7 s behind one dead device, so every following clip arrived late on the reachable devices. The FFI bump forces a binding regeneration for anyone building the apps.

## Technical Details

- Publish lanes: a copy burst behind a dead peer held one clip frame per copy for up to 7 s (about 256 KiB each). Now at most two frames per peer.
- Dial: `Dialer::connect_following_addrs` restarts a running dial when presence gives new addresses. Test `dial_follows_addresses`: 2.01 s before, 0.38 s after.
- Harness: the first copy waits for the CLI `ready` event and the presence list. The old 1.5 s fixed sleep before polling the clipboard was removed.
- Packaging check: `smoke-install.sh` passes on ubuntu:22.04, debian:12, ubuntu:24.04 (.deb), fedora (.rpm), AppImage (three hosts) and Arch (namcap clean).

## What We Tried

- Lazy-file gate: first theory was the product's send path. Stage timestamps showed the harness caused most of the variance (fixed 1.5 s sleep, cold PasteHelper). Product share left: about 2 s from offer to "received files".
- Bundle the Arch OpenSSL like deb/rpm: rejected. Arch uses system OpenSSL (`OPENSSL_NO_VENDOR=1`) to avoid a build path in the binary.

## Key Decisions

- **Publish wait bounded at 1.5 s; late peers finish in the background.** Trade-off: a pending device gets "not confirmed" status, not "failed", and the late outcome is only logged.
- **FFI bump to 3 without changing the checksum.** A v2 binding loads against the new library and reports a mismatch at startup instead of a decode error.
- **Deferred to MVP phase 16:** base image digests, rustup-init pinning and linuxdeploy trust-on-first-use. Owner decision 2026-11-11.

## Lessons Learned

- Measure with stage timestamps before changing the product. The 8.7 s lazy leg was mostly the harness.
- A "latest slot" per lane is the right shape for clipboard data; a queue behind a dead peer is not.
- Keep the FFI record change and the contract number in the same commit, or the mismatch is silent until decode.

## Next Steps

- First send after about 30 s idle costs about 2 s. Candidates: QUIC keep-alive on clip connections or a shorter first attempt (owner: Alex, no date). Weigh battery cost before choosing.
- Offline account device still delays the next publish by up to 7 s when the lane is not free (ADR 014 Open).
- Re-verify the git checksum for the PKGBUILD at release time (`updpkgsums`, MVP phase 16).

## Unresolved Questions

- Whether the 2 s first-send delay appears on other networks; measured only on the waso-vm tunnel and the LAN.
