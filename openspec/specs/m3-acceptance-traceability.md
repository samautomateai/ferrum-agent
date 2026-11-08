# M3 — Requirement-to-test traceability

Status: Proposed. Tests below are not yet implemented or executed.

| Requirement | Test class | Evidence / expected assertion | Phase |
|---|---|---|---|
| M3-01 Typed errors | Unit/property | Public code stable; cause chains never leak secrets; unknown outcome preserved | M3.1 |
| M3-02 Supervisor | Unit/integration | Drain rejects new work; cancellation reaps owned worker tree | M3.1 |
| M3-03 Resource budgets | Unit/OS integration | Oversize and queue-full fail boundedly; hard cap reports actual enforcement | M3.1 |
| M3-04 Boa worker containment | Integration | Worker panic/timeout leaves parent alive; no side-effect replay | M3.1 |
| M3-05 Session integrity | Fault injection | Interrupted SQLite transaction retains last committed state | M3.3 |
| M3-06 Structured logs | Unit/integration | Correlation IDs present; secrets/payloads absent | M3.2 |
| M3-07 Telemetry | Unit/integration | No remote connection by default; schema v1 and bounded labels | M3.2 |
| M3-08 Crash metadata | Integration | Sanitized crash ID, build identity, no secret content | M3.3 |
| M3-09 Dumps | OS integration | Opt-out emits no Ferrum-triggered full dump; opt-in permissions/retention verified | M3.3 |
| M3-10 Symbols | Release integration | Exact binary-to-PDB/build-ID association verified | M3.3 |
| M3-11 Diagnostic CLI | CLI integration | Doctor/health/diagnostics stable JSON; legacy prompt still works | M3.2 |
| M3-12 Fault injection | Integration | Worker kill, stderr flood, disk full, cancellation, timeout, restart tested | M3.4 |
| M3-13 Quality | CI | >=90% measured line coverage, security scans, cross-platform builds pass | All |

## CI matrix
Linux AMD64 and ARM64: test process groups, graceful termination, hard memory enforcement only when delegated cgroups are available; otherwise assert explicit unsupported status. Windows AMD64: test Job Object process tree, memory policy reporting, console interruption, and optional dump policy. Avoid tests requiring privileged global OS changes on shared runners.

## Negative security tests
Canary credentials embedded in environment, prompt, tool output, extension stderr and panic message must not appear in default diagnostic events or support bundles. Sensitive full dumps are excluded from this assertion because they can contain raw memory by design; test consent and containment instead.
