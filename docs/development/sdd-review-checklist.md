# Ferrum SDD review and approval checklist

Status: **proposed**; use for substantive feature/security/architecture PRs. Maintainer approval of this process is pending.

## Requirements review

- [ ] Problem, user outcome and measurable success criteria are clear.
- [ ] Scope, exclusions and constraints are explicit.
- [ ] Each requirement has a stable ID and observable acceptance condition.
- [ ] Security, failure handling, compatibility and platform constraints are covered.
- [ ] Ambiguities and decisions requiring owner approval are listed.
- [ ] Approver, date, revision and change rationale are recorded.

## Design review

- [ ] Component ownership, interfaces, state and data flow are documented.
- [ ] Each requirement maps to design behavior or an explicit deferral.
- [ ] Alternatives, trade-offs, operational risks and rollback are addressed.
- [ ] Concurrency, timeouts, resource cleanup and error propagation are explicit.
- [ ] Trust boundaries, redaction, observability and cross-platform behavior are covered.
- [ ] Approver, date, revision and accepted risks are recorded.

## Implementation readiness

- [ ] OpenSpec references the agreed requirement and design revisions.
- [ ] OpenSpec has testable MUST/SHOULD statements and negative scenarios.
- [ ] TDD plan identifies unit, integration, fault injection and platform coverage.
- [ ] No requirement is silently omitted; deferred items have owners/reasons.
- [ ] Work is grouped into one cohesive reviewable unit with clear exit criteria.

## Delivery and evidence

- [ ] Tests and implementation match the approved design or record a change request.
- [ ] Formatting, lint, tests, security and measured coverage gates pass.
- [ ] Linux AMD64, Linux ARM64 and Windows AMD64 pass where applicable.
- [ ] Checkpoint links evidence per requirement; no unchecked blocker is called complete.
- [ ] Merge/release approval recorded separately from requirements/design approval.

This checklist is a review aid, not a claim of CI enforcement.
