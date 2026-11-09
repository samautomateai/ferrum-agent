# M3.1 requirement acceptance matrix

Status: **implementation evidence verified; acceptance pending**. Updated 2026-11-09.
Sources: [requirements](m31-worker-supervision.md) and [design](../design/m31-worker-supervision.md).

Validated CI baseline: [run 42688251419](https://github.com/samautomateai/ferrum-agent/actions/runs/42688251419) at commit `fc46cd4`: rustfmt, quality, tests, Linux AMD64, Linux ARM64, Windows AMD64 all passed.

| ID | Evidence | Remaining requirement | Status |
| --- | --- | --- | --- |
| M31-R1 | `src/supervisor_runtime.rs`, sync bridge tests | Maintainer acceptance | Implemented; unaccepted |
| M31-R2 | Bounded supervisor actor, cancellation and Drop tests | Verify runtime-independent direct-child reap under teardown and runtime-construction failure | Partial |
| M31-R3 | 2 MiB framing, deadlines, bounded queue, blocked-write tests | Maintainer acceptance | Implemented; unaccepted |
| M31-R4 | Pre-dispatch versus dispatched cancellation, no replay, fault tests | Maintainer acceptance | Implemented; unaccepted |
| M31-R5 | Graceful shutdown, OS liveness, runtime teardown regression | Review independent process-owner wait/reap and OS-level teardown evidence | Partial |
| M31-R6 | Real worker handshake, invoke and shutdown tests | Maintainer compatibility sign-off | Implemented; unaccepted |
| M31-R7 | Sanitized error kinds | Review fixed-field JSON lifecycle diagnostics and production logging policy | Partial |
| M31-R8 | Direct-child scope explicitly documented | Confirm M3.2 descendant handoff | Proposed |
| M31-N1 | Bounded channel/frame; blocked write/read and deadline tests | Maintainer acceptance | Implemented; unaccepted |
| M31-N2 | Three supported OS/architecture CI jobs green | Maintainer acceptance | Implemented; unaccepted |
| M31-N3 | Rustfmt, quality and test jobs green | Publish measured line coverage >=90%; CI pass alone does not establish coverage | Unverified |
| M31-N4 | Truncated, oversized, blocked read/write, broken pipe, crash, cancellation and teardown tests | Review direct-child reaping under runtime teardown | Partial |

## Release gate

Do not mark M3.1 complete or PR #10 ready for review until the following evidence exists:

1. Review the implemented independent process-owner thread, prove OS-level wait/reaping across runtime teardown on Linux and Windows, and address runtime-construction failure.
2. Review fixed-field JSON lifecycle diagnostics and redaction tests, including production logging policy.
3. Recorded measured line coverage meeting the >=90% requirement, or an explicitly approved exception.
4. Maintainer approval of requirements, design and remaining risks.

A successful process-disappearance check is **not** evidence that the parent waited for the child.
