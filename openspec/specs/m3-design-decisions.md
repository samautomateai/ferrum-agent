# M3.0 — Decision register and implementation boundaries

Status: PROPOSED FOR REVIEW (not frozen). This record resolves open questions into concrete recommendations; acceptance requires review of ADR-0001/0002/0003.

## Decision register
| ID | Recommended resolution | Validation |
|---|---|---|
| D1 Error contract | Stable error code, sanitized public message, retryability, and outcome status. Unknown outcome is distinct from failed. | Contract and serialization tests |
| D2 Supervisor | Parent-owned state machine; no new requests after drain; idempotent cleanup and process-tree ownership. | Cancellation and orphan tests |
| D3 Budget defaults | 30s per worker call, 5s graceful shutdown, 2 MiB IPC, 1 MiB JS result, 256 KiB stderr ring, 16 pending requests, 512 MiB *target* worker memory. | Limits and overload tests |
| D4 OS enforcement | Treat 512 MiB as enforceable only when Windows Job Object or delegated Linux cgroup v2 configured and verified. If `require_hard_memory_limit=true`, reject startup when unavailable. Otherwise report `unavailable` and operate trusted extensions only. | OS capability probes and fail-closed tests |
| D5 Logs | Rust `tracing`; allowlisted structured fields; local stderr by default, optional bounded rotating files. | Canary leak tests |
| D6 Crash | Metadata by default; full dumps opt-in, no automatic upload. OS crash-dump settings are independent. | Dump policy tests |
| D7 Telemetry | Local counters and spans first; OpenTelemetry opt-in, off by default. No high-cardinality labels. | Schema and disabled-export tests |
| D8 Sessions | Preserve committed SQLite transactions, JSON export atomicity best effort per OS; uncertain side effects never marked successful. | Crash and disk-full tests |
| D9 Retry | Zero retries for tools. Up to two retries for explicitly idempotent network operations with jitter and total deadline. No retries after ambiguous tool side effects. | Fault injection |
| D10 Bundles | Explicit user request; 25 MiB cap; allowlisted sanitized metadata only. Separate consent for memory dumps. | Privacy and size tests |
| D11 CLI | Introduce `ferrum doctor`, `ferrum health`, `ferrum diagnostics` as subcommands. Preserve existing positional-prompt CLI behavior. | Backward compatibility tests |
| D12 Diagnostic failures | Nonblocking best-effort local diagnostics; bounded queue; dropped-event counters. Sensitive artifact permission failure disables capture. | Disk-full and permission tests |
| D13 Deadline overrides | 30s default, 1–600s explicit override; one end-to-end deadline includes queue/write/flush/response/cleanup. Preserve existing 5s behavior until M3.1 implementation explicitly migrates it. | Boundary and compatibility tests |

## Compatibility and migration
- Current `--extension-timeout` defaults to 5 seconds. M3 proposes changing the *future* default to 30 seconds; preserve flag syntax, document migration and add tests before changing behavior.
- Existing worker request/response size cap is 2 MiB and extension arguments/results are capped at 1 MiB. Retain both; avoid duplicate contradictory limits.
- Existing worker uses blocking stdin and detached stdout reader. Deadline redesign must explicitly handle blocked writes and thread cleanup.
- Existing persistent Boa workers are trusted-only. This milestone does not make them secure for untrusted extensions.
- Current CLI accepts a positional prompt. New diagnostics subcommands must not break normal prompts or existing flags.
- Retention, hard memory controls and dump mechanisms must be advertised by capability, not by intent.

## Exit gate
M3.0 is complete only when: ADRs accepted; defaults and exception behavior agreed; requirements-to-tests traceability reviewed; CI impact understood; and PR is reviewed. Until then, do not begin production implementation.
