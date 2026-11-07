# M1.9 — Git inspection (read-only)

## Scope
- Provide `git_status` and `git_diff` to the agent tool registry.
- Execute the installed Git executable directly without a shell.
- Forbid repository mutation, hooks, external diff drivers and text converters.
- Reject paths that are absolute, contain parent traversal, or start with a dash.
- Require the workspace to equal the Git repository top-level directory.
- Bound stdout to 64 KiB and stderr diagnostics to 512 characters.

## Acceptance tests
- Modified tracked file appears in porcelain status and unstaged diff.
- Staged diff excludes unstaged modifications.
- Path traversal, command-option injection and non-repositories fail.
- Git index and working tree remain unchanged after inspection.

## Deferred
- Structured porcelain-v2 parsing, rename-aware diff metadata, safe patch application.
- Bounded execution deadlines and robust cancellation of hung Git subprocesses.
- Race-resistant workspace confinement and symlink-aware path policies.
- Git availability on every target and full cross-platform verification.
