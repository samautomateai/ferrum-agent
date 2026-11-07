# M1.7: Deterministic bounded repository search (first slice)

## Requirements
- REQ-1: Search uses Rust-native ignore-aware walking and regex matching; no shell or rg executable required.
- REQ-2: Results are ordered deterministically and use workspace-relative paths and one-based line numbers.
- REQ-3: Do not follow symlinks outside the canonical workspace; skip binary/non-UTF-8 files and files over 1 MiB.
- REQ-4: Bound output to 100 matches and 32 KiB and visibly mark truncation.
- REQ-5: Invalid regex produces a tool error, not an empty successful result.

## Acceptance tests
- Stable ordering and relative line-number formatting
- Invalid pattern rejection
- Match count truncation
- Binary and oversized file skipping

## Deferred
- OMP-inspired edit reliability and LSP integration
- Benchmarks against OMP and ripgrep on representative repositories
- Git-aware diff/working-tree operations
- Windows atomic replace semantics
- Race-resistant file confinement (canonicalize alone is insufficient under adversarial mutation)

## Security note
This is a search improvement, not a security boundary. Directory races and hostile extensions
remain outside the trust model until filesystem handles and OS sandboxing are implemented.
