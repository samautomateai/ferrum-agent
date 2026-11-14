# Pi-based development workflow

This repository adopts Pi as an interactive coding-agent runtime, not as a CI trust boundary. `AGENTS.md` is the shared project contract; role files in `docs/agentic/roles/` are explicit prompts for separate Pi sessions. This initial setup does **not** claim automatic subagent orchestration or a working Pi extension API.

## Bootstrap
Install the maintained Pi coding-agent CLI using its official installation instructions. Verify `pi --version`. Configure your chosen provider through Pi's supported authentication/model settings; do not commit tokens or provider credentials. Run `task doctor` to check Rust and security tooling.

## Execution
1. Start a fresh Pi session from repository root; have it read `AGENTS.md` and `docs/agentic/roles/architect.md`.
2. In a separate session, use `scrutinizer.md` to challenge the spec, platform constraints and threats. Record decisions in an ADR.
3. Only after maintainer approval, run `implementer.md` in a feature branch. Require red-green-refactor evidence.
4. In distinct fresh sessions, run `reviewer.md` and `security.md`. Reviewers produce findings and do not modify implementation.
5. Run `task quality` locally and require GitHub CI. `quality.md` audits evidence and must never invent results.

Do not paste secrets into agent sessions. Do not grant automatic push, merge, deployment, or unrestricted network permissions. Pin and review any third-party Pi extension before installation. Automated delegation is deferred until a vetted extension and a deterministic review contract exist.

## Required evidence per PR
Spec/ADR references; acceptance matrix; red test and green test evidence; `task quality` output; actual line coverage; platform CI links; independent review findings and disposition; risks and rollback.

## Scope
This workflow PR introduces repo policy and reusable role prompts only. It does not replace GitHub branch rules or prove agent isolation. Administrators must configure required status checks and PR reviews in repository settings.
