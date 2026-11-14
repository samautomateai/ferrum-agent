# Ferrum delegation runbook

This runbook is a **manual pilot**, not evidence of enforced tool isolation.

1. Ensure a reviewed and approved Pi subagent extension is installed; check exact version.
2. Start Pi from repository root and read `AGENTS.md`.
3. Identify the approved OpenSpec and acceptance criteria; stop if approval is missing.
4. Delegate design scrutiny to a fresh read-only agent and record blocking findings.
5. Allow an implementer in a separate worktree only after maintainer approval.
6. Delegate independent code and security reviews to two fresh, read-only child sessions.
7. Run `task quality` and inspect GitHub checks on the exact commit.
8. Record review findings, severity, remediation and human approval in the PR.

Suggested pilot prompt after extension approval:

> Delegate two independent, read-only reviews of the current PR: one correctness/TDD review and one security/platform review. Do not permit file edits, shell execution, git operations or secret access. Return findings with file/line evidence and stop if tool restrictions cannot be enforced.

Never assume a prompt can enforce permissions. If the extension cannot impose a child-specific tool allowlist, run reviews in a separately sandboxed environment or do not delegate.
