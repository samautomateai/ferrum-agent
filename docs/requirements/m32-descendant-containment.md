# M3.2 — Descendant process containment requirements

Status: proposed; review required. Scope: Linux AMD64/ARM64 and Windows AMD64.

## Problem
M3.1 guarantees direct-child supervision, but an extension worker can spawn descendants that outlive it. M3.2 must contain and terminate the complete worker process tree on normal shutdown, crash, cancellation, deadline expiry and shared-runtime teardown.

## Requirements
- **M32-R1** Containment MUST be established before worker-controlled code executes. No uncontained startup window; fail closed if containment cannot be established.
- **M32-R2** On Linux, use an isolated process group/session with verified process-group signaling; document and test limitations of setsid, process escape and daemonization. Evaluate cgroup v2 for stronger containment as a separate capability, never imply process groups are a security sandbox.
- **M32-R3** On Windows, use a Job Object with kill-on-job-close and reliable assignment before worker-controlled execution. Handle inherited jobs, nested-job restrictions, and assignment failures explicitly.
- **M32-R4** Containment ownership MUST survive shared Tokio runtime teardown and be released only after cleanup. Cleanup must be idempotent and bounded for the caller.
- **M32-R5** Preserve M3.1 sync facade, framed protocol, deadline semantics, NotStarted/Unknown distinction and no-replay guarantee.
- **M32-R6** No secrets, payloads, environment values or extension source in process-tree diagnostics. Report OS errors by category.
- **M32-R7** Reject unsupported containment environments rather than silently degrading to direct-child-only termination.
- **M32-R8** Distinguish process-tree containment from a sandbox: no claims about filesystem, network, credentials or hostile escape prevention.

## Acceptance
- **M32-A1** Real worker fixture spawns child and grandchild; all exit on graceful shutdown, forced timeout, crash and Drop.
- **M32-A2** Repeat during shared-runtime teardown; assert OS-level descendant termination, not merely parent PID disappearance.
- **M32-A3** Inject containment creation/assignment failure and prove worker-controlled code never runs.
- **M32-A4** Concurrent workers cannot terminate one another; 64-owner cap and cleanup paths remain correct.
- **M32-A5** Windows AMD64, Linux AMD64 and Linux ARM64 CI green, >=90% measured line coverage, security/quality checks and source limits intact.
- **M32-A6** Record actual measured coverage, CI links, platform constraints, explicit maintainer sign-off and rollback.

## Out of scope
Container sandboxing, untrusted code execution guarantees, cgroup delegation setup by default, Pi extension parity, new user-facing APIs.

## Open decisions
1. Linux baseline: process groups only, or mandatory delegated cgroup v2 for stronger coverage?
2. Windows creation/assignment: suspended process bootstrap and required OS APIs/dependencies?
3. Behavior when running under an existing restrictive Windows Job Object?
4. Bounded caller timeout versus asynchronous completion proof for descendant cleanup?
