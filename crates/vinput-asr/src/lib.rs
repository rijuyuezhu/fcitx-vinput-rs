//! ASR backend contract and deterministic mock implementation.
//!
//! This crate mirrors the original C++ daemon's recognition contract at a Rust
//! trait boundary.  Real backends such as sherpa-onnx and command streaming
//! should implement these traits after the mock contract is covered by tests.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};
use vinput_protocol::{CandidateSource, RecognitionPayload};

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
    /// Push signed 16-bit mono PCM samples.
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

/// Deterministic ASR backend for tests and early daemon wiring.
#[derive(Debug, Clone)]
pub struct MockAsrBackend {
    descriptor: BackendDescriptor,
    final_text: String,
    partial_text: Option<String>,
}

impl MockAsrBackend {
    /// Creates a buffered mock backend with fixed final text.
    #[must_use]
    pub fn buffered(final_text: impl Into<String>) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                "mock",
                "mock-buffered",
                "Mock buffered ASR",
                BackendCapabilities::buffered(),
            ),
            final_text: final_text.into(),
            partial_text: None,
        }
    }

    /// Creates a streaming mock backend with fixed partial and final text.
    #[must_use]
    pub fn streaming(partial_text: impl Into<String>, final_text: impl Into<String>) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                "mock",
                "mock-streaming",
                "Mock streaming ASR",
                BackendCapabilities::streaming(),
            ),
            final_text: final_text.into(),
            partial_text: Some(partial_text.into()),
        }
    }
}

/// Builds ASR backends from typed config entries.
#[derive(Debug, Clone, Copy, Default)]
pub struct AsrBackendFactory;

impl AsrBackendFactory {
    /// Creates a factory.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the active backend from ASR config.
    pub fn build_active(config: &AsrConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        let provider = config
            .providers
            .iter()
            .find(|provider| provider.id == config.active_provider)
            .ok_or_else(|| AsrError::UnknownProvider(config.active_provider.clone()))?;
        Self::build_provider(provider)
    }

    /// Builds a backend from one provider entry.
    pub fn build_provider(provider: &AsrProviderConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        if provider.id == "mock" {
            return Ok(Box::new(MockAsrBackend::streaming(
                "mock partial",
                "mock recognition result",
            )));
        }
        unsupported_provider(&provider.id, &provider.kind)
    }
}

fn unsupported_provider(
    provider_id: &str,
    kind: &AsrProviderKind,
) -> Result<Box<dyn AsrBackend>, AsrError> {
    Err(AsrError::UnsupportedProviderKind {
        provider_id: provider_id.to_owned(),
        kind: provider_kind_label(kind).to_owned(),
    })
}

fn provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}
impl AsrBackend for MockAsrBackend {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(MockRecognitionSession {
            final_text: self.final_text.clone(),
            partial_text: self.partial_text.clone(),
            accepted_samples: 0,
            finished: false,
            cancelled: false,
            partial_sent: false,
            events: Vec::new(),
        }))
    }
}

/// Converts recognition events into a legacy result payload.
pub fn events_to_payload(events: &[RecognitionEvent]) -> Result<RecognitionPayload, AsrError> {
    let final_text = events.iter().find_map(|event| match event {
        RecognitionEvent::FinalText { text } => Some(text.as_str()),
        RecognitionEvent::Error { message } => Some(message.as_str()),
        RecognitionEvent::PartialText { .. } | RecognitionEvent::Completed => None,
    });

    match final_text {
        Some(text) => Ok(RecognitionPayload {
            commit_text: text.to_owned(),
            candidates: vec![vinput_protocol::Candidate::new(text, CandidateSource::Raw)],
        }),
        None => Err(AsrError::Backend(
            "recognition completed without final text".to_owned(),
        )),
    }
}

#[derive(Debug)]
struct MockRecognitionSession {
    final_text: String,
    partial_text: Option<String>,
    accepted_samples: usize,
    finished: bool,
    cancelled: bool,
    partial_sent: bool,
    events: Vec<RecognitionEvent>,
}

