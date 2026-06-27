//! Daemon library pieces shared by the binary and integration tests.

pub mod dbus_service;
pub mod runtime;

pub use dbus_service::VinputDbusService;
pub use runtime::{RuntimeError, RuntimeState, StopRecordingReport};
