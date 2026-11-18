# How to build a release on the remote hosts and publish it

This guide builds the downloadable clients for all three platforms and publishes them as a GitHub
release. The Linux host drives everything. Windows and macOS are built over ssh.

| Platform | Host | Output |
|---|---|---|
| Linux | the Linux host (Docker) | `.deb`, `.rpm`, AppImage |
| Windows | `waso` (Windows 11 x64, user `dev`) | `fsync-windows-win-x64.zip`, `fsync-windows-win-arm64.zip` |
| macOS | `macmini` (Apple Silicon, user `dev`) | `fsync-macos-arm64.zip` |

First release made this way: [v0.1.0](https://github.com/hoadh/fsync-native/releases/tag/v0.1.0)
(2026-11-17).

## Before you start

1. Merge the release commit to `main` and push it. Build every platform from that same commit.
2. Check the hosts: `ssh -G waso | grep '^user '` and `ssh -G macmini | grep '^user '` must show
   `dev`.
3. Pick the version (for example `0.1.0`). The Linux packages take it from `apps/linux/Cargo.toml`.
   The macOS script takes it from `--version`.
4. Check free disk space. Windows needs about 20 GB (Rust `target` for two triples, plus .NET).
   macOS needs about 8 GB (rustup, two Rust triples). On `macmini`,
   `~/Library/Developer/Xcode/DerivedData` is a safe cache to delete when space is short.

All builds use the production backend `https://api-fsync.10z.one` and both relays. Linux and
Windows have these values built in. macOS gets them from the `--api` and `--relay` flags below.

## 1. Linux (local)

```bash
git checkout main && git pull
FSYNC_RELEASE_STRICT=1 apps/linux/scripts/build-release.sh
ls apps/linux/dist     # .deb, .rpm, AppImage, SHA256SUMS
```

`FSYNC_RELEASE_STRICT=1` refuses a dirty tree. The build takes about 5 minutes.

## 2. Windows (`waso`)

Use a fresh clone. Do not reuse old worktrees: they keep large `target` folders.

```bash
ssh waso 'cd /d C:\Users\dev\workspace && git clone -q https://github.com/hoadh/fsync-native.git fsync-build'
ssh waso 'cd /d C:\Users\dev\workspace\fsync-build && powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-windows-release.ps1'
scp 'waso:workspace/fsync-build/dist/windows/*.zip' <local-release-dir>/
```

`scripts/build-windows-release.ps1` builds both DLLs and runs `Fsync.CoreBindings.Tests` and
`Fsync.App.Tests`. Then it publishes one self-contained zip per RID and prints the SHA-256 of each zip.
It stops in these cases:
- the tree is dirty;
- a DLL was not rebuilt (a stale DLL);
- the regenerated `Generated/fsync_ffi.cs` differs from the commit in more than line endings.

A first build takes about 15 minutes.

## 3. macOS (`macmini`)

```bash
ssh macmini 'export PATH="$HOME/.cargo/bin:$PATH"; cd ~/workspace && rm -rf fsync-build &&
  git clone -q --depth 1 https://github.com/hoadh/fsync-native.git fsync-build && cd fsync-build &&
  scripts/build-xcframework.sh &&
  apps/macos/scripts/build-fsync-mac-app.sh --version 0.1.0 --api https://api-fsync.10z.one \
    --relay https://relay-fsync.10z.one,https://relay-sg-fsync.10z.one &&
  cd apps/macos/build && ditto -c -k --keepParent FsyncMac.app ~/workspace/fsync-macos-arm64.zip'
scp macmini:workspace/fsync-macos-arm64.zip <local-release-dir>/
```

If rustup is missing, install it first:
`curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none`.
`rust-toolchain.toml` then installs the pinned toolchain.

Check the bundle:

```bash
ssh macmini 'plutil -p ~/workspace/fsync-build/apps/macos/build/FsyncMac.app/Contents/Info.plist | grep -E "Fsync|ShortVersion"'
```

Without a Developer ID certificate the app is ad-hoc signed. Gatekeeper blocks the first launch, so
put the steps to open it in the release notes. To sign with Developer ID, see the `--sign`,
`--profile` and `--notary-profile` flags of the script. These flags need both the "Developer ID
Application" certificate (`.p12` with its private key) in the keychain and a Developer ID
provisioning profile for `app.fsync.mac`. The profile alone is not enough.

The app is arm64 only: `swift build` builds for the host architecture.

## 4. Publish

```bash
cd <local-release-dir>
cp ~/Work/fsync-native/apps/linux/dist/fsync-linux* .
sha256sum fsync-* > SHA256SUMS
gh release create v0.1.0 --target main --prerelease --title "fsync v0.1.0 (preview)" \
  --notes-file notes.md ./*
gh release view v0.1.0 --json assets --jq '.assets[] | "\(.name) \(.size)"'
```

`--target` takes a branch name or a full commit sha. A short sha fails with
`target_commitish is invalid`.

## 5. Clean up

```bash
ssh waso 'rmdir /s /q C:\Users\dev\workspace\fsync-build'
ssh macmini 'rm -rf ~/workspace/fsync-build ~/workspace/fsync-macos-arm64.zip'
```

## Troubleshooting

- **`cargo.exe`: "The system cannot execute the specified program", or error 448 "untrusted mount
  point" (Windows, over ssh).** The rustup proxies in `C:\Users\dev\.cargo\bin` are symlinks to
  `rustup.exe`, and Windows does not run them in an ssh session. This makes the DLL build fail, so the
  old DLL stays. Replace the symlinks with hard links. This was done on `waso` on 2026-11-18. Run it
  again after a rustup reinstall that brings the symlinks back:

  ```powershell
  $bin = "$env:USERPROFILE\.cargo\bin"
  Get-ChildItem $bin -Filter *.exe | Where-Object LinkType -eq 'SymbolicLink' | ForEach-Object {
      Remove-Item $_.FullName -Force
      cmd /c mklink /H "$($_.FullName)" "$bin\rustup.exe" | Out-Null
  }
  ```
- **Inline PowerShell over ssh fails with "The filename, directory name, or volume label syntax is
  incorrect".** The ssh default shell on `waso` is `cmd`, and the quoting breaks. Use one of these
  instead: copy a `.ps1` with `scp` and run it with `powershell -File`, use
  `powershell -EncodedCommand <base64 of UTF-16LE>`, or keep to plain `cmd` syntax.
- **The WinUI app crashes when started from ssh.** ssh runs in session 0. The release build does not
  start the app. For GUI checks, see "Cross-OS e2e" in `apps/windows/README.md`.
