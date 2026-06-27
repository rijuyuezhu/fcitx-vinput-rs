//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

use std::time::{Duration, Instant};
use thiserror::Error;
use vinput_asr::{
    AsrBackend, AsrBackendFactory, AsrError, MockAsrBackend, RecognitionContext, RecognitionEvent,
    RecognitionSession, events_to_payload,
};
use vinput_audio::{
    AudioError, AudioProcessingOptions, AudioSource, CapturedAudio, MockAudioSource, PcmBuffer,
};
use vinput_config::VinputConfig;
use vinput_protocol::{AsrBackendState, RecognitionPayload, ServiceStatus};
use vinput_text::{AdapterRegistry, MockTextProcessor, TextProcessor, TextRequest};

const MOCK_PCM: &[i16] = &[256, -128, 64, -32];
const MOCK_SILENCE_THRESHOLD: i16 = 8;
const DEFAULT_MOCK_AUDIO_FRAMES: usize = 4;

/// In-memory runtime state for the first daemon milestone.
pub struct RuntimeState {
    config: VinputConfig,
    status: ServiceStatus,
    started_at: Instant,
    current_scene: Option<String>,
    selected_text: Option<String>,
    partial_text: Option<String>,
    asr_backend: Box<dyn AsrBackend>,
    audio_source: Box<dyn AudioSource>,
    text_processor: Box<dyn TextProcessor>,
    active_session: Option<Box<dyn RecognitionSession>>,
}

/// Payload and stop-time metadata produced by a completed recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopRecordingReport {
    /// Final recognition payload after scene text processing.
    pub payload: RecognitionPayload,
    /// Latest partial text emitted while finishing the ASR session, if any.
    pub partial_text: Option<String>,
}

impl RuntimeState {
    /// Builds an idle runtime from validated config and a deterministic mock ASR backend.
    pub fn new(config: VinputConfig) -> Result<Self, RuntimeError> {
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        Self::with_asr_backend(config, Box::new(backend))
    }

    /// Builds an idle runtime from config-selected ASR provider.
    pub fn with_configured_asr(config: VinputConfig) -> Result<Self, RuntimeError> {
        let backend = AsrBackendFactory::build_active(&config.asr).map_err(RuntimeError::Asr)?;
        Self::with_asr_backend(config, backend)
    }

    /// Builds an idle runtime from validated config and an injected ASR backend.
    pub fn with_asr_backend(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
    ) -> Result<Self, RuntimeError> {
        Self::with_components(
            config,
            asr_backend,
            Box::new(default_mock_audio_source()),
            Box::new(MockTextProcessor::new()),
        )
    }

    /// Builds an idle runtime from validated config and injected backend seams.
    pub fn with_backends(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_source: Box<dyn AudioSource>,
    ) -> Result<Self, RuntimeError> {
        Self::with_components(
            config,
            asr_backend,
            audio_source,
            Box::new(MockTextProcessor::new()),
        )
    }

    /// Builds an idle runtime from validated config and injected component seams.
    pub fn with_components(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_source: Box<dyn AudioSource>,
        text_processor: Box<dyn TextProcessor>,
    ) -> Result<Self, RuntimeError> {
        config.validate().map_err(RuntimeError::InvalidConfig)?;
        Ok(Self {
            config,
            status: ServiceStatus::Idle,
            started_at: Instant::now(),
            current_scene: None,
            selected_text: None,
            partial_text: None,
            asr_backend,
            audio_source,
            text_processor,
            active_session: None,
        })
    }

    /// Builds a diagnostic ASR state from config without constructing a runtime.
    #[must_use]
    pub fn configured_asr_state(config: &VinputConfig) -> AsrBackendState {
        AsrBackendFactory::state_for_config(&config.asr)
    }

    /// Builds a diagnostic ASR state from this runtime's current config.
    #[must_use]
    pub fn configured_asr_state_for_runtime(&self) -> AsrBackendState {
        Self::configured_asr_state(&self.config)
    }

    /// Builds a text adapter registry from this runtime's current config.
    #[must_use]
    pub fn configured_text_adapters(&self) -> AdapterRegistry {
        AdapterRegistry::from_configs(&self.config.llm.adapters)
    }

