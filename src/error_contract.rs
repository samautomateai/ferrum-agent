//! Stable, sanitized runtime error contracts.
//! Internal causes are intentionally excluded from Display.
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    WorkerTimeout,
    WorkerCrash,
    SessionIo,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WorkerTimeout => "FERRUM-WORKER-TIMEOUT",
            Self::WorkerCrash => "FERRUM-WORKER-CRASH",
            Self::SessionIo => "FERRUM-SESSION-IO",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorOutcome {
    NotStarted,
    Failed,
    Succeeded,
    Unknown,
}

#[derive(Debug)]
pub struct RuntimeFailure {
    code: ErrorCode,
    outcome: ErrorOutcome,
    public_message: &'static str,
    internal_cause: Option<String>,
}

impl RuntimeFailure {
    pub fn new(code: ErrorCode, outcome: ErrorOutcome, public_message: &'static str) -> Self {
        Self {
            code,
            outcome,
            public_message,
            internal_cause: None,
        }
    }

    pub fn with_internal_cause(mut self, cause: impl Into<String>) -> Self {
        self.internal_cause = Some(cause.into());
        self
    }

    pub const fn code(&self) -> &'static str {
        self.code.as_str()
    }

    pub const fn outcome(&self) -> ErrorOutcome {
        self.outcome
    }

    pub const fn retryable(&self) -> bool {
        false
    }
}

impl fmt::Display for RuntimeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.public_message)
    }
}

impl std::error::Error for RuntimeFailure {}

// M3.1 contract tests.
#[cfg(test)]
mod tests {
    use super::{ErrorCode, ErrorOutcome, RuntimeFailure};

    #[test]
    fn stable_public_error_code() {
        let failure = RuntimeFailure::new(
            ErrorCode::WorkerTimeout,
            ErrorOutcome::Unknown,
            "worker deadline exceeded",
        );
        assert_eq!(failure.code(), "FERRUM-WORKER-TIMEOUT");
        assert_eq!(failure.outcome(), ErrorOutcome::Unknown);
        assert!(!failure.retryable());
    }

    #[test]
    fn interrupted_side_effect_is_not_retried() {
        let failure = RuntimeFailure::new(
            ErrorCode::WorkerCrash,
            ErrorOutcome::Unknown,
            "worker exited",
        );
        assert!(!failure.retryable());
        assert_eq!(failure.outcome(), ErrorOutcome::Unknown);
    }

    #[test]
    fn public_message_does_not_include_internal_cause() {
        let failure = RuntimeFailure::new(
            ErrorCode::SessionIo,
            ErrorOutcome::Failed,
            "session storage failed",
        )
        .with_internal_cause("authorization token: secret-canary-123");
        assert!(!failure.to_string().contains("secret-canary-123"));
        assert_eq!(
            failure.to_string(),
            "FERRUM-SESSION-IO: session storage failed"
        );
    }
}
