# M1.8 — Reliable exact-match editing

## Scope
- The `edit_file` tool MUST reject empty or ambiguous match text.
- The tool MUST preserve untouched bytes, including CRLF, UTF-8 and final newline.
- The tool MUST optionally accept `expected_sha256` for optimistic concurrency.
- The tool MUST reject oversized inputs and outputs (>1 MiB), no-op edits and stale hashes.
- A second read MUST detect common concurrent changes before replacement.
- Existing `--allow-write` gating MUST remain mandatory.

## Tests (TDD)
- Single-match CRLF preservation, stale digest, malformed digest, duplicate matches,
  no-op, size limit and Unicode replacement.

## Deferred / limitations
- Current path canonicalization plus temporary-file rename is vulnerable to filesystem
  races and does not provide race-free workspace confinement.
- The pre-rename second read is NOT atomic compare-and-swap.
- Windows replacement semantics and directory fsync require platform-specific validation.
- Do not treat these tools as safe for adversarial workspaces until handle-based traversal,
  no-follow semantics and platform-specific atomic replace are implemented.
