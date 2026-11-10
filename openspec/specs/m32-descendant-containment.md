# OpenSpec M3.2 — Worker descendant containment

Status: proposed. Requirements: [M3.2 requirements](../../docs/requirements/m32-descendant-containment.md). Design: [M3.2 design](../../docs/design/m32-descendant-containment.md).

## Specification
The worker supervisor MUST establish OS-specific descendant containment before any worker-controlled code executes, MUST fail closed on unsupported containment, MUST terminate contained descendants on every worker shutdown path, and MUST preserve M3.1 protocol and error semantics.

## Red-green-refactor
First write failing real-process tests for child/grandchild cleanup, forced termination, crash, shared runtime teardown, startup assignment failure and concurrent worker isolation. Implement one OS slice at a time. Do not relax quality, coverage or source limits.

## Exit criteria
All M32-R1..R8 and M32-A1..A6 trace to tests and CI evidence. Record the measured coverage percentage, known containment limitations, approvals and rollback before merge.
