# Dependency and toolchain modernization

## Baseline (2026-11-08)

- Rust stable: 1.99.1, pinned in `rust-toolchain.toml` for reproducibility.
- `rustup`: update installation with `rustup self update` (1.29.3 at baseline).
- `cargo audit --deny warnings` fails on RUSTSEC-2024-0436: `paste` 1.0.15, transitively required by `boa_string` 0.20.0.
- The existing Rust CI and 90% line coverage passed before modernization; mandatory quality gates remain blocked.

## Upgrade procedure (do not suppress audit warnings)

1. Record `rustc --version`, `cargo --version`, `rustup --version` and versions of `cargo-llvm-cov`, `cargo-audit`, `cargo-deny`, `actionlint`, `zizmor`, `gitleaks`, and `task`.
2. Run `cargo outdated --root-deps-only` and `cargo tree -i paste` to inventory direct and transitive upgrades.
3. Upgrade Boa as a dedicated compatibility change. Confirm the chosen version eliminates the `boa_string -> paste` dependency, or document an upstream blocker without disabling audit.
4. Review and upgrade direct dependencies to their latest compatible stable releases, including major versions only after API and MSRV review. Regenerate and commit `Cargo.lock` using `cargo update`; do not hand-edit the lockfile.
5. Re-run extension and worker protocol tests, JavaScript runtime tests, and all `task quality` gates. Require >=90% line coverage and green Linux AMD64/ARM64 and Windows AMD64 jobs.
6. Review supply-chain security and licenses with `cargo audit --deny warnings` and `cargo deny --locked check advisories bans licenses sources`; review action and CLI versions separately.
7. Merge only after all required checks pass; remove the temporary M2 bootstrap workflow before final merge.

## Tooling policy

Use current stable Rust with an explicit toolchain pin. Update this pin deliberately with a tested PR, not a silent CI upgrade. Record versions for external quality CLIs; review their releases regularly, but do not use unreviewed floating tool versions for reproducible release gates. `cargo update` only updates within manifest constraints; major-version changes require explicit manifest edits and compatibility testing.
