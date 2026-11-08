# ADR-0002 — Explicit resource budgets and platform enforcement

Status: Proposed

## Decision
Keep application-level byte and queue caps separate from OS-enforced process memory/CPU limits. Report each limit's status as `enforced`, `best_effort`, or `unavailable` in diagnostics. Never silently describe a best-effort check as memory protection.

Use a parent-owned runtime supervisor for worker process-tree termination. Windows Job Objects and Linux process groups are baseline cleanup strategies; Linux cgroups v2 hard memory limits require delegated control and are optional. A crash in an extension must not kill the parent. Timeout outcomes are marked unknown when external side effects may have occurred.

Default worker deadline is proposed as 30 seconds and memory target 512 MiB, subject to acceptance tests and benchmark evidence. Existing 2 MiB IPC caps remain in force.

## Security considerations
- Process separation is not a sandbox; workers currently inherit environment and filesystem access.
- Core dumps/minidumps may include secrets and are disabled in Ferrum's default diagnostic policy.
- OS-level core-dump policies remain outside Ferrum's exclusive control.
- Do not log raw child stderr: it can contain extension-provided secrets.
- Reject resource-limit overrides that cannot be enforced if policy explicitly requires hard enforcement.

## Consequences
Enforcement differs by OS and privileges. Tests must verify both supported and unsupported paths, including fail-closed policy behavior. The milestone must not claim universal hard memory protection.
