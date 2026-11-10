# Ferrum lightweight requirements and design governance

Status: **proposed for maintainer approval** (2026-11-09). Inspired by BMAD's separation of requirements and design; **BMAD itself is not a dependency**.

## Authority and artifacts

1. **Requirements** (WHY / WHAT): `docs/requirements/<milestone>.md`. Define user outcomes, scope and exclusions, functional and nonfunctional requirements with stable IDs, risks, measurable acceptance criteria, and open questions. No implementation details masquerading as requirements.
2. **Design** (HOW): `docs/design/<milestone>.md`. Describe components, ownership, lifecycle, interfaces and data flow, alternatives and trade-offs, failure modes, security boundaries, compatibility, observability, test strategy and rollout/rollback. Link every decision to requirement IDs.
3. **OpenSpec** (implementation contract): `openspec/specs/<milestone>.md`. Translate agreed requirements/design into normative MUST/SHOULD behavior, testable scenarios, and implementation slices. It must not silently contradict the two upstream artifacts.
4. **ADR** (only material decisions): document irreversible, cross-cutting or contested choices; no ADR for routine edits.
5. **Evidence**: tests, CI, coverage, fault injection and `docs/ferrum-readiness-checkpoint.md` track actual acceptance. Green CI alone is not sign-off.

## Approval and implementation gate

- **Draft**: AI or contributor may research and propose requirements and alternatives. Open questions are explicit.
- **Agreed requirements**: maintainer explicitly approves the requirement baseline; record approver/date and revision. No implied approval from silence.
- **Approved design**: maintainer approves selected approach and accepted risks; record approver/date and revision.
- **Implementation ready**: OpenSpec references the approved baselines; acceptance scenarios and test plan are present.
- **Build**: write failing tests first where practical, implement cohesive vertical slices, run format/lint/tests/security/coverage, and update the checkpoint in the same PR.
- **Done**: acceptance evidence linked, residual risks and deferrals recorded, CI green across supported platforms, maintainer approves merge.

If a requirement changes, reopen the affected approval and update design, OpenSpec and tests together. For emergency bug fixes, a short requirement/design delta in the PR is sufficient, but never silently bypass security or tests.

## Right-sized process

- Small bug: one paragraph for requirement + design rationale in OpenSpec/PR; existing tests.
- Medium change: requirement and design sections in the same milestone document, with IDs.
- Architecture/security/cross-platform milestone: separate requirements, design, OpenSpec and ADR where justified.
- No duplicated epics, ceremonies, agent roles, generated documents or mandatory BMAD tooling.
- **Enforcement:** initially PR review and checklist; CI validation of cross-file references can follow once the format stabilizes. This document does not claim automatic enforcement exists.

## Minimum traceability

| Requirement | Design section | OpenSpec clause | Test(s) | Evidence | State |
| --- | --- | --- | --- | --- | --- |
| M31-R5 | M3.1 design: lifecycle cleanup | R5 | `tests/tokio_supervisor.rs` | Pending full OS/runtime teardown verification | Open |

Never mark a requirement accepted solely because a code path exists.

## Operational aids

- [SDD review checklist](sdd-review-checklist.md)
- [Traceability contract](sdd-traceability.md)
- [Right-sized milestone template](sdd-milestone-template.md)
- [M3.1 acceptance matrix](../requirements/m31-acceptance-matrix.md)

These aids are proposed; no automatic approval or CI enforcement is implied.

## CI validation

The `task source-limits` and `task quality` workflows execute `scripts/check_sdd_contract.py`, which fails for missing M3.1 artifacts, duplicate requirement IDs, missing design mappings, missing acceptance rows and undefined IDs. It does **not** infer or grant human approval; approval remains a separate gate. This first validator covers M3.1, not every future milestone.
