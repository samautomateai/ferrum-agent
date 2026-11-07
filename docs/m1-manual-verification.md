# Manual verification queue

## M0.8 — blocked
- [ ] `cargo generate-lockfile` (commit lockfile)
- [ ] `cargo fmt --all`
- [ ] `cargo clippy --locked --all-features --all-targets -- -D warnings`
- [ ] `cargo test --locked --all-features --workspace --all-targets`
- [ ] `cargo llvm-cov --locked --all-features --workspace --all-targets --fail-under-lines 90`
- [ ] `cargo audit --deny warnings` and `cargo deny --locked check advisories bans licenses sources`
- [ ] Gitleaks, actionlint, Zizmor
- [ ] Linux ARM64/AMD64 and Windows AMD64 CI builds

## M1.0 — blocked
- [ ] Confirm Boa 0.20 builds with selected Rust compiler version
- [ ] Verify five extension manifest unit tests
- [ ] Measure startup, peak memory and dispatch overhead
- [ ] Review Boa's runtime limits and license inventory

Run `task quality` only after installing the tools shown in `task doctor`.
No M0/M1 release accepted until the entire quality gate passes.
