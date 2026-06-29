//! Runtime error types.

use thiserror::Error;
use vinput_asr::AsrError;
use vinput_audio::AudioError;
use vinput_protocol::ServiceStatus;

/// Runtime errors.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Config failed validation.
    #[error("invalid config: {0}")]
    InvalidConfig(#[source] vinput_config::ConfigError),
    /// Runtime cannot start a new session while busy.
    #[error("runtime is busy: {0}")]
    Busy(ServiceStatus),
    /// Stop was requested while not recording.
    #[error("runtime is not recording: {0}")]
    NotRecording(ServiceStatus),
    /// Recording reached stop without an active ASR session.
    #[error("runtime is missing an active ASR session")]
    MissingAsrSession,
    /// ASR backend/session failed.
    #[error("asr error: {0}")]
    Asr(#[source] AsrError),
    /// Audio source failed.
    #[error("audio error: {0}")]
    Audio(#[source] AudioError),
    /// Result finishing failed.
    #[error("result finishing error: {0}")]
    Finish(#[source] vinput_text::TextError),
    /// Requested text adapter is not configured.
    #[error("text adapter `{0}` is not configured")]
    TextAdapterNotConfigured(String),
    /// Requested text adapter is already managed by this runtime.
    #[error("text adapter `{0}` is already running")]
    TextAdapterAlreadyRunning(String),
    /// Text adapter process supervision failed.
    #[error("text adapter supervisor error: {0}")]
    TextAdapterSupervisor(#[source] vinput_text::TextError),
}
