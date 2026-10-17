# Code Standards

## File size and modules
- Every source file < 200 lines (test files may be longer). Split by concern (one protocol, one repo, one route group per file).
- Rust: `snake_case.rs`; Swift/C#: `PascalCase`; docs/scripts: `kebab-case`.
- `fsync-ffi` holds adapters only — no business logic. `fsync-proto` holds wire types only — no I/O.

## Errors
- Libraries (`fsync-proto`, `fsync-core`, `fsync-ffi`): `thiserror` enums, no `unwrap()`/`expect()` outside tests.
- Binaries (`fsync-cli`, `fsync-backend`): `anyhow` allowed at the edges.
- Never log secrets, keys, tokens, clipboard content or file names.

## Wire formats
- postcard (serde) for wire frames and signed bytes; JSON for HTTP.
- Protocols versioned by ALPN (`fsync/clip/2`, `fsync/file/1`, `fsync/pair/1`). Never change v1 semantics — add v2.
- Signatures are domain-separated (`"fsync-devlist-v1" || body`).

## Security
- Secret material wrapped in `zeroize` types; constant-time compares for MACs/codes.
- No plaintext clipboard/file bytes ever reach backend or relay.
- No secrets in repo; `.env.example` only.

## Test-only features
- `fsync-core` features `test-hooks` (fault injection) and `test-relay-ca` (trust a private relay CA from `FSYNC_TEST_RELAY_CA`, module `net/test_relay_ca.rs`). Neither is default, and neither may reach `fsync-ffi`.
- `fsync-cli/test-relay-ca` is enabled only by the netns harness (`tests/e2e/netns/run-in-container.sh`).
- `scripts/build-xcframework.sh` and `scripts/build-windows-dll.ps1` fail the build if `cargo tree -e normal,build -p fsync-ffi` lists either feature. Keep it that way when adding a test feature.
- Test certificates are generated per run and never committed.

## Quality gates (CI)
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
