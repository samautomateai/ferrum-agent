# M3.1 requirement acceptance matrix

Status: **implementation evidence verified; acceptance pending**. Updated 2026-11-09.
Sources: [requirements](m31-worker-supervision.md) and [design](../design/m31-worker-supervision.md).

Validated CI baseline: [run 42688251419](https://github.com/samautomateai/ferrum-agent/actions/runs/42688251419) at commit `fc46cd4`: rustfmt, quality, tests, Linux AMD64, Linux ARM64, Windows AMD64 all passed.

| ID | Evidence | Remaining requirement | Status |
| --- | --- | --- | --- |
| M31-R1 | `src/supervisor_runtime.rs`, sync bridge tests | Maintainer acceptance | Implemented; unaccepted |
| M31-R2 | Bounded supervisor actor, cancellation and Drop tests | Runtime-independent direct-child reap guarantee | Partial |
| M31-R3 | 2 MiB framing, deadlines, bounded queue, blocked-write tests | Maintainer acceptance | Implemented; unaccepted |
| M31-R4 | Pre-dispatch versus dispatched cancellation, no replay, fault tests | Maintainer acceptance | Implemented; unaccepted |
| M31-R5 | Graceful shutdown, OS liveness, runtime teardown regression | Prove direct-child wait/reap after runtime teardown, not merely disappearance | Partial |
| M31-R6 | Real worker handshake, invoke and shutdown tests | Maintainer compatibility sign-off | Implemented; unaccepted |
| M31-R7 | Sanitized error kinds | Structured payload-free tracing and redaction verification | Partial |
| M31-R8 | Direct-child scope explicitly documented | Confirm M3.2 descendant handoff | Proposed |
| M31-N1 | Bounded channel/frame; blocked write/read and deadline tests | Maintainer acceptance | Implemented; unaccepted |
| M31-N2 | Three supported OS/architecture CI jobs green | Maintainer acceptance | Implemented; unaccepted |
| M31-N3 | Rustfmt, quality and test jobs green | Publish measured line coverage >=90%; CI pass alone does not establish coverage | Unverified |
| M31-N4 | Truncated, oversized, blocked read/write, broken pipe, crash, cancellation and teardown tests | Runtime-independent reaping proof | Partial |

## Release gate

Do not mark M3.1 complete or PR #10 ready for review until the following evidence exists:

1. Runtime-independent direct-child process ownership and bounded termination/reaping, verified across runtime shutdown on Linux and Windows.
2. Structured lifecycle tracing that excludes payloads, secrets, source and environment values, with redaction tests.
3. Recorded measured line coverage meeting the >=90% requirement, or an explicitly approved exception.
4. Maintainer approval of requirements, design and remaining risks.

A successful process-disappearance check is **not** evidence that the parent waited for the child.
