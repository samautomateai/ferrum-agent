# Ferrum naming migration — M2.0 preparation

- Product and CLI: Ferrum / `ferrum`.
- Repository: `ferrum-agent`.
- Cargo package: `ferrum` (previously `pi-rs`).
- Integration-test binary reference: `CARGO_BIN_EXE_ferrum`.
- Session examples: `.ferrum-session.json`, `.ferrum.sqlite`.
- Historical OpenSpec milestones retain original language for traceability.
- The JavaScript `pi` extension API remains intentionally unchanged for compatibility.
- `ferrum-core`, `ferrum-runtime`, `ferrum-tools` and `ferrum-protocol` are **future proposed crates**, not existing packages.

## Upload to GitHub

Create an empty repository named `ferrum-agent` (without a README), extract this ZIP and run inside `ferrum-agent`:

```bash
git init -b main
git add .
git commit -m "chore: migrate Pi-RS source to Ferrum"
git remote add origin git@github.com:YOUR_ACCOUNT/ferrum-agent.git
git push -u origin main
```

## Release blockers

Cargo.lock is absent. On a Rust-enabled machine run `cargo generate-lockfile`, then `task quality` and resolve all failures. 90% line coverage and cross-platform validation are mandatory before release.
