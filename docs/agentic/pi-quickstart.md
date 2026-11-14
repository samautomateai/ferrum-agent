# Pi project-local prompts

Pi's project prompt discovery uses `.pi/prompts/*.md`. Run Pi from the Ferrum repository root and use these commands:

- `/architect` — design, ADR, acceptance criteria
- `/scrutinize` — independent architecture challenge
- `/implement` — approved OpenSpec and TDD implementation
- `/review` — code review in a **fresh Pi session**
- `/security-review` — security review in a **fresh Pi session**
- `/quality-review` — evidence-based quality audit

Install Pi with `npm install -g @mariozechner/pi-coding-agent`, then verify `pi --version`. Windows requires a bash shell such as Git Bash. Never commit API keys. Pi prompts are role entrypoints, **not** isolated agents or a security boundary. Start fresh sessions for independent reviews.

A future separately reviewed extension can provide controlled delegation. No third-party extensions are installed by this repository.
