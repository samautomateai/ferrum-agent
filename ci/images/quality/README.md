# Prebuilt Linux quality image: rollout contract

Status: **candidate only**. No existing CI job uses this image yet.

## Goal

Replace repeated installation of Rust and Go quality tools on Linux quality runners, while preserving the mandatory `task quality` contract and native Windows CI.

## Build and supply chain

- Use a reviewed **digest-pinned** multi-platform base image with Rust **1.99.1**, rustfmt, clippy, llvm-tools-preview, Go and Python 3. Do not pass a mutable tag as `BASE_IMAGE`.
- Build `ci/images/quality/Dockerfile` independently for `linux/amd64` and `linux/arm64` with Buildx. Supply explicit tool versions for actionlint, Task, cargo-audit, cargo-deny and zizmor; defaults are deliberately absent. Resolve and review versions before publication.
- Publish to GHCR only from a trusted protected-branch workflow with `packages: write`, not from untrusted PR code. Do not embed credentials or access tokens in image layers.
- Record base digest, tool versions, image digest and SBOM; scan the published image. Pin the **published digest** in the consuming workflow.
- Ensure `task quality` runs in the checked-out repository with `.git` metadata available to Gitleaks. Confirm `actionlint`, `zizmor`, Python and Rust coverage tools work inside the container.
- Container jobs have different HOME and tool paths; use `/root/.cargo/bin` and `/root/go/bin` or adapt to the container user. Do not mount privileged Docker sockets into quality jobs.

## Rollout gates

1. Publish both architectures and verify manifests/digests, then exercise the image in a separate non-blocking smoke job.
2. Compare cold and warm Linux quality durations against run 42526102519. Record image pull time, tool installation avoided, cache hit rate and total wall time.
3. Switch the Linux quality job to the image only after the full `task quality` suite passes. Preserve the 90% line coverage threshold, LCOV artifact, Windows tests and all three platform builds.
4. Keep a documented rollback to the current GitHub-hosted runner installation path.

The Dockerfile is a reproducible build **scaffold**, not a published or validated image. No performance improvement from prebuilt images is claimed yet.