    /// Current daemon status.
    #[must_use]
    pub const fn status(&self) -> ServiceStatus {
        self.status
    }

    /// Returns how long the mock runtime has been alive.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Starts normal recording.
    pub fn start_recording(&mut self) -> Result<(), RuntimeError> {
        self.start_recording_internal(self.config.scenes.active_scene.clone(), None)
    }

    /// Starts command-mode recording.
    pub fn start_command_recording(
        &mut self,
        selected_text: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        self.start_recording_internal(
            vinput_config::COMMAND_SCENE_ID.to_owned(),
            Some(selected_text.into()),
        )
    }

    /// Stops recording and returns a deterministic mock result payload.
    pub fn stop_recording(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<RecognitionPayload, RuntimeError> {
        Ok(self.stop_recording_report(scene_id)?.payload)
    }

    /// Stops recording and returns final payload plus stop-time ASR metadata.
    pub fn stop_recording_report(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<StopRecordingReport, RuntimeError> {
        if self.status != ServiceStatus::Recording {
            return Err(RuntimeError::NotRecording(self.status));
        }

        self.status = ServiceStatus::Inferring;
        let scene = scene_id
            .map(ToOwned::to_owned)
            .or_else(|| self.current_scene.clone())
            .unwrap_or_else(|| self.config.scenes.active_scene.clone());

        let result = (|| {
            let mut session = self
                .active_session
                .take()
                .ok_or(RuntimeError::MissingAsrSession)?;
            let pcm = self.read_captured_pcm()?;
            session.push_pcm(&pcm).map_err(RuntimeError::Asr)?;
            self.capture_partial_events(&mut *session)?;
            session.finish().map_err(RuntimeError::Asr)?;
            let events = session.poll_events().map_err(RuntimeError::Asr)?;
            let partial_text = latest_partial_text(&events);
            let raw_payload = events_to_payload(&events).map_err(RuntimeError::Asr)?;
            let scene_definition = self.scene_definition(&scene);
            let payload = self
                .text_processor
                .finish(&TextRequest {
                    raw_text: &raw_payload.commit_text,
                    scene: &scene_definition,
                    selected_text: self.selected_text.as_deref(),
                })
                .map_err(RuntimeError::Finish)?;
            Ok(StopRecordingReport {
                payload,
                partial_text,
            })
        })();

        self.reset_to_idle();
        result
    }

    /// Returns the latest partial text, if any.
    #[must_use]
    pub fn partial_text(&self) -> Option<&str> {
        self.partial_text.as_deref()
    }

    /// Returns an ASR backend state derived from config and backend descriptor.
    #[must_use]
    pub fn asr_backend_state(&self) -> AsrBackendState {
        let descriptor = self.asr_backend.describe();
        let configured = Self::configured_asr_state(&self.config);
        let mut state = AsrBackendState::ready(descriptor.provider_id, descriptor.model_id);
        state.target_provider_id = configured.target_provider_id;
        state.target_model_id = configured.target_model_id;
        state.remote_endpoints = configured.remote_endpoints;
        state
    }

    /// Reloads the ASR backend state after validating config.
    ///
    /// The prototype keeps the injected runtime backend, but the returned
    /// state includes the config-selected target provider, model, and remote
    /// endpoint metadata.
    pub fn reload_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        Ok(self.asr_backend_state())
    }

    /// Rebuilds the runtime ASR backend from the validated active provider.
    pub fn reload_configured_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        self.asr_backend =
            AsrBackendFactory::build_active(&self.config.asr).map_err(RuntimeError::Asr)?;
        Ok(self.asr_backend_state())
    }

    fn start_recording_internal(
        &mut self,
        scene_id: String,
        selected_text: Option<String>,
    ) -> Result<(), RuntimeError> {
        self.ensure_idle()?;
        let context = self.recognition_context(&scene_id, selected_text.as_deref());
        let mut session = self
            .asr_backend
            .create_session(context)
            .map_err(RuntimeError::Asr)?;
        let pcm = self.read_captured_pcm()?;
        session.push_pcm(&pcm).map_err(RuntimeError::Asr)?;
        self.capture_partial_events(&mut *session)?;
        self.status = ServiceStatus::Recording;
        self.current_scene = Some(scene_id);
        self.selected_text = selected_text;
        self.active_session = Some(session);
        Ok(())
    }

