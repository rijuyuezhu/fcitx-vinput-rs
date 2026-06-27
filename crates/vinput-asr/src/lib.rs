//! ASR backend contract, deterministic mock, and backend skeletons.
//!
//! This crate mirrors the original C++ daemon's recognition contract at a Rust
//! trait boundary. Real backends such as sherpa-onnx and command execution
//! should implement these traits after their contracts are covered by tests.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};
use vinput_protocol::{AsrBackendState, CandidateSource, RecognitionPayload};

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

/// Parsed external command ASR provider specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAsrSpec {
    /// Provider id from config.
    pub provider_id: String,
    /// Executable path or command name.
    pub command: String,
    /// Arguments passed to the command.
    pub args: Vec<String>,
    /// Environment variables passed to the command.
    pub env: std::collections::HashMap<String, String>,
    /// Optional model id selected for this provider.
    pub model_id: Option<String>,
    /// Optional timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl TryFrom<&AsrProviderConfig> for CommandAsrSpec {
    type Error = AsrError;

    fn try_from(provider: &AsrProviderConfig) -> Result<Self, Self::Error> {
        if provider.kind != AsrProviderKind::Command {
            return Err(AsrError::Backend(format!(
                "provider `{}` is not a command ASR provider",
                provider.id
            )));
        }
        let command = provider
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .ok_or_else(|| {
                AsrError::Backend(format!(
                    "command ASR provider `{}` must configure a command",
                    provider.id
                ))
            })?;
        Ok(Self {
            provider_id: provider.id.clone(),
            command: command.to_owned(),
            args: provider.args.clone(),
            env: provider.env.clone(),
            model_id: provider.model.clone(),
            timeout_ms: provider.timeout_ms,
        })
    }
}

/// Runner seam for command-backed ASR providers.
pub trait CommandAsrRunner: Send + Sync {
    /// Creates a recognition session for one command ASR request.
    fn create_session(
        &self,
        spec: &CommandAsrSpec,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError>;
}

/// Runner placeholder used until process execution is ported.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedCommandAsrRunner;

impl CommandAsrRunner for UnsupportedCommandAsrRunner {
    fn create_session(
        &self,
        spec: &CommandAsrSpec,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Err(AsrError::Backend(format!(
            "command ASR provider `{}` runner is not implemented yet",
            spec.provider_id
        )))
    }
}

/// Command-backed ASR backend skeleton.
#[derive(Debug, Clone)]
pub struct CommandAsrBackend<R = UnsupportedCommandAsrRunner> {
    spec: CommandAsrSpec,
    descriptor: BackendDescriptor,
    runner: R,
}

impl CommandAsrBackend<UnsupportedCommandAsrRunner> {
    /// Creates a command ASR backend skeleton from a parsed spec.
    #[must_use]
    pub fn new(spec: CommandAsrSpec) -> Self {
        Self::with_runner(spec, UnsupportedCommandAsrRunner)
    }
}

impl<R> CommandAsrBackend<R> {
    /// Creates a command ASR backend with an injected runner.
    #[must_use]
    pub fn with_runner(spec: CommandAsrSpec, runner: R) -> Self {
        let descriptor = BackendDescriptor::new(
            spec.provider_id.clone(),
            spec.model_id.clone().unwrap_or_default(),
            "Command ASR",
            BackendCapabilities::buffered(),
        );
        Self {
            spec,
            descriptor,
            runner,
        }
    }

    /// Creates a command ASR backend from typed provider config with an injected runner.
    pub fn with_config(provider: &AsrProviderConfig, runner: R) -> Result<Self, AsrError> {
        Ok(Self::with_runner(
            CommandAsrSpec::try_from(provider)?,
            runner,
        ))
    }

    /// Returns the parsed command provider spec.
    #[must_use]
    pub const fn spec(&self) -> &CommandAsrSpec {
        &self.spec
    }

    /// Returns the configured command runner.
    #[must_use]
    pub const fn runner(&self) -> &R {
        &self.runner
    }
}

impl<R: CommandAsrRunner> AsrBackend for CommandAsrBackend<R> {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        self.runner.create_session(&self.spec, context)
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
        let provider = active_provider(config)
            .ok_or_else(|| AsrError::UnknownProvider(config.active_provider.clone()))?;
        Self::build_provider(provider)
    }

    /// Parses an external command ASR provider into an executable spec.
    pub fn command_spec(provider: &AsrProviderConfig) -> Result<CommandAsrSpec, AsrError> {
        CommandAsrSpec::try_from(provider)
    }

