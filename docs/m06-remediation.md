# M0.6 quality remediation (in progress)

## Acceptance is blocked

- Cargo.lock is missing. Generate it with `cargo generate-lockfile` using a trusted Rust toolchain, review and commit it.
- Rust source-limit gate has 24 violations in src/main.rs. Refactor and format; do not suppress.
- Rust compilation, Clippy, tests, measured 90% coverage, and security scanners have not run here.
- CI must run the same `task quality` as developers; the contract checker verifies this.
- Do not claim Linux ARM64/Windows portability without executing the matrix.
- Pin GitHub Actions to immutable commit SHAs, review permissions, and verify with zizmor.
- Audit transitive and vendored OMP dependencies before redistributing their code.

## Verified locally

- Six Python policy/checker tests pass.
- The policy contract correctly blocks on the missing Cargo.lock.
- Source limit checker correctly reports 24 violations.

## Required commands on a Rust-equipped machine

```bash
cargo generate-lockfile
cargo fmt --all
python3 scripts/check_source_limits.py --max-lines 600 --max-width 120
task quality
```

No gate should be disabled or ignored to make CI green.