    fn capture_partial_events(
        &mut self,
        session: &mut dyn RecognitionSession,
    ) -> Result<(), RuntimeError> {
        for event in session.poll_events().map_err(RuntimeError::Asr)? {
            if let vinput_asr::RecognitionEvent::PartialText { text } = event {
                self.partial_text = Some(text);
            }
        }
        Ok(())
    }

    fn recognition_context(
        &self,
        scene_id: &str,
        selected_text: Option<&str>,
    ) -> RecognitionContext {
        if scene_id == vinput_config::COMMAND_SCENE_ID {
            RecognitionContext::command(
                scene_id.to_owned(),
                Some(self.config.global.default_language.clone()),
                selected_text.unwrap_or_default().to_owned(),
            )
        } else {
            RecognitionContext::normal(
                scene_id.to_owned(),
                Some(self.config.global.default_language.clone()),
            )
        }
    }

    fn read_captured_pcm(&mut self) -> Result<PcmBuffer, RuntimeError> {
        let captured = self
            .audio_source
            .read_buffer()
            .map_err(RuntimeError::Audio)?;
        Ok(self.process_captured_pcm(&captured.pcm))
    }

    fn process_captured_pcm(&self, pcm: &PcmBuffer) -> PcmBuffer {
        self.audio_processing_options().process(pcm)
    }

    fn audio_processing_options(&self) -> AudioProcessingOptions {
        AudioProcessingOptions::new(
            MOCK_SILENCE_THRESHOLD,
            self.config.asr.normalize_audio.then_some(16_000),
            self.config.asr.input_gain,
        )
    }

    fn scene_definition(&self, scene_id: &str) -> vinput_config::SceneDefinition {
        self.config
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == scene_id)
            .cloned()
            .unwrap_or_else(|| vinput_config::SceneDefinition {
                id: scene_id.to_owned(),
                label: scene_id.to_owned(),
                prompt: None,
                provider_id: None,
                model: None,
                candidate_count: 0,
                timeout_ms: None,
                context_lines: 0,
            })
    }

    fn reset_to_idle(&mut self) {
        self.status = ServiceStatus::Idle;
        self.current_scene = None;
        self.selected_text = None;
        self.partial_text = None;
        self.active_session = None;
    }

    fn ensure_idle(&self) -> Result<(), RuntimeError> {
        if self.status == ServiceStatus::Idle {
            Ok(())
        } else {
            Err(RuntimeError::Busy(self.status))
        }
    }
}

fn default_mock_audio_source() -> MockAudioSource {
    let frame = CapturedAudio::anonymous(PcmBuffer::at_default_rate(MOCK_PCM.to_vec()));
    MockAudioSource::from_frames(vec![frame; DEFAULT_MOCK_AUDIO_FRAMES])
}

