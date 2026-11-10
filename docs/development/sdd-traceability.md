# SDD traceability contract

Status: **proposed**. One requirement ID must mean one stable behavior; IDs must not be recycled.

## Required relationships

`Requirement ID -> design section -> OpenSpec clause -> test(s) -> CI/evidence -> decision`.

Use a compact table in each milestone document or link to one authoritative matrix. Evidence can be a test name and a CI run; do not invent percentages, run results or sign-off.

| Field | Meaning |
| --- | --- |
| ID | Stable `M<major><minor>-R<number>` or `M<major><minor>-N<number>` |
| Acceptance | Observable pass/fail behavior, including failure paths |
| Design | Section describing ownership, implementation strategy and trade-offs |
| OpenSpec | Normative clause and scenarios |
| Tests | Concrete test names, fixtures and target OS |
| Evidence | CI run, coverage result or manual reproducible verification |
| State | Proposed, agreed, implemented, verified, accepted or deferred |

## Change control

- Requirements revision changes need explicit owner approval before design is considered current.
- Design changes must identify impacted requirement IDs and update OpenSpec and tests.
- An implementation deviation must be recorded as a design change, not silently rationalized after the fact.
- Acceptance is per requirement and per supported platform; a green workflow alone is not sufficient.
- A deferred requirement must identify owner, rationale, risk and target milestone.
- A milestone is complete only when all mandatory requirements are accepted and residual risks are approved.

## Practical scope

For small bug fixes, keep this mapping in the PR description. For milestones, maintain a dedicated traceability matrix. Avoid generating duplicate copies of the same requirements.
