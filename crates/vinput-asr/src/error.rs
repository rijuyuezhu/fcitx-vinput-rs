//! ASR error types.

use thiserror::Error;

/// Recognition errors.
#[derive(Debug, Error)]
pub enum AsrError {
    /// Audio was pushed after the session finished.
    #[error("recognition session is already finished")]
    AlreadyFinished,
    /// Session was cancelled.
    #[error("recognition session was cancelled")]
    Cancelled,
    /// The requested ASR provider is not present in config.
    #[error("ASR provider `{0}` is not configured")]
    UnknownProvider(String),
    /// Configured provider kind is recognized but not implemented yet.
    #[error("ASR provider `{provider_id}` of kind `{kind}` is not implemented yet")]
    UnsupportedProviderKind {
        /// Provider id.
        provider_id: String,
        /// Provider kind label.
        kind: String,
    },
    /// Backend-specific error.
    #[error("backend error: {0}")]
    Backend(String),
}
