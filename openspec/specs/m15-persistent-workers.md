# M1.5 — Persistent extension workers (OMP-inspired)

## Motivation
Keep the lightweight agent core and extension flexibility of Pi/OMP while reducing repeated JS initialization.
This is an independently developed implementation, not copied OMP source.

## Requirements
- One long-lived Boa subprocess per extension, with independent JS global state.
- Newline-delimited JSON protocol with bounded request and response frames.
- Deadline for every tool invocation. Kill/reap workers on timeouts, broken pipes or malformed replies.
- Never automatically replay a tool invocation: it may have side effects.
- Keep protocol output separate from extension logs; stderr is not a protocol channel.
- Reject duplicate native/extension tool names at registration.
- Worker exit is idempotent; no orphan processes on host drop.
- No claims of OS-level sandboxing; extensions are trusted-only.

## Tests / acceptance
- Consecutive calls increment persistent JS state.
- Promise handlers settle and return JSON.
- Unknown tool errors do not terminate a healthy worker.
- Timeout terminates the worker without replay.
- Truncated/oversized frames fail closed.
- Rust compilation, 90% coverage, security and platform gates are MANUAL VERIFICATION PENDING.

## Deferred
- OMP-like event lifecycle, request cancellation token, bounded multi-request queue, process resource limits,
  per-extension permission broker, TS/Node compatibility and automatic restart for safe initialization only.
