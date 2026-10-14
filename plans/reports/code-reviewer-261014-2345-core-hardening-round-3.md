# Round 3 review: core/hardening @ 0d2d665
Score 9.6/10. 0 critical, 0 high. Gates: fmt, clippy -D warnings clean; cargo test --workspace all pass.
L1-L5 verified fixed (see final message). Open: M1 lingering-engine task can block restart forever; M2 stop() 5s wait exceeds 3s teardown timeout (lock/engine dropped on cancel); L-a blocking std mutex in async logout; L-b docs wording for on_session_changed not yet in docs/ffi-contract.md.
