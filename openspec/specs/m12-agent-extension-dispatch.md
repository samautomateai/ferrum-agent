# M1.2 — Extension tool dispatch (TDD)

## Requirements
- Load repeatable `--extension <path>` flags from explicitly trusted local JavaScript files.
- Advertise registered extension tool JSON schemas alongside native tools in Chat Completions requests.
- Reject duplicate names across native tools and all extensions before any provider request.
- Route returned tool calls to the registered Boa handler and persist the JSON result in the session.
- Preserve native tool behavior and report handler failures as tool errors.

## Acceptance tests
- Schema is included in merged definitions.
- Handler receives JSON arguments and returns JSON result.
- Native and extension collisions are rejected.
- Cross-extension collisions are rejected.
- Invalid schema rejected.

## Deferred / risks
- Boa execution remains synchronous and trusted-only. No interrupt or sandbox.
- Async Promise execution, module resolution, TS, Pi API parity and cancellation are M1.3+.
- M0.8 manual verification remains a release blocker. No claims of Rust compilation or 90% coverage.