fn latest_partial_text(events: &[RecognitionEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| match event {
        RecognitionEvent::PartialText { text } => Some(text.clone()),
        RecognitionEvent::FinalText { .. }
        | RecognitionEvent::Error { .. }
        | RecognitionEvent::Completed => None,
    })
}

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
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::RuntimeState;
    use vinput_asr::{
        AsrBackend, AsrBackendFactory, AsrError, BackendDescriptor, CommandAsrRequest,
        MockAsrBackend, RecognitionContext, RecognitionSession,
    };
    use vinput_audio::{CapturedAudio, MockAudioSource, PcmBuffer, PcmSpec};
    use vinput_config::{AsrProviderConfig, AsrProviderKind, VinputConfig};
    use vinput_protocol::ServiceStatus;
    use vinput_text::TextFinisher;

    struct ContextRecordingBackend {
        inner: MockAsrBackend,
        captured: Arc<Mutex<Option<RecognitionContext>>>,
    }

    impl ContextRecordingBackend {
        fn new(captured: Arc<Mutex<Option<RecognitionContext>>>) -> Self {
            Self {
                inner: MockAsrBackend::streaming("listening", "custom final"),
                captured,
            }
        }
    }

    impl AsrBackend for ContextRecordingBackend {
        fn describe(&self) -> BackendDescriptor {
            self.inner.describe()
        }

        fn create_session(
            &self,
            context: RecognitionContext,
        ) -> Result<Box<dyn RecognitionSession>, AsrError> {
            *self.captured.lock().expect("context lock poisoned") = Some(context.clone());
            self.inner.create_session(context)
        }
    }

    #[test]
    fn duplicate_start_is_rejected_while_recording() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();

        runtime.start_recording().unwrap();
        let error = runtime
            .start_command_recording("selected text")
            .unwrap_err();

        assert!(matches!(
            error,
            super::RuntimeError::Busy(ServiceStatus::Recording)
        ));
        assert_eq!(runtime.status(), ServiceStatus::Recording);
        assert_eq!(runtime.partial_text(), Some("mock partial"));
        assert_eq!(
            runtime.stop_recording(None).unwrap().commit_text,
            "mock recognition result"
        );
    }

    #[test]
    fn stop_while_idle_is_rejected_without_state_changes() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();

        let error = runtime.stop_recording(None).unwrap_err();

        assert!(matches!(
            error,
            super::RuntimeError::NotRecording(ServiceStatus::Idle)
        ));
        assert_eq!(runtime.status(), ServiceStatus::Idle);
        assert!(runtime.partial_text().is_none());
    }

    #[test]
    fn normal_recording_mock_roundtrip_returns_to_idle() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();
        runtime.start_recording().unwrap();
        assert_eq!(runtime.status(), ServiceStatus::Recording);
        assert_eq!(runtime.partial_text(), Some("mock partial"));
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "mock recognition result");
        assert_eq!(runtime.status(), ServiceStatus::Idle);
    }

    #[test]
    fn default_mock_audio_source_supports_two_roundtrips() {
        assert_eq!(super::DEFAULT_MOCK_AUDIO_FRAMES, 4);
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();

        runtime.start_recording().unwrap();
        assert_eq!(
            runtime.stop_recording(None).unwrap().commit_text,
            "mock recognition result"
        );
        runtime.start_command_recording("selected text").unwrap();
        assert_eq!(
            runtime.stop_recording(None).unwrap().commit_text,
            "mock command result for: selected text"
        );
        assert_eq!(runtime.status(), ServiceStatus::Idle);
    }

    #[test]
    fn configured_asr_state_reports_default_backend_without_runtime() {
        let config = VinputConfig::bundled_default().unwrap();
        let state = RuntimeState::configured_asr_state(&config);
        assert_eq!(state.target_provider_id, "sherpa-onnx");
        assert!(!state.has_effective_backend);
        assert!(!state.last_error.is_empty());
    }

    #[test]
    fn configured_text_adapters_index_runtime_config() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let runtime = RuntimeState::new(config).unwrap();
        let registry = runtime.configured_text_adapters();

        assert_eq!(registry.len(), 1);
        assert!(registry.contains_command_adapter("cmd-adapter"));
        assert!(!registry.contains_command_adapter("missing"));
        assert_eq!(
            registry
                .command_adapter("cmd-adapter")
                .map(vinput_text::CommandTextAdapter::command),
            Some("vinput-postprocess")
        );
    }

    #[test]
    fn configured_asr_builds_mock_provider() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "mock".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let runtime = RuntimeState::with_configured_asr(config).unwrap();
        assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");
    }

    #[test]
    fn reload_asr_backend_keeps_injected_backend() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("helper".to_owned()),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let mut runtime = RuntimeState::new(config).unwrap();

        let state = runtime.reload_asr_backend().unwrap();
        assert_eq!(state.effective_provider_id, "mock");
        assert_eq!(state.target_provider_id, "cmd");
        assert_eq!(state.target_model_id, "cmd-model");
    }

    #[test]
    fn configured_command_asr_provider_runs_process_helper() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"text":"runtime command final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

        runtime.start_recording().unwrap();
        let payload = runtime.stop_recording(None).unwrap();

        assert_eq!(payload.commit_text, "runtime command final");
        assert_eq!(runtime.status(), ServiceStatus::Idle);
    }

    #[test]
    fn configured_command_asr_provider_forwards_runtime_pcm_metadata() {
        let mut capture_path = std::env::temp_dir();
        capture_path.push(format!(
            "vinput-runtime-command-asr-request-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat > "$ASR_REQUEST"; printf '%s
' '{"text":"runtime command final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::from([(
                "ASR_REQUEST".to_owned(),
                capture_path.to_string_lossy().into_owned(),
            )]),
            endpoint: None,
        });
        let backend = AsrBackendFactory::build_active(&config.asr).unwrap();
        let pcm = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 48_000,
                channels: 2,
            },
            vec![16, -32, 48, -64],
        )
        .unwrap();
        let audio = CapturedAudio::named(pcm, "fixture");
        let mut runtime = RuntimeState::with_backends(
            config,
            backend,
            Box::new(MockAudioSource::from_frames(vec![audio.clone(), audio])),
        )
        .unwrap();

        runtime.start_recording().unwrap();
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "runtime command final");

        let request: CommandAsrRequest =
            serde_json::from_str(&std::fs::read_to_string(&capture_path).unwrap()).unwrap();
        std::fs::remove_file(&capture_path).unwrap();
        assert_eq!(request.pcm.sample_rate_hz, 48_000);
        assert_eq!(request.pcm.channels, 2);
        assert_eq!(request.samples.len(), 8);
        assert_eq!(runtime.status(), ServiceStatus::Idle);
    }

    #[test]
    fn configured_command_asr_report_includes_stop_partial() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"partial_text":"runtime partial","text":"runtime final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

        runtime.start_recording().unwrap();
        let report = runtime.stop_recording_report(None).unwrap();

        assert_eq!(report.payload.commit_text, "runtime final");
        assert_eq!(report.partial_text.as_deref(), Some("runtime partial"));
        assert_eq!(runtime.status(), ServiceStatus::Idle);
        assert!(runtime.partial_text().is_none());
    }

    #[test]
    fn configured_asr_state_preserves_command_provider_metadata() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_500),
            model: Some("cmd-model".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("helper".to_owned()),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let runtime = RuntimeState::with_configured_asr(config).unwrap();

        let state = runtime.asr_backend_state();
        assert!(state.has_effective_backend);
        assert_eq!(state.target_provider_id, "cmd");
        assert_eq!(state.target_model_id, "cmd-model");
        assert_eq!(state.effective_provider_id, "cmd");
        assert_eq!(state.effective_model_id, "cmd-model");
    }

    #[test]
    fn reload_configured_asr_backend_swaps_to_configured_provider() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "mock".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: Some("mock-model".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let mut runtime = RuntimeState::new(config).unwrap();
        assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");

        let state = runtime.reload_configured_asr_backend().unwrap();
        assert_eq!(state.effective_provider_id, "mock");
        assert_eq!(state.effective_model_id, "mock-streaming");
        assert_eq!(state.target_model_id, "mock-model");
        assert!(state.has_effective_backend);
    }

    #[test]
    fn reload_configured_asr_backend_reports_build_errors_without_swapping() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();
        let before = runtime.asr_backend_state();

        let Err(error) = runtime.reload_configured_asr_backend() else {
            panic!("default unsupported configured ASR should fail to build");
        };

        assert!(matches!(error, super::RuntimeError::Asr(_)));
        let after = runtime.asr_backend_state();
        assert_eq!(after.effective_provider_id, before.effective_provider_id);
        assert_eq!(after.effective_model_id, before.effective_model_id);
    }

    #[test]
    fn runtime_asr_state_preserves_configured_target_metadata() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "remote".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "remote".to_owned(),
            kind: AsrProviderKind::Remote,
            timeout_ms: None,
            model: Some("cloud-model".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: Some("https://asr.example.test".to_owned()),
        });
        let runtime = RuntimeState::new(config).unwrap();

        let state = runtime.asr_backend_state();
        assert_eq!(state.target_provider_id, "remote");
        assert_eq!(state.target_model_id, "cloud-model");
        assert_eq!(state.effective_provider_id, "mock");
        assert_eq!(state.effective_model_id, "mock-streaming");
        assert_eq!(state.remote_endpoints, ["https://asr.example.test"]);
    }

    #[test]
    fn configured_asr_reports_default_backend_as_unsupported() {
        let config = VinputConfig::bundled_default().unwrap();
        let Err(error) = RuntimeState::with_configured_asr(config) else {
            panic!("default backend should be unsupported in current prototype");
        };
        assert!(matches!(error, super::RuntimeError::Asr(_)));
    }

    #[test]
    fn injected_asr_backend_drives_normal_result() {
        let config = VinputConfig::bundled_default().unwrap();
        let backend = MockAsrBackend::streaming("listening", "custom final");
        let mut runtime = RuntimeState::with_asr_backend(config, Box::new(backend)).unwrap();
        runtime.start_recording().unwrap();
        assert_eq!(runtime.partial_text(), Some("listening"));
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "custom final");
    }

    #[test]
    fn injected_audio_source_is_used_by_runtime() {
        let config = VinputConfig::bundled_default().unwrap();
        let backend = MockAsrBackend::streaming("listening", "custom final");
        let source = MockAudioSource::from_frames(vec![
            CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 32, -32, 0])),
            CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 64, -64, 0])),
        ]);
        let mut runtime =
            RuntimeState::with_backends(config, Box::new(backend), Box::new(source)).unwrap();
        runtime.start_recording().unwrap();
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "custom final");
    }

    #[test]
    fn command_recording_passes_context_to_asr_backend() {
        let config = VinputConfig::bundled_default().unwrap();
        let captured = Arc::new(Mutex::new(None));
        let backend = ContextRecordingBackend::new(Arc::clone(&captured));
        let mut runtime = RuntimeState::with_asr_backend(config, Box::new(backend)).unwrap();

        runtime.start_command_recording("selected text").unwrap();

        let context = captured
            .lock()
            .expect("context lock poisoned")
            .clone()
            .expect("ASR backend should receive context");
        assert!(context.command_mode);
        assert_eq!(context.scene_id, vinput_config::COMMAND_SCENE_ID);
        assert_eq!(context.language.as_deref(), Some("zh"));
        assert_eq!(context.selected_text.as_deref(), Some("selected text"));
    }

    #[test]
    fn timeout_scene_finish_error_returns_runtime_to_idle() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.active_scene = "timeout-scene".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "timeout-scene".to_owned(),
                label: "Timeout scene".to_owned(),
                prompt: None,
                provider_id: None,
                model: None,
                candidate_count: 0,
                timeout_ms: Some(2500),
                context_lines: 0,
            });
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        let audio = super::default_mock_audio_source();
        let mut runtime = RuntimeState::with_components(
            config,
            Box::new(backend),
            Box::new(audio),
            Box::new(TextFinisher::new()),
        )
        .unwrap();

        runtime.start_recording().unwrap();
        let error = runtime.stop_recording(None).unwrap_err();
        let message = error.to_string();

        assert!(matches!(error, super::RuntimeError::Finish(_)));
        assert!(message.contains("text adapter/postprocess backend"));
        assert_eq!(runtime.status(), ServiceStatus::Idle);
        assert!(runtime.partial_text().is_none());
    }

    #[test]
    fn failed_text_finishing_returns_runtime_to_idle() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.active_scene = "needs-adapter".to_owned();
        config.llm.providers.push(vinput_config::LlmProviderConfig {
            id: "openai".to_owned(),
            base_url: "https://example.invalid/v1".to_owned(),
            api_key: String::new(),
            model: None,
            extra_body: serde_json::json!({}),
            extra: std::collections::HashMap::new(),
        });
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "needs-adapter".to_owned(),
                label: "Needs adapter".to_owned(),
                prompt: Some("polish text".to_owned()),
                provider_id: Some("openai".to_owned()),
                model: None,
                candidate_count: 1,
                timeout_ms: None,
                context_lines: 0,
            });
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        let audio = super::default_mock_audio_source();
        let mut runtime = RuntimeState::with_components(
            config,
            Box::new(backend),
            Box::new(audio),
            Box::new(TextFinisher::new()),
        )
        .unwrap();

        runtime.start_recording().unwrap();
        let error = runtime.stop_recording(None).unwrap_err();

        assert!(matches!(error, super::RuntimeError::Finish(_)));
        assert_eq!(runtime.status(), ServiceStatus::Idle);
        assert!(runtime.partial_text().is_none());
    }

    #[test]
    fn command_recording_uses_selected_text_context() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();
        runtime.start_command_recording("hello").unwrap();
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "mock command result for: hello");
    }
}
