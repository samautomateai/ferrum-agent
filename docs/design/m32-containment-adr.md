# ADR M3.2-001 — Platform process-tree containment

Status: accepted for incremental implementation, 2026-11-10. Security claims remain limited to cooperative worker descendants.

## Decision

**Linux:** require an isolated process group as the portable baseline for Linux AMD64 and ARM64. A worker MUST enter its own process group in the pre-exec phase; process-group termination applies to descendants remaining in that group. Evaluate delegated cgroup v2 as an optional stronger backend in a later slice. Do not require cgroup delegation in ordinary CI, and do not describe process groups as a secure sandbox.

**Windows:** require a Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Create the process suspended, assign it to the job, and only then resume its primary thread. If creation, assignment, or resumption fails, terminate and wait for the process and report `NotStarted`. Ordinary Tokio `Command::spawn` followed by assignment is explicitly rejected because it allows an uncontained execution window.

**Ownership:** the independent M3.1 process owner owns containment handles. On forced cleanup terminate the group/job, then reap the direct child. On successful direct-child exit, still clean up descendants before releasing containment. Avoid targeting process groups by PID after reaping and potential PID reuse.

## Rejected alternatives

- Post-spawn Windows Job Object assignment: startup race.
- Linux process groups described as escape-proof: descendants can create a new session/group.
- Mandatory cgroup v2 for all Linux installations: delegated permissions cannot be assumed.
- Silent direct-child-only fallback: violates M32-R7.

## Acceptance and release policy

Start with failing process-tree tests and fail-closed startup tests. Implement Linux and Windows separately, keeping CI green after each slice. Explicitly test descendant liveness, teardown and worker isolation. Retain >=90% line coverage and all quality gates. No merge until all M32-A1..A6 are verified.
