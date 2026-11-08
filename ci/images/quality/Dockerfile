# Ferrum Linux quality runner image (candidate, not yet enabled)
# Supply a reviewed digest-pinned Linux/Go/Rust base image when building.
# The publisher must pass --build-arg BASE_IMAGE=<image>@sha256:<digest>.
ARG BASE_IMAGE
FROM ${BASE_IMAGE}

ARG LLVM_COV_VERSION=0.9.1
ARG SCCACHE_VERSION=0.12.0
ARG GITLEAKS_VERSION=v8.30.1
ARG ACTIONLINT_VERSION
ARG TASK_VERSION
ARG CARGO_AUDIT_VERSION
ARG CARGO_DENY_VERSION
ARG ZIZMOR_VERSION

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]

RUN test -n "${ACTIONLINT_VERSION}" && test -n "${TASK_VERSION}" \
 && test -n "${CARGO_AUDIT_VERSION}" && test -n "${CARGO_DENY_VERSION}" \
 && test -n "${ZIZMOR_VERSION}" \
 && cargo install --locked cargo-llvm-cov --version "${LLVM_COV_VERSION}" \
 && cargo install --locked cargo-audit --version "${CARGO_AUDIT_VERSION}" \
 && cargo install --locked cargo-deny --version "${CARGO_DENY_VERSION}" \
 && cargo install --locked zizmor --version "${ZIZMOR_VERSION}" \
 && cargo install --locked sccache --version "${SCCACHE_VERSION}" \
 && go install "github.com/rhysd/actionlint/cmd/actionlint@${ACTIONLINT_VERSION}" \
 && go install "github.com/go-task/task/v3/cmd/task@${TASK_VERSION}" \
 && go install "github.com/zricethezav/gitleaks/v8@${GITLEAKS_VERSION}"

ENV PATH="/root/go/bin:/root/.cargo/bin:${PATH}"
RUN rustc --version && cargo --version && cargo llvm-cov --version \
 && cargo audit --version && cargo deny --version && zizmor --version \
 && sccache --version && task --version && actionlint -version && gitleaks version
