//! Pure worker lifecycle model. Tokio transport will use this state machine in the next slice.
//! State transitions are explicit to prevent accepting requests during shutdown.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Starting,
    Ready,
    Busy,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEvent {
    Started,
    RequestAccepted,
    RequestCompleted,
    StopRequested,
    Stopped,
    Fault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTransition {
    pub state: WorkerState,
    pub event: WorkerEvent,
}

impl fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid worker lifecycle transition")
    }
}

impl std::error::Error for InvalidTransition {}

#[derive(Debug)]
pub struct WorkerLifecycle {
    state: WorkerState,
}

impl Default for WorkerLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerLifecycle {
    pub fn new() -> Self {
        Self {
            state: WorkerState::Starting,
        }
    }

    pub fn state(&self) -> WorkerState {
        self.state
    }

    pub fn transition(&mut self, event: WorkerEvent) -> Result<WorkerState, InvalidTransition> {
        use WorkerEvent as E;
        use WorkerState as S;
        let next = match (self.state, event) {
            (S::Starting, E::Started) => S::Ready,
            (S::Ready, E::RequestAccepted) => S::Busy,
            (S::Busy, E::RequestCompleted) => S::Ready,
            (S::Starting | S::Ready | S::Busy, E::StopRequested) => S::Stopping,
            (S::Stopping, E::Stopped) => S::Stopped,
            (S::Starting | S::Ready | S::Busy | S::Stopping, E::Fault) => S::Failed,
            _ => {
                return Err(InvalidTransition {
                    state: self.state,
                    event,
                });
            }
        };
        self.state = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_invoke_shutdown_happy_path() {
        let mut lifecycle = WorkerLifecycle::new();
        assert_eq!(lifecycle.state(), WorkerState::Starting);
        assert_eq!(lifecycle.transition(WorkerEvent::Started), Ok(WorkerState::Ready));
        assert_eq!(lifecycle.transition(WorkerEvent::RequestAccepted), Ok(WorkerState::Busy));
        assert_eq!(lifecycle.transition(WorkerEvent::RequestCompleted), Ok(WorkerState::Ready));
        assert_eq!(lifecycle.transition(WorkerEvent::StopRequested), Ok(WorkerState::Stopping));
        assert_eq!(lifecycle.transition(WorkerEvent::Stopped), Ok(WorkerState::Stopped));
    }

    #[test]
    fn rejects_requests_after_shutdown_and_after_failure() {
        for ending in [WorkerEvent::StopRequested, WorkerEvent::Fault] {
            let mut lifecycle = WorkerLifecycle::new();
            lifecycle.transition(WorkerEvent::Started).unwrap();
            lifecycle.transition(ending).unwrap();
            let previous = lifecycle.state();
            assert!(lifecycle.transition(WorkerEvent::RequestAccepted).is_err());
            assert_eq!(lifecycle.state(), previous);
        }
    }

    #[test]
    fn rejects_concurrent_requests_on_same_worker() {
        let mut lifecycle = WorkerLifecycle::new();
        lifecycle.transition(WorkerEvent::Started).unwrap();
        lifecycle.transition(WorkerEvent::RequestAccepted).unwrap();
        assert_eq!(
            lifecycle.transition(WorkerEvent::RequestAccepted),
            Err(InvalidTransition {
                state: WorkerState::Busy,
                event: WorkerEvent::RequestAccepted,
            })
        );
    }

    #[test]
    fn failure_is_terminal_and_does_not_restart_implicitly() {
        let mut lifecycle = WorkerLifecycle::new();
        lifecycle.transition(WorkerEvent::Fault).unwrap();
        assert_eq!(lifecycle.state(), WorkerState::Failed);
        assert!(lifecycle.transition(WorkerEvent::Started).is_err());
        assert!(lifecycle.transition(WorkerEvent::StopRequested).is_err());
    }

    #[test]
    fn shutdown_while_busy_is_explicit_and_terminal() {
        let mut lifecycle = WorkerLifecycle::new();
        lifecycle.transition(WorkerEvent::Started).unwrap();
        lifecycle.transition(WorkerEvent::RequestAccepted).unwrap();
        lifecycle.transition(WorkerEvent::StopRequested).unwrap();
        assert!(lifecycle.transition(WorkerEvent::RequestCompleted).is_err());
        assert_eq!(lifecycle.transition(WorkerEvent::Stopped), Ok(WorkerState::Stopped));
    }
}
