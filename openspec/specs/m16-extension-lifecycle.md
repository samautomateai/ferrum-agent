# M1.6 Extension lifecycle — draft implementation

## Requirements
- M1.6-01: A persistent Boa worker MUST retain extension state between tool calls and events.
- M1.6-02: `pi.on(event, handler)` MUST reject non-callable handlers and permit multiple callbacks.
- M1.6-03: An event MUST await Promise microtasks and propagate handler failures.
- M1.6-04: A shutdown request MUST emit `shutdown`, acknowledge, and exit the worker loop.
- M1.6-05: Worker IPC MUST retain frame size and request deadline limits.
- M1.6-06: A timed-out tool call MUST NOT be replayed automatically.

## Verification
- Unit tests: event state preservation, invalid callbacks, event rejection.
- Integration: event dispatch followed by stateful tool invocation and shutdown acknowledgement.
- Manual: cargo fmt, clippy, cargo test, cargo llvm-cov >= 90%, cargo deny, cargo audit,
  Gitleaks, OS builds, end-to-end agent lifecycle.

## Known limitations
- This is a partial lifecycle protocol, not complete upstream Pi event parity.
- The agent loop does not yet automatically broadcast lifecycle events.
- Shutdown callback errors are currently ignored by the worker.
- No OS sandbox or bounded scheduling queue; only one in-flight request per worker.
- Boa remains trusted-extension-only; memory/CPU isolation not established.
- Cargo.lock and Rust compilation remain pending manual verification.