    /// Builds a backend from one provider entry.
    pub fn build_provider(provider: &AsrProviderConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        if provider.id == "mock" {
            return Ok(Box::new(MockAsrBackend::streaming(
                "mock partial",
                "mock recognition result",
            )));
        }
        if provider.kind == AsrProviderKind::Command {
            return Ok(Box::new(CommandAsrBackend::new(Self::command_spec(
                provider,
            )?)));
        }
        unsupported_provider(&provider.id, &provider.kind)
    }

    /// Builds a user-facing ASR state snapshot from config and load outcome.
    #[must_use]
    pub fn state_for_config(config: &AsrConfig) -> AsrBackendState {
        let target_model_id = target_model_id(config);
        let remote_endpoints = remote_endpoints(config);
        match Self::build_active(config) {
            Ok(backend) => {
                let descriptor = backend.describe();
                let mut state = AsrBackendState::ready(descriptor.provider_id, descriptor.model_id);
                state.target_provider_id.clone_from(&config.active_provider);
                state.target_model_id = target_model_id;
                state.remote_endpoints = remote_endpoints;
                state
            }
            Err(error) => {
                let mut state = AsrBackendState::unavailable(
                    config.active_provider.clone(),
                    target_model_id,
                    error.to_string(),
                );
                state.remote_endpoints = remote_endpoints;
                state
            }
        }
    }
}

fn active_provider(config: &AsrConfig) -> Option<&AsrProviderConfig> {
    config
        .providers
        .iter()
        .find(|provider| provider.id == config.active_provider)
}

fn target_model_id(config: &AsrConfig) -> String {
    active_provider(config)
        .and_then(|provider| provider.model.clone())
        .unwrap_or_default()
}

