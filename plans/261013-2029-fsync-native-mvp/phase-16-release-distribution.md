# Phase 16 — Release: signing, notarization, installers, auto-update

## Context Links
- Design §5 Distribution; procurement in [phase-01](phase-01-repo-scaffold-and-decisions.md); gate [phase-15](phase-15-integration-testing-cross-os.md)

## Overview
- Priority: P1 | Status: pending | Effort: 2-3 wk (B pipelines; C mac packaging; D win packaging), P1 w8-14
- macOS: Developer ID signing, hardened runtime, notarization + stapling, DMG, Sparkle 2 (EdDSA-signed appcast). Windows: Velopack installer/updates, Azure Trusted Signing for exe/dll/installer, firewall rule. Channels: beta, stable.

## Key Insights
- Every native binary must be signed: Rust dylib inside xcframework (codesign deep via Xcode), `fsync_ffi.dll` + app exe + Velopack setup on Windows.
- Auto-update is also a security channel → update feeds signed (Sparkle EdDSA; Velopack served via HTTPS + Authenticode signed packages).
- Rollback = fix-forward with higher version (updaters don't downgrade); keep previous release downloadable.

## Requirements
- One-tag release: `git tag vX.Y.Z` → CI builds core artifacts, both apps, signs, notarizes, publishes to beta channel; promote to stable manually.
- Versioning: semver shared across mac/win/core; core version embedded and reported in About + backend `User-Agent`.
- Backend/protocol compatibility: server supports N-1 app versions; ALPN v1 frozen.

## Architecture
```
.github/workflows/
  release-macos.yml    # xcodebuild archive → codesign → notarytool submit --wait → staple → DMG → appcast (sign_update)
  release-windows.yml  # dotnet publish x64/arm64 → Trusted Signing (exe, dll) → vpk pack → sign setup → upload
  release-promote.yml  # beta → stable channel copy
apps/macos/Release/{ExportOptions.plist, FsyncMac.entitlements, appcast-template.xml}
apps/windows/release/{vpk-config, firewall-rule.ps1 (Velopack install hook)}
deploy/updates/       # static hosting (object storage + CDN) layout: /mac/{beta,stable}/appcast.xml, /win/{beta,stable}/
```

## Related Code Files
- Create: above; `docs/release-runbook.md`
- Modify: `.github/workflows/core-artifacts.yml` (reuse as job), app project settings for signing (C/D own their app files)

## Implementation Steps
1. Mac: entitlements (network client/server, no sandbox), hardened runtime, notarize locally, then CI; Sparkle integration + EdDSA key in CI secret.
2. Win: Velopack integration in app startup (`VelopackApp.Build().Run()`), install hook adds firewall rule for UDP, Trusted Signing in CI.
3. Update hosting + channels; staged beta.
4. Release runbook: tagging, promote, hotfix, key rotation, revoked-cert procedure.
5. Dry-run full release to beta; update from previous beta build both OS.
6. Crash reporting (if ADR 006 approved, opt-in) wired before stable.
7. Linux release supply chain (deferred here from the Linux app review, phase 05 low L5; ADR 014):
   - Pin the container base images by digest for releases (`ubuntu:22.04` in `build.Dockerfile` /
     `release.Dockerfile`, `fedora:latest` and `archlinux:latest` in the smoke and run images), not
     floating tags.
   - Pin `rustup-init` (version + sha256) instead of fetching the unpinned script.
   - Add `--proto-redir '=https'` to every `curl --proto '=https'` download.
   - linuxdeploy and its Qt plugin are trust-on-first-use (sha256 pinned, no upstream signature):
     re-verify on each tag bump, or build them from a pinned source commit.
   - Sign `SHA256SUMS` of the Linux artifacts (review L7), and run `updpkgsums` on the PKGBUILD once
     `_commit` is a real release commit (pacman >= 6.1 git checksum instead of `SKIP`).
   - LGPL-3 Qt in the AppImage (bundled as replaceable `.so` files): add the written source offer, or
     ship/link the exact Qt 6.2.4 sources used by the `ubuntu:22.04` build image, with each release.
8. Third-party license notices for macOS and Windows (found in the Linux phase 04 review fix
   round, 2026-11-11): neither app ships a license report today, yet both link `fsync-ffi`, which
   statically compiles SQLCipher (BSD-3, Zetetic LLC, notice required in binary distributions) and
   vendored OpenSSL (Apache-2.0). Generate a `cargo about` report for `fsync-ffi` (reuse
   `apps/linux/about.toml`: the libsqlite3-sys clarify and the pruned `accepted` list) and ship it
   in the app bundle / installer (About or Acknowledgements).

## Todo List
- [ ] Mac signing + notarization CI
- [ ] Sparkle + appcast
- [ ] Win Trusted Signing CI
- [ ] Velopack + firewall hook
- [ ] Update hosting + channels
- [ ] Runbook
- [ ] Beta dry-run with update
- [ ] Linux supply chain (L5): base image digests, pinned rustup-init, `--proto-redir`, linuxdeploy trust, signed SHA256SUMS, PKGBUILD git checksum, AppImage Qt LGPL source offer
- [ ] macOS / Windows third-party license report (SQLCipher BSD-3, OpenSSL)

## Success Criteria
- Fresh install on clean macOS & Windows with no Gatekeeper/SmartScreen block (SmartScreen reputation may need time — document); beta N → N+1 auto-update works on both OS; release from tag < 60 min unattended.

## Risk Assessment
| Risk | L x I | Mitigation |
|---|---|---|
| Signing identity not ready | M x H | started phase 01; internal beta with ad-hoc/test certs |
| Notarization failures (unsigned nested dylib) | M x M | `codesign --verify --deep --strict` + `spctl` check in CI |
| SmartScreen warnings for new publisher | H x M | Trusted Signing builds reputation; communicate in beta |
| Leaked update signing key | L x H | keys only in CI secrets/HSM; rotation procedure in runbook |

## Rollback
- Pull bad version from feed (appcast/Velopack releases) within minutes; ship hotfix N+1; backend keeps N-1 compatibility.

## Security Considerations
- Update feed HTTPS only + signature verification; no admin install (per-user); minimal entitlements; SBOM + `cargo audit`/`dotnet list package --vulnerable` in release CI.

## Next Steps
- MVP GA; P2 backlog (per-device targeting, folders, OIDC, encrypted history).
