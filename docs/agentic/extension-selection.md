# Pi extension selection — proposed

Status: evaluation; installation is not automated.

## Selected candidate: pi-subagents

Package: `pi-subagents` (https://pi.dev/packages/pi-subagents).
Purpose: delegated independent sessions, parallel reviewers and scripted workflows.
Observed catalog release on 2026-11-10: 0.87.0. Treat this as a candidate version, not an audited pin.

Before installation:
1. Review exact npm tarball, transitive dependencies, install scripts and source against a checksum.
2. Verify installed Pi version satisfies the package's Pi >=1.2.0 requirement.
3. Confirm local child execution, role-specific tool restrictions and fresh review context.
4. Confirm no reviewer has write, shell execution, git push or secret-read permissions.
5. Verify max concurrency, timeout, cancellation, recursion depth and process reaping.
6. Exercise a synthetic malicious repository instruction and verify no privilege escalation.
7. Record package version, checksum, maintainer approval and a rollback procedure.

Suggested **manual** install only after review: `pi install npm:pi-subagents@0.87.0`.
Verify with `pi --version` and Pi's installed-package listing. Do not assume installation succeeded from this document.

## Permission candidate: pi-permission-modes

Package: `pi-permission-modes` (https://github.com/wynainfo/pi-permission-modes).
Do not install until its OS sandbox backend is tested on Linux amd64/arm64 and Windows amd64.
Permission prompts alone are not isolation. In particular, confirm child agents inherit or receive a strictly narrower permission profile.

## Operating model

Architect/scrutinizer/reviewer/security/quality: read-only project access, no repository mutations, no secrets, no unrestricted shell. Implementer: scoped worktree write, restricted shell and network, no merge/push without human approval. CI is the authoritative quality gate.

Do not place a package reference in `.pi/settings.json` before review: Pi may load project packages automatically. Avoid unreviewed install hooks.

## Evaluation acceptance matrix

| Scenario | Required outcome |
| --- | --- |
| Reviewer requests write | Denied and logged |
| Reviewer requests secret | Denied and logged |
| Implementer modifies out-of-scope path | Denied and logged |
| Delegated child times out | Terminated and reaped |
| Two parallel reviews | Distinct sessions and outputs |
| Nested delegation exceeds cap | Rejected |
| Parent session exits | No orphaned children |
| Invalid spec approval | Implementation blocked |
| Quality check fails | PR remains blocked |
| Windows/Linux sandbox unavailable | Fail closed |

Only after these tests pass should we implement deterministic orchestration and promote the extension to an approved dependency.
