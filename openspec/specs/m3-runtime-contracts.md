# M3.0 — Runtime contracts and resource budgets

Status: PROPOSED. Specification only. Numeric values are initial defaults for review, not implemented guarantees.

## Scope
These contracts apply to parent agent, trusted Boa extension workers, LLM calls, sessions, and diagnostics. Units are binary MiB and wall-clock seconds unless noted.

## Default budgets
| Resource | Default | Hard/soft | Failure behavior |
|---|---:|---|---|
| Extension invocation end-to-end deadline | 30 s | Hard wall-clock only when supervisor owns nonblocking IPC and process-tree kill | Cancel and terminate worker; report unknown outcome if effects may have happened |
| Worker graceful shutdown | 5 s | Hard wall-clock with OS process termination fallback | Kill owned process tree after grace |
| IPC request/response frame | 2 MiB | Hard application bound | Reject frame, terminate offending worker |
| JS argument/result payload | 1 MiB each | Hard application bound | Reject before send/after decode |
| Worker stderr diagnostic ring | 256 KiB | Hard in-memory bound | Drop oldest bytes; record truncation counter |
| Worker resident memory | 512 MiB | Hard only where OS primitive verified; otherwise **unenforced** | Terminate on OS-enforced breach; never claim memory cap without enforcement |
| Pending work per worker | 16 items | Hard application bound | Return queue-full without waiting indefinitely |
| Concurrent extension invocations | 1 per persistent worker | Hard application bound | Queue within bound or reject |
| Local diagnostic storage | 100 MiB total | Best effort quota with bounded retention | Stop recording; do not block core agent |
| Log record size | 16 KiB | Hard application bound | Truncate safely and mark truncated |
| Support bundle | 25 MiB | Hard archive size limit | Fail with explicit size error |
| Automatic retries | 0 for tools; at most 2 for explicitly safe network operations | Hard application bound | Surface final error; never replay side effects |

Existing M1 limits remain authoritative until a separate implementation change modifies them. No configuration may silently raise a hard cap. Override ranges, precedence (CLI > config > default), and validation errors must be specified before code.

## Typed error contract
A structured error has `code`, `domain`, `message` (sanitized), `retryable`, `outcome` (`not_started|failed|succeeded|unknown`), `correlation_id`, and optional internal cause chain. Stable public code examples: `FERRUM-WORKER-TIMEOUT`, `FERRUM-WORKER-CRASH`, `FERRUM-IPC-OVERSIZE`, `FERRUM-SESSION-IO`, `FERRUM-LLM-RATE_LIMIT`, `FERRUM-DIAGNOSTICS-QUOTA`. Internal chains must never be automatically serialized to public logs.

## Supervisor lifecycle state machine
`Created -> Starting -> Running -> Draining -> Stopped`; `Starting|Running|Draining -> Failed -> Stopped`.
- Each worker has a single supervisor owner and one in-flight request.
- New work rejected once draining starts.
- Request deadline covers queueing, write, flush, response, and cleanup, not only response wait.
- Termination must target the owned process tree, with bounded cleanup. Windows Job Objects kill-on-close; Linux process groups for tree signaling, optionally cgroup v2 for enforced resource limits.
- A crashed worker may be reinitialized for future calls, but its interrupted invocation is never automatically replayed.
- All failure paths are idempotent on shutdown and report whether the tool outcome is unknown.

## Platform capabilities and truth-in-advertising
| Capability | Linux AMD64/ARM64 | Windows AMD64 |
|---|---|---|
| Parent-owned worker process tree | Process groups; cgroups if delegated | Job Objects |
| Hard RSS/working-set cap | cgroups v2 memory.max when delegated; otherwise unavailable | Job Object process/job memory limits subject to OS behavior |
| Crash dump capture | OS core dump / external collector; may need ulimit/core_pattern setup | WER LocalDumps or supported external minidump collector |
| Crash symbol mapping | Build ID and split debug symbols | PDB matched to exact binary |
| Ctrl+C | SIGINT/SIGTERM handling | Console control handling |
| Full dump confidentiality | Sensitive, disabled by Ferrum default; OS settings independent | Sensitive, disabled by Ferrum default; OS settings independent |

A runtime `doctor` must report whether hard limits and dump mechanisms are available. Unsupported controls cannot silently degrade while reporting enforcement success.

## Diagnostics event schema v1
Required: `schema_version=1`, `timestamp_utc`, `severity`, `event_name`, `correlation_id`, `component`, `duration_ms?`, `error_code?`, `truncated?`. Permitted metadata: tool **name**, provider **identifier** (not URL or credential), counters, and aggregate byte/token usage. Prohibited by default: prompt content, raw tool arguments/results, environment, API keys, authorization headers, session transcript, arbitrary exception debug output, or raw child stderr.

Diagnostic export is local-only by default. User consent is required for support bundle generation and separate consent for including full memory dumps. No network export by default.

## Open questions before acceptance
1. Should 512 MiB be a configurable hard worker limit or a recommended target when enforcement is unavailable?
2. How should worker diagnostics behave if disk is full or permission changes mid-run?
3. Should CLI `--diagnostics` use a subcommand or flag for backward compatibility?
4. Are 30-second worker deadlines sufficient for legitimate long-running extension operations?
