# M1.3 — Boa Promise microtasks (limited)

## Scope
- Given a registered handler returning a JSON-serializable value or Promise resolved by Boa microtasks, the agent receives the JSON result.
- Rejected Promises and non-JSON results fail closed.
- Pending Promises with no runnable microtasks fail with an explicit error after bounded queue pumping.
- Existing synchronous extension tools continue to work.

## Explicit exclusions
- This is NOT asynchronous host I/O, timers, preemptive cancellation, process isolation, memory budgeting, or a secure sandbox.
- A synchronous infinite loop in an extension can still hang the process.
- The bounded pump count is NOT a wall-clock deadline. Boa `run_jobs` can itself execute arbitrarily long JavaScript.
- Pi event lifecycle compatibility is not implemented by this milestone.

## Tests (written, Rust execution pending)
- Async handler resolves JSON.
- Promise rejection is reported.
- Never-settled Promise is rejected.
- Promise microtask chaining resolves.
- Existing sync dispatch tests remain valid.

## Acceptance
- cargo test --locked, cargo clippy, 90% line coverage, dependency/security audits, and all cross-platform builds must pass.
- M0.8 and M1.0–M1.2 remain manually unverified and block release.
