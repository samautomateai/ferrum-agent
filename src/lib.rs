//! Reusable Ferrum worker transport and lifecycle components.
//!
//! The Tokio supervisor remains staged until the production facade is migrated.
pub mod async_ipc;
pub mod worker_lifecycle;
pub mod worker_supervisor;