fn remote_endpoints(config: &AsrConfig) -> Vec<String> {
    active_provider(config)
        .and_then(|provider| provider.endpoint.as_deref())
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(|endpoint| vec![endpoint.to_owned()])
        .unwrap_or_default()
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
        AsrBackend, AsrBackendFactory, AsrError, AudioDeliveryMode, CommandAsrBackend,
        CommandAsrRunner, CommandAsrSpec, MockAsrBackend, RecognitionContext, RecognitionEvent,
        RecognitionSession, events_to_payload,
    };
    use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};

    #[derive(Debug, Clone, Copy)]
    struct FinalTextCommandRunner;

    impl CommandAsrRunner for FinalTextCommandRunner {
        fn create_session(
            &self,
            spec: &CommandAsrSpec,
            context: RecognitionContext,
        ) -> Result<Box<dyn RecognitionSession>, AsrError> {
            MockAsrBackend::buffered(format!("{}:{}", spec.command, context.scene_id))
                .create_session(context)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ConfigEchoCommandRunner;

    impl CommandAsrRunner for ConfigEchoCommandRunner {
        fn create_session(
            &self,
            spec: &CommandAsrSpec,
            context: RecognitionContext,
        ) -> Result<Box<dyn RecognitionSession>, AsrError> {
            let scene_id = context.scene_id.clone();
            let language = context.language.clone().unwrap_or_default();
            let env_value = spec
                .env
                .get("ASR_MODE")
                .map(String::as_str)
                .unwrap_or_default();
            MockAsrBackend::buffered(format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                spec.provider_id,
                spec.command,
                spec.args.join(","),
                env_value,
                spec.model_id.as_deref().unwrap_or_default(),
                spec.timeout_ms.unwrap_or_default(),
                scene_id,
                language,
            ))
            .create_session(context)
        }
    }

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
    fn command_asr_spec_parses_provider_fields() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_500),
            model: None,
            hotwords_file: None,
            command: Some(" helper ".to_owned()),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("RUST_LOG".to_owned(), "info".to_owned())]),
            endpoint: None,
        };

        let spec = CommandAsrSpec::try_from(&provider).unwrap();
        assert_eq!(spec.provider_id, "cmd");
        assert_eq!(spec.command, "helper");
        assert_eq!(spec.args, ["--json"]);
        assert_eq!(spec.env.get("RUST_LOG").map(String::as_str), Some("info"));
        assert_eq!(spec.model_id, None);
        assert_eq!(spec.timeout_ms, Some(1_500));
    }

    #[test]
    fn backend_factory_command_spec_uses_same_parser() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some("helper".to_owned()),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let spec = AsrBackendFactory::command_spec(&provider).unwrap();
        assert_eq!(spec.provider_id, "cmd");
        assert_eq!(spec.command, "helper");
    }

    #[test]
    fn command_asr_spec_rejects_missing_command() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let error = CommandAsrSpec::try_from(&provider).unwrap_err();
        assert!(
            matches!(error, AsrError::Backend(message) if message.contains("must configure a command"))
        );
    }

    #[test]
    fn command_asr_backend_describes_configured_provider() {
        let backend = CommandAsrBackend::new(CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            model_id: Some("cmd-model".to_owned()),
            timeout_ms: Some(1_000),
        });

        let descriptor = backend.describe();
        assert_eq!(descriptor.provider_id, "cmd");
        assert_eq!(descriptor.model_id, "cmd-model");
        assert_eq!(
            descriptor.capabilities.delivery_mode,
            AudioDeliveryMode::Buffered
        );
        assert_eq!(backend.spec().command, "helper");
    }

    #[test]
    fn command_asr_backend_delegates_to_injected_runner() {
        let backend = CommandAsrBackend::with_runner(
            CommandAsrSpec {
                provider_id: "cmd".to_owned(),
                command: "helper".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                model_id: None,
                timeout_ms: None,
            },
            FinalTextCommandRunner,
        );

        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("mock runner should create a session");
        session.push_audio(&[1, 2, 3]).unwrap();
        session.finish().unwrap();
        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(payload.commit_text, "helper:raw");
    }

    #[test]
    fn command_asr_backend_passes_config_to_injected_runner() {
        let backend = CommandAsrBackend::with_runner(
            CommandAsrSpec {
                provider_id: "cmd".to_owned(),
                command: "helper".to_owned(),
                args: vec!["--format".to_owned(), "json".to_owned()],
                env: std::collections::HashMap::from([("ASR_MODE".to_owned(), "fast".to_owned())]),
                model_id: Some("paraformer".to_owned()),
                timeout_ms: Some(2_500),
            },
            ConfigEchoCommandRunner,
        );

        let mut session = backend
            .create_session(RecognitionContext::normal(
                "dictation",
                Some("zh".to_owned()),
            ))
            .expect("mock runner should create a session");
        session.finish().unwrap();

        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(
            payload.commit_text,
            "cmd|helper|--format,json|fast|paraformer|2500|dictation|zh"
        );
    }

    #[test]
    fn command_asr_backend_builds_from_provider_config_with_runner() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(2_500),
            model: Some("paraformer".to_owned()),
            hotwords_file: None,
            command: Some("helper".to_owned()),
            args: vec!["--format".to_owned(), "json".to_owned()],
            env: std::collections::HashMap::from([("ASR_MODE".to_owned(), "fast".to_owned())]),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ConfigEchoCommandRunner)
            .expect("command provider config should build");
        assert_eq!(backend.spec().provider_id, "cmd");
        assert_eq!(backend.spec().model_id.as_deref(), Some("paraformer"));

        let mut session = backend
            .create_session(RecognitionContext::normal(
                "dictation",
                Some("zh".to_owned()),
            ))
            .expect("mock runner should create a session");
        session.finish().unwrap();

        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(
            payload.commit_text,
            "cmd|helper|--format,json|fast|paraformer|2500|dictation|zh"
        );
    }

    #[test]
    fn command_asr_backend_runner_is_not_implemented_yet() {
        let backend = CommandAsrBackend::new(CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            model_id: None,
            timeout_ms: None,
        });

        let Err(error) = backend.create_session(RecognitionContext::normal("raw", None)) else {
            panic!("command ASR runner should not be implemented yet");
        };
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("runner is not implemented yet")
        ));
    }

    #[test]
    fn backend_factory_builds_command_backend_skeleton() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("helper".to_owned()),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = AsrBackendFactory::build_provider(&provider).unwrap();
        let descriptor = backend.describe();
        assert_eq!(descriptor.provider_id, "cmd");
        assert_eq!(descriptor.model_id, "cmd-model");
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

    #[test]
    fn backend_factory_state_reports_unavailable_provider() {
        let config = AsrConfig {
            active_provider: "sherpa-onnx".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "sherpa-onnx".to_owned(),
                kind: AsrProviderKind::Local,
                timeout_ms: None,
                model: Some("paraformer".to_owned()),
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                endpoint: None,
            }],
            ..AsrConfig::default()
        };

        let state = AsrBackendFactory::state_for_config(&config);
        assert_eq!(state.target_provider_id, "sherpa-onnx");
        assert_eq!(state.target_model_id, "paraformer");
        assert!(!state.has_effective_backend);
        assert!(state.last_error.contains("not implemented"));
    }

    #[test]
    fn backend_factory_state_preserves_remote_endpoint() {
        let config = AsrConfig {
            active_provider: "remote".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "remote".to_owned(),
                kind: AsrProviderKind::Remote,
                timeout_ms: None,
                model: Some("cloud-model".to_owned()),
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                endpoint: Some("https://asr.example.test".to_owned()),
            }],
            ..AsrConfig::default()
        };

        let state = AsrBackendFactory::state_for_config(&config);
        assert_eq!(state.target_provider_id, "remote");
        assert_eq!(state.target_model_id, "cloud-model");
        assert!(!state.has_effective_backend);
        assert_eq!(state.remote_endpoints, ["https://asr.example.test"]);
    }
}
