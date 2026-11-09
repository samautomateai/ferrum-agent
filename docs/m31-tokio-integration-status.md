# M3.1 Tokio transport integration status (PR #10)

Status: **draft / CI-green; acceptance incomplete**. See [Ferrum readiness checkpoint](ferrum-readiness-checkpoint.md). The production `ExtensionWorker` synchronous facade now delegates to the shared Tokio supervisor. The old per-worker writer and reader threads have been removed from the facade. The configured CI matrix passed at commit `c0d55c8` ([run 42581802633](https://github.com/samautomateai/ferrum-agent/actions/runs/42581802633)); subprocess fault-injection and process-reaping guarantees are not yet proven.

## Implemented on branch
- Register the Tokio transport and lifecycle modules in the production binary.
- Preserve the synchronous `ExtensionWorker` API for load, invoke, emit and shutdown.
- Use the process-wide runtime bridge; reject reentrant calls from Tokio contexts.
- Translate supervisor outcomes into sanitized `RuntimeFailure` codes. Unknown outcomes remain non-retryable and are never automatically replayed.
- Retain an actor JoinHandle and capture its runtime handle for cleanup from synchronous Drop contexts.
- Add real-worker concurrency, repeated start/stop, deadline and synchronous-drop regression tests.

## Blockers before merging
1. **CI verified at baseline:** rustfmt, quality, tests, Linux amd64/arm64 and Windows amd64 passed ([run 42581802633](https://github.com/samautomateai/ferrum-agent/actions/runs/42581802633)). Record the actual measured coverage figure before milestone sign-off; keep subsequent commits green.
2. Prove that the supervisor always reaps the direct child after cancellation, shutdown timeout, and Drop; runtime shutdown can still interrupt an async reaper. Do not claim guaranteed cleanup yet.
3. Add subprocess fault-injection for broken pipe, malformed and truncated responses, and blocked shutdown.
4. Validate actual CLI extension compatibility and no regression in event and invoke behavior.
5. M3.2 still owns Windows Job Object and Linux process-group/cgroup descendant containment; killing a direct child does not imply tree cleanup.

Do not merge until these are resolved; do not relax quality gates or add Docker requirements.

## Cohesive M3.1 verification batch (pending CI)

- Expose direct worker PID and actor exit observer without changing the IPC wire format.
- Assert actor cleanup completion after graceful shutdown, worker crash, timeout and synchronous Drop.
- Verify event followed by invoke against the real worker binary.
- **Not yet proven:** OS-level PID disappearance, runtime teardown, blocked writes/shutdown and descendant containment. These remain exit blockers.

## Additional reliability checks (pending CI)

- Regression: timed-out side-effecting invocation must not leave a usable worker or replay the request.
- Regression: malformed JSON must close the worker and finish actor cleanup.
- Transport module comment corrected to reflect production use.
- OS process-liveness, blocked stdin/shutdown, runtime teardown and tracing remain outstanding.

## Linux direct-child liveness verification (pending CI)

- Add Linux-only `/proc/<pid>` disappearance assertions following `Child::wait` on graceful shutdown, Drop, and unknown-outcome timeout.
- This tests direct-child disappearance on Linux AMD64/ARM64, not Windows reaping, runtime teardown, or descendants.
- Latest verified baseline: [CI 42589940863](https://github.com/samautomateai/ferrum-agent/actions/runs/42589940863), all six jobs passed.
