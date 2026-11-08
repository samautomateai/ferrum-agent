//! M3.1 red-phase contract tests. Implementation follows in a separate commit.
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
        ).with_internal_cause("authorization token: secret-canary-123");
        assert!(!failure.to_string().contains("secret-canary-123"));
        assert_eq!(failure.to_string(), "FERRUM-SESSION-IO: session storage failed");
    }
}
