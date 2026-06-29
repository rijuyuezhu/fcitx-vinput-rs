//! ASR backend contract, deterministic mock, and backend skeletons.
//!
//! This crate mirrors the original C++ daemon's recognition contract at a Rust
//! trait boundary. Real backends such as sherpa-onnx and command execution
//! should implement these traits after their contracts are covered by tests.

mod command;
mod error;
mod factory;
mod mock;
mod payload;
mod traits;

pub use command::{
    CommandAsrBackend, CommandAsrRequest, CommandAsrResponse, CommandAsrRunner, CommandAsrSpec,
    LegacyCommandBatchRunner, LegacyCommandStreamingRunner, ProcessCommandAsrRunner,
    UnsupportedCommandAsrRunner, legacy_command_streaming_audio_line,
    legacy_command_streaming_finish_line, parse_legacy_command_streaming_line,
};
pub use error::AsrError;
pub use factory::AsrBackendFactory;
pub use mock::MockAsrBackend;
pub use payload::events_to_payload;
pub use traits::{
    AsrBackend, AudioDeliveryMode, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};

#[cfg(test)]
mod tests;
