//! Error types for text finishing, adapter supervision, prompts, and context cache.

use thiserror::Error;

/// Errors from text finishing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextError {
    /// A non-raw scene with candidates needs adapter support that is not ported yet.
    #[error("scene `{0}` requires a text adapter backend")]
    AdapterRequired(String),
    /// A configured adapter path exists but is not implemented yet.
    #[error("scene `{0}` requested a text adapter that is not implemented yet")]
    UnsupportedAdapter(String),
    /// Adapter selection was ambiguous for a scene.
    #[error("scene `{0}` has ambiguous text adapter selection")]
    AmbiguousAdapter(String),
    /// Scene references an unknown OpenAI-compatible provider.
    #[error("scene `{scene_id}` references unknown OpenAI-compatible provider `{provider_id}`")]
    UnknownProvider {
        /// Scene id.
        scene_id: String,
        /// Missing provider id.
        provider_id: String,
    },
    /// OpenAI-compatible provider selection was ambiguous for a scene.
    #[error("scene `{0}` has ambiguous OpenAI-compatible provider selection")]
    AmbiguousProvider(String),
    /// Command adapter id is unsafe for runtime paths.
    #[error("invalid text adapter id for runtime path: {0}")]
    InvalidAdapterId(String),
    /// Adapter runtime filesystem operation failed.
    #[error("text adapter runtime I/O failed: {0}")]
    AdapterRuntimeIo(String),
    /// Adapter runtime pid file was malformed.
    #[error("text adapter runtime pid file is invalid: {0}")]
    InvalidAdapterPid(String),
    /// Command adapter helper returned an error or invalid response.
    #[error("text adapter failed: {0}")]
    AdapterFailed(String),
    /// Legacy prompt file resolution failed.
    #[error("prompt file load failed: {0}")]
    PromptFileLoad(String),
    /// Recent-input context cache read failed.
    #[error("context cache read failed: {0}")]
    ContextCacheRead(String),
    /// Recent-input context cache write failed.
    #[error("context cache write failed: {0}")]
    ContextCacheWrite(String),
}
