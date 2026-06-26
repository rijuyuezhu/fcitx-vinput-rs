//! Stable protocol types shared by the daemon, CLI, and Fcitx5 frontend bridge.
//!
//! This crate is intentionally small and dependency-light.  It mirrors the
//! public JSON and D-Bus ABI exposed by the original C++ implementation before
//! the Rust daemon starts replacing internals.

pub mod asr;
pub mod dbus;
pub mod recognition;
pub mod status;

pub use asr::AsrBackendState;
pub use recognition::{Candidate, CandidateSource, RecognitionPayload, RecognitionProtocolError};
pub use status::ServiceStatus;
