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

## Todo List
- [ ] Mac signing + notarization CI
- [ ] Sparkle + appcast
- [ ] Win Trusted Signing CI
- [ ] Velopack + firewall hook
- [ ] Update hosting + channels
- [ ] Runbook
- [ ] Beta dry-run with update

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
