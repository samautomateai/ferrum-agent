# ADR 0004: Tokio for worker supervision

Status: Accepted for staged implementation, 2026-11-08.

## Context
Ferrum currently supervises persistent Boa worker subprocesses using dedicated std threads, sync channels, and request timeouts. It provides basic failure classification but lacks explicit ownership of all tasks, cancellation propagation, and lifecycle transitions.

## Decision
Adopt Tokio for the internal worker supervisor using tokio::process, bounded tokio::sync::mpsc, tokio::time, and tokio-util CancellationToken. Keep Boa execution in a separate subprocess and preserve the synchronous ExtensionWorker facade. Use one process-wide runtime owner and an explicit supervisor state machine. Introduce the implementation in tested slices and remove the old transport only after parity.

## Constraints
A request may have side effects even if its response is missing; never replay it. No nested runtime block_on; reject reentrant sync calls or use a documented safe bridge. Every child and I/O task must have an owner, cancellation and reaping policy. Tokio does not enforce process memory or CPU budgets; those remain M3.2 OS-specific controls. Do not claim process-tree cleanup without Job Objects / process groups.

## Alternatives
Retain thread-per-pipe: simple for a few workers, but cancellation and task ownership become increasingly fragile. Async-std/smol: smaller ecosystem for the needed process/cancellation integration. Rewriting Boa as async: unnecessary and high risk.

## Consequences
Additional runtime dependencies and a compatibility bridge, offset by clearer lifecycle semantics, cancellation and bounded concurrency. Cross-platform behavior and latency/resource overhead must be measured in CI before retiring legacy transport.
