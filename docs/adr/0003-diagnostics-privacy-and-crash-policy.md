# ADR-0003 — Diagnostics privacy, crash capture, and retention

Status: Proposed for M3.0 review

## Decision
Ferrum operates local-first with safe metadata diagnostics enabled by default and remote export disabled by default. Full memory dumps are disabled in Ferrum by default and require explicit opt-in; OS-configured dumps may still occur independently.

1. Instrument only allowlisted typed fields. Do not rely on regex redaction as the primary defense: arbitrary strings, panic payloads, stderr and stack traces may include secrets. Drop or classify them before emission.
2. Each diagnostic event has a schema version, correlation ID, timestamp, component, severity and safe error code. Session IDs and user paths are not metric labels.
3. Store diagnostic metadata under an application-specific user data directory, never the current repository by default. Use owner-only permissions where supported; fail closed if restrictive permissions cannot be established for sensitive artifacts.
4. Bound local diagnostic storage to 100 MiB by default, including rotation and retention. Failure to write diagnostics must not recursively trigger logging or block agent shutdown.
5. Capture worker stderr into a 256 KiB in-memory ring for *internal debugging only*. Never put raw stderr into default logs or support bundles; discard on exit unless explicitly requested under a sensitive-data consent flow.
6. Crash metadata (non-memory) is enabled by default. Windows minidumps and Linux cores are opt-in, require explicit warning that secrets can be present, and must not be uploaded automatically.
7. Support bundles are opt-in, allowlist-based, capped at 25 MiB, and scanned for test canaries. A clean canary scan is not proof of universal secret removal. Full dumps require separate authorization.
8. Symbols are distributed/stored separately with exact binary identity. Never publish a user dump as a release artifact.
9. No prompt, response, tool arguments/results, environment, authorization headers or arbitrary exception `Debug` strings are emitted by default.

## Failure modes
- Disk full: drop new diagnostic records, increment in-memory dropped-record counter, continue core work if safe.
- Permissions unavailable: disable sensitive artifact capture and emit a minimal sanitized console warning once.
- Log queue saturated: drop lower-priority events with counters; do not block supervisor.
- Panic/abort/OOM: best-effort metadata only; reliable in-process dump capture is not guaranteed.

## Alternatives rejected
Always-on core dumps, raw stderr logging, unrestricted tracing field serialization, and automatic cloud telemetry.

## Acceptance evidence
Secret-canary tests across all sinks; dump opt-in/opt-out integration tests; disk-full and permission-denied injection; retention and rotation tests; cross-platform directory permission checks.
