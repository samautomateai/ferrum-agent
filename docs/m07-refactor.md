# M0.7: module extraction and source-limit remediation

## Change scope

- Extracted `tools`, `stream`, and `sessions` modules from the monolithic CLI.
- Preserved existing public CLI arguments and regression test fixtures.
- Wrapped long Rust source lines without suppressing the 120-column policy.
- Kept mandatory 90% line coverage and dependency/security gates unchanged.

## Verified locally

- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 6 passed.
- `python3 scripts/check_source_limits.py --max-lines 600 --max-width 120`: PASS.

## Unverified / blockers

- Rust compiler and Cargo unavailable in this execution environment.
- `Cargo.lock` missing: the quality-contract check must continue to fail.
- `cargo fmt`, Clippy, Rust tests, measured coverage, RustSec, and cross-platform builds not executed.
- Existing GitHub workflow uses floating action references; pin to reviewed commit SHAs before treating CI as secure.
- SQLite and filesystem race protections still require further threat-model-driven tests.

## Acceptance gate

M0.7 is not accepted until the repository compiles with `--locked`, the full `task quality` command passes, and measured coverage is >=90%.
