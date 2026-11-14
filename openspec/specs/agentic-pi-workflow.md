# OpenSpec — Pi-based agentic development contract

Status: proposed for maintainer acceptance.

## Requirements
- PI-R1: Every production change has an approved OpenSpec and acceptance trace.
- PI-R2: Implementation follows TDD with observable red and green results.
- PI-R3: Architect and scrutinizer review designs before implementation.
- PI-R4: Reviewer and security roles use separate sessions and report evidence.
- PI-R5: Quality agent validates same-commit CI and measured >=90% coverage.
- PI-R6: Human approval and GitHub required checks gate merges.
- PI-R7: No committed provider credentials or unvetted automatic extensions.
- PI-R8: Workflow works from repository root on supported developer OSes.

## Acceptance
- A new contributor can follow docs/agentic/README.md with installed Pi.
- Each role prompt enforces read-only review versus implementation boundaries.
- CI still enforces existing Rust gates independently of agent prompts.
- Document GitHub branch-rule configuration and any checks not yet enforced.
- No claim of automatic multi-agent execution without tested integration.

## Follow-up
After this policy is reviewed, evaluate a maintained Pi delegation extension in a separate change with pinned versions, threat review, deterministic role outputs and integration tests.