impl RecognitionSession for MockRecognitionSession {
    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.accepted_samples += samples.len();
        if !self.partial_sent
            && let Some(text) = &self.partial_text
        {
            self.events
                .push(RecognitionEvent::PartialText { text: text.clone() });
            self.partial_sent = true;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.finished = true;
        self.events.push(RecognitionEvent::FinalText {
            text: self.final_text.clone(),
        });
        self.events.push(RecognitionEvent::Completed);
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.cancelled = true;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsrBackend, AsrBackendFactory, AsrError, AudioDeliveryMode, MockAsrBackend,
        RecognitionContext, RecognitionEvent, events_to_payload,
    };
    use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};

    #[test]
    fn recognition_context_marks_command_sessions() {
        let context =
            super::RecognitionContext::command("__command__", Some("zh".to_owned()), "text");
        assert!(context.command_mode);
        assert_eq!(context.scene_id, "__command__");
        assert_eq!(context.language.as_deref(), Some("zh"));
        assert_eq!(context.selected_text.as_deref(), Some("text"));
    }

    #[test]
    fn mock_buffered_backend_emits_final_text_on_finish() {
        let backend = MockAsrBackend::buffered("hello");
        let descriptor = backend.describe();
        assert_eq!(
            descriptor.capabilities.delivery_mode,
            AudioDeliveryMode::Buffered
        );

        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.push_audio(&[1, 2, 3]).unwrap();
        assert!(session.poll_events().unwrap().is_empty());
        session.finish().unwrap();
        let events = session.poll_events().unwrap();
        assert_eq!(
            events,
            vec![
                RecognitionEvent::FinalText {
                    text: "hello".to_owned()
                },
                RecognitionEvent::Completed
            ]
        );
        assert_eq!(events_to_payload(&events).unwrap().commit_text, "hello");
    }

    #[test]
    fn mock_streaming_backend_emits_partial_once() {
        let backend = MockAsrBackend::streaming("partial", "final");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.push_audio(&[1]).unwrap();
        assert_eq!(
            session.poll_events().unwrap(),
            vec![RecognitionEvent::PartialText {
                text: "partial".to_owned()
            }]
        );
        session.push_audio(&[2]).unwrap();
        assert!(session.poll_events().unwrap().is_empty());
    }

    #[test]
    fn error_event_maps_to_payload() {
        let payload = events_to_payload(&[RecognitionEvent::Error {
            message: "err".to_owned(),
        }])
        .unwrap();
        assert_eq!(payload.commit_text, "err");
    }

    #[test]
    fn events_without_final_text_return_error() {
        let error = events_to_payload(&[RecognitionEvent::Completed]).unwrap_err();
        assert!(
            matches!(error, AsrError::Backend(message) if message.contains("without final text"))
        );
    }

    #[test]
    fn session_rejects_work_after_cancel() {
        let backend = MockAsrBackend::buffered("done");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.push_audio(&[1, 2]).unwrap();
        session.cancel().unwrap();

        assert!(session.poll_events().unwrap().is_empty());
        assert!(matches!(
            session.push_audio(&[3]).unwrap_err(),
            AsrError::Cancelled
        ));
        assert!(matches!(session.finish().unwrap_err(), AsrError::Cancelled));
    }

    #[test]
    fn session_rejects_audio_after_finish() {
        let backend = MockAsrBackend::buffered("done");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.finish().unwrap();
        assert!(matches!(
            session.push_audio(&[1]).unwrap_err(),
            AsrError::AlreadyFinished
        ));
    }

    #[test]
    fn backend_factory_builds_mock_provider() {
        let config = AsrConfig {
            active_provider: "mock".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "mock".to_owned(),
                kind: AsrProviderKind::Local,
                timeout_ms: None,
                model: None,
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                endpoint: None,
            }],
            ..AsrConfig::default()
        };

        let backend = AsrBackendFactory::build_active(&config).unwrap();
        assert_eq!(backend.describe().provider_id, "mock");
    }

    #[test]
    fn backend_factory_reports_unknown_active_provider() {
        let config = AsrConfig {
            active_provider: "missing".to_owned(),
            providers: Vec::new(),
            ..AsrConfig::default()
        };

        let Err(error) = AsrBackendFactory::build_active(&config) else {
            panic!("missing provider should fail");
        };
        assert!(matches!(error, AsrError::UnknownProvider(id) if id == "missing"));
    }

    #[test]
    fn backend_factory_reports_unimplemented_provider_kind() {
        let provider = AsrProviderConfig {
            id: "sherpa-onnx".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let Err(error) = AsrBackendFactory::build_provider(&provider) else {
            panic!("unsupported provider should fail");
        };
        assert!(matches!(
            error,
            AsrError::UnsupportedProviderKind { provider_id, kind }
                if provider_id == "sherpa-onnx" && kind == "local"
        ));
    }
}
