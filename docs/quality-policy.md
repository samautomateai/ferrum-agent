# Ferrum quality policy (M0 onward)

Every production change starts with an OpenSpec change and failing tests, followed by the minimum implementation and refactoring. No exception for AI-generated code.

## Required gates

- `task quality` is the local canonical quality command.
- Rust source: <=120 characters per line, <=600 lines per file. Generated/vendor code is excluded; exceptions require an ADR.
- Rustfmt; Clippy `-D warnings`; all unit/integration tests; >=90% line coverage via llvm-cov.
- Cargo audit (RustSec), cargo deny (advisories, license, source, dependency policy).
- Gitleaks for secrets; actionlint and zizmor for GitHub workflow checks.
- Release build and cross-platform Linux amd64/arm64 and Windows amd64 test/build jobs.
- No `allow_failure`, blanket `#[allow(...)]`, or ignored tests to evade gates.
- Treat warnings and tool errors as failures. Review license allowlist against actual dependencies; exceptions require documented approval.

## Additional acceptance gates (not yet implemented)

- Threat model and adversarial tests for filesystem and shell tool boundaries.
- Mutation testing on critical parsers and permission checks.
- SBOM generation, signed release provenance, and dependency update automation.
- Strictly pinned action SHAs, tool versions, and a documented upgrade policy.
- Measured branch coverage and security regression suites.

## Current status

Configuration is present but not verified. The existing Rust source violates the 120-character line limit, and the repository lacks a committed Cargo.lock. Do not mark M0 accepted until the tools run and every gate passes. CI workflow actions also require pinning and validation before production use.
