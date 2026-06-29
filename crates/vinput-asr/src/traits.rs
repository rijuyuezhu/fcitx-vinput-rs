//! ASR backend traits, descriptors, recognition context, and event types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinput_audio::PcmBuffer;

use crate::AsrError;

/// How audio should be delivered to an ASR session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioDeliveryMode {
    /// The backend expects all PCM samples after recording stops.
    Buffered,
    /// The backend accepts incremental PCM chunks while recording.
    Chunked,
}

/// Static backend capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BackendCapabilities {
    /// Whether this backend can emit partial recognition text.
    pub partial_results: bool,
    /// Preferred audio delivery mode.
    pub delivery_mode: AudioDeliveryMode,
}

impl BackendCapabilities {
    /// Capabilities for a simple buffered backend.
    #[must_use]
    pub const fn buffered() -> Self {
        Self {
            partial_results: false,
            delivery_mode: AudioDeliveryMode::Buffered,
        }
    }

    /// Capabilities for a streaming backend.
    #[must_use]
    pub const fn streaming() -> Self {
        Self {
            partial_results: true,
            delivery_mode: AudioDeliveryMode::Chunked,
        }
    }
}

/// Backend identity and capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BackendDescriptor {
    /// Stable provider id.
    pub provider_id: String,
    /// Stable model id.
    pub model_id: String,
    /// Human-readable backend label.
    pub label: String,
    /// Backend capability flags.
    pub capabilities: BackendCapabilities,
}

impl BackendDescriptor {
    /// Creates a descriptor with owned strings.
    #[must_use]
    pub fn new(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        label: impl Into<String>,
        capabilities: BackendCapabilities,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            label: label.into(),
            capabilities,
        }
    }
}

/// Event emitted by a recognition session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecognitionEvent {
    /// Streaming partial text.
    PartialText {
        /// Partial recognized text.
        text: String,
    },
    /// Final recognized text.
    FinalText {
        /// Final recognized text.
        text: String,
    },
    /// Backend error surfaced during recognition.
    Error {
        /// Human-readable error message.
        message: String,
    },
    /// Session has no more events.
    Completed,
}

/// Recognition context passed to concrete ASR backends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecognitionContext {
    /// Optional BCP-47-like language tag from config or scene policy.
    #[serde(default)]
    pub language: Option<String>,
    /// Scene id selected for this recognition session.
    pub scene_id: String,
    /// Whether this session is command mode.
    pub command_mode: bool,
    /// Optional selected text provided by the frontend for command mode.
    #[serde(default)]
    pub selected_text: Option<String>,
}

impl RecognitionContext {
    /// Creates a normal recognition context.
    #[must_use]
    pub fn normal(scene_id: impl Into<String>, language: Option<String>) -> Self {
        Self {
            language,
            scene_id: scene_id.into(),
            command_mode: false,
            selected_text: None,
        }
    }

    /// Creates a command-mode recognition context.
    #[must_use]
    pub fn command(
        scene_id: impl Into<String>,
        language: Option<String>,
        selected_text: impl Into<String>,
    ) -> Self {
        Self {
            language,
            scene_id: scene_id.into(),
            command_mode: true,
            selected_text: Some(selected_text.into()),
        }
    }
}

/// Mutable recognition session.
pub trait RecognitionSession: Send {
    /// Push signed 16-bit PCM samples with explicit layout metadata.
    fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        self.push_audio(pcm.samples())
    }

    /// Push raw signed 16-bit PCM samples using backend/default metadata.
    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError>;

    /// Finish audio delivery and let the backend enqueue final events.
    fn finish(&mut self) -> Result<(), AsrError>;

    /// Cancel work and enqueue no further result.
    fn cancel(&mut self) -> Result<(), AsrError>;

    /// Drain currently pending events.
    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError>;
}

/// ASR backend factory.
pub trait AsrBackend: Send + Sync {
    /// Returns backend identity and capabilities.
    fn describe(&self) -> BackendDescriptor;

    /// Creates a fresh recognition session for the given context.
    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError>;
}
