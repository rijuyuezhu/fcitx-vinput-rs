//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use thiserror::Error;
use vinput_asr::{
    AsrBackend, AsrBackendFactory, AsrError, MockAsrBackend, RecognitionContext, RecognitionEvent,
    RecognitionSession, events_to_payload,
};
use vinput_audio::{
    AudioError, AudioProcessingOptions, AudioRecorder, AudioSource, CaptureTarget, CapturedAudio,
    MockAudioSource, PcmBuffer, SourceAudioRecorder,
};
use vinput_config::{LlmAdapterConfig, VinputConfig};
use vinput_protocol::{
    AsrBackendState, RecognitionPayload, ServiceStatus, TextAdapterState, TextAdapterSummary,
};
use vinput_text::{
    AdapterProcessSpec, AdapterRegistry, AdapterRuntimePaths, AdapterStopOutcome,
    CommandTextProcessor, MockTextProcessor, ProcessCommandTextRunner, StartedAdapterProcess,
    TextProcessor, TextRequest, start_adapter_process, stop_adapter_process,
};

const MOCK_PCM: &[i16] = &[256, -128, 64, -32];
const MOCK_SILENCE_THRESHOLD: i16 = 8;
const DEFAULT_MOCK_AUDIO_FRAMES: usize = 4;

fn text_adapter_summary(adapter: &LlmAdapterConfig, pid: Option<u32>) -> TextAdapterSummary {
    TextAdapterSummary {
        id: adapter.id.clone(),
        kind: "command".to_owned(),
        command: adapter.command.clone(),
        args: adapter.args.clone(),
        env_count: adapter.env.len(),
        is_running: pid.is_some(),
        pid,
        has_working_dir: adapter.working_dir.is_some(),
    }
}

/// In-memory runtime state for the first daemon milestone.
pub struct RuntimeState {
    config: VinputConfig,
    status: ServiceStatus,
    started_at: Instant,
    current_scene: Option<String>,
    selected_text: Option<String>,
    partial_text: Option<String>,
    asr_backend: Box<dyn AsrBackend>,
    audio_recorder: Box<dyn AudioRecorder>,
    text_processor: Box<dyn TextProcessor>,
    active_session: Option<Box<dyn RecognitionSession>>,
    adapter_runtime_paths: AdapterRuntimePaths,
    adapter_processes: HashMap<String, StartedAdapterProcess>,
}

impl Drop for RuntimeState {
    fn drop(&mut self) {
        let _ = self.audio_recorder.cancel_recording();
        for (adapter_id, mut process) in self.adapter_processes.drain() {
            let _ = process.child.kill();
            let _ = process.child.wait();
            let _ = self.adapter_runtime_paths.remove_pid(&adapter_id);
        }
    }
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

    /// Builds an idle runtime from config-selected ASR and command text adapters.
    pub fn with_configured_backends(config: VinputConfig) -> Result<Self, RuntimeError> {
        let backend = AsrBackendFactory::build_active(&config.asr).map_err(RuntimeError::Asr)?;
        Self::with_configured_text(config, backend, Box::new(default_mock_audio_source()))
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

    /// Builds an idle runtime with injected ASR/audio backends and configured command text adapters.
    pub fn with_configured_text(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_source: Box<dyn AudioSource>,
    ) -> Result<Self, RuntimeError> {
        let text_processor = Box::new(CommandTextProcessor::from_configs_with_runner(
            &config.llm.adapters,
            ProcessCommandTextRunner,
        ));
        Self::with_components(config, asr_backend, audio_source, text_processor)
    }

    /// Builds an idle runtime from validated config and injected component seams.
    pub fn with_components(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_source: Box<dyn AudioSource>,
        text_processor: Box<dyn TextProcessor>,
    ) -> Result<Self, RuntimeError> {
        Self::with_recorder_components(
            config,
            asr_backend,
            Box::new(SourceAudioRecorder::new(audio_source)),
            text_processor,
        )
    }

    /// Builds an idle runtime from validated config and an injected recorder seam.
    pub fn with_audio_recorder(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_recorder: Box<dyn AudioRecorder>,
    ) -> Result<Self, RuntimeError> {
        Self::with_recorder_components(
            config,
            asr_backend,
            audio_recorder,
            Box::new(MockTextProcessor::new()),
        )
    }

    /// Builds an idle runtime from validated config and injected recorder/text seams.
    pub fn with_recorder_components(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_recorder: Box<dyn AudioRecorder>,
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
            audio_recorder,
            text_processor,
            active_session: None,
            adapter_runtime_paths: AdapterRuntimePaths::for_current_user(),
            adapter_processes: HashMap::new(),
        })
    }

    /// Overrides adapter runtime paths for tests or embedded callers.
    #[must_use]
    pub fn with_adapter_runtime_paths(mut self, paths: AdapterRuntimePaths) -> Self {
        self.adapter_runtime_paths = paths;
        self
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

    /// Parses the configured desktop capture target.
    pub fn configured_capture_target(config: &VinputConfig) -> Result<CaptureTarget, RuntimeError> {
        CaptureTarget::from_config_value(&config.global.capture_device).map_err(RuntimeError::Audio)
    }

    /// Parses this runtime's configured desktop capture target.
    pub fn capture_target_for_runtime(&self) -> Result<CaptureTarget, RuntimeError> {
        Self::configured_capture_target(&self.config)
    }

    /// Builds sanitized text adapter diagnostics from config without constructing a runtime.
    #[must_use]
    pub fn configured_text_adapter_state(config: &VinputConfig) -> TextAdapterState {
        TextAdapterState::from_adapters(
            config
                .llm
                .adapters
                .iter()
                .map(|adapter| text_adapter_summary(adapter, None))
                .collect(),
        )
    }

    /// Builds sanitized text adapter diagnostics from this runtime's current config.
    #[must_use]
    pub fn configured_text_adapter_state_for_runtime(&self) -> TextAdapterState {
        TextAdapterState::from_adapters(
            self.config
                .llm
                .adapters
                .iter()
                .map(|adapter| {
                    let pid = self
                        .adapter_processes
                        .get(&adapter.id)
                        .map(|process| process.pid);
                    text_adapter_summary(adapter, pid)
                })
                .collect(),
        )
    }

    /// Returns the only configured command text adapter id, if exactly one exists.
    #[must_use]
    pub fn single_configured_text_adapter_id(&self) -> Option<String> {
        self.configured_text_adapter_state_for_runtime()
            .single_adapter_id
    }

    /// Returns the supervised process id for a currently managed text adapter.
    #[must_use]
    pub fn text_adapter_pid(&self, adapter_id: &str) -> Option<u32> {
        self.adapter_processes
            .get(adapter_id)
            .map(|process| process.pid)
    }

    /// Returns whether a text adapter is currently supervised by this runtime.
    #[must_use]
    pub fn is_text_adapter_running(&self, adapter_id: &str) -> bool {
        self.text_adapter_pid(adapter_id).is_some()
    }

    /// Reaps supervised text adapters that have already exited.
    pub fn refresh_text_adapters(&mut self) -> Vec<String> {
        let exited_adapter_ids: Vec<_> = self
            .adapter_processes
            .iter_mut()
            .filter_map(|(adapter_id, process)| match process.child.try_wait() {
                Ok(Some(_status)) => Some(adapter_id.clone()),
                Ok(None) | Err(_) => None,
            })
            .collect();
        for adapter_id in &exited_adapter_ids {
            self.adapter_processes.remove(adapter_id);
            let _ = self.adapter_runtime_paths.remove_pid(adapter_id);
        }
        exited_adapter_ids
    }

    /// Starts a configured command text adapter process.
    pub fn start_text_adapter(&mut self, adapter_id: &str) -> Result<u32, RuntimeError> {
        if self.adapter_processes.contains_key(adapter_id) {
            return Err(RuntimeError::TextAdapterAlreadyRunning(
                adapter_id.to_owned(),
            ));
        }
        let adapter = self
            .config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == adapter_id)
            .ok_or_else(|| RuntimeError::TextAdapterNotConfigured(adapter_id.to_owned()))?;
        let spec = AdapterProcessSpec::from_config(adapter);
        let process = start_adapter_process(&spec, &self.adapter_runtime_paths)
            .map_err(RuntimeError::TextAdapterSupervisor)?;
        let pid = process.pid;
        self.adapter_processes
            .insert(adapter_id.to_owned(), process);
        Ok(pid)
    }

    /// Stops a configured command text adapter process.
    pub fn stop_text_adapter(
        &mut self,
        adapter_id: &str,
    ) -> Result<AdapterStopOutcome, RuntimeError> {
        if !self
            .configured_text_adapters()
            .contains_command_adapter(adapter_id)
        {
            return Err(RuntimeError::TextAdapterNotConfigured(
                adapter_id.to_owned(),
            ));
        }
        let outcome = stop_adapter_process(adapter_id, &self.adapter_runtime_paths)
            .map_err(RuntimeError::TextAdapterSupervisor)?;
        if let Some(mut process) = self.adapter_processes.remove(adapter_id) {
            if matches!(outcome, AdapterStopOutcome::NotRunning) {
                let _ = process.child.kill();
            }
            let _ = process.child.wait();
        }
        Ok(outcome)
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
            let pcm = self.stop_and_process_recording()?;
            session.push_pcm(&pcm).map_err(RuntimeError::Asr)?;
            self.capture_partial_events(&mut *session)?;
            session.finish().map_err(RuntimeError::Asr)?;
            let events = session.poll_events().map_err(RuntimeError::Asr)?;
            let partial_text = latest_partial_text(&events).or_else(|| self.partial_text.clone());
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

        if result.is_err() && self.audio_recorder.is_recording() {
            let _ = self.audio_recorder.cancel_recording();
        }
        self.audio_recorder.set_chunk_callback(None);
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
        let capture_target = self.capture_target_for_runtime()?;
        let context = self.recognition_context(&scene_id, selected_text.as_deref());
        let session = self
            .asr_backend
            .create_session(context)
            .map_err(RuntimeError::Asr)?;
        self.audio_recorder
            .begin_recording(capture_target)
            .map_err(RuntimeError::Audio)?;
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

    fn stop_and_process_recording(&mut self) -> Result<PcmBuffer, RuntimeError> {
        let captured = self
            .audio_recorder
            .stop_and_get_buffer()
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::RuntimeState;
    use vinput_asr::{
        AsrBackend, AsrBackendFactory, AsrError, BackendDescriptor, MockAsrBackend,
        RecognitionContext, RecognitionSession,
    };
    use vinput_audio::{CaptureTarget, CapturedAudio, MockAudioSource, PcmBuffer, PcmSpec};
    use vinput_config::{AsrProviderConfig, AsrProviderKind, VinputConfig};
    use vinput_protocol::ServiceStatus;
    use vinput_text::{AdapterRuntimePaths, TextFinisher};

    fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vinput-runtime-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ))
    }

    fn config_with_sleep_adapter(adapter_id: &str) -> VinputConfig {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: adapter_id.to_owned(),
            command: "sleep".to_owned(),
            args: vec!["30".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        config
    }

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
        assert!(runtime.partial_text().is_none());
        let report = runtime.stop_recording_report(None).unwrap();
        assert_eq!(report.partial_text.as_deref(), Some("mock partial"));
        assert_eq!(report.payload.commit_text, "mock recognition result");
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
        assert!(runtime.partial_text().is_none());
        let report = runtime.stop_recording_report(None).unwrap();
        assert_eq!(report.partial_text.as_deref(), Some("mock partial"));
        assert_eq!(report.payload.commit_text, "mock recognition result");
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
            env: std::collections::HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            working_dir: Some("/tmp/adapter-work".to_owned()),
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
        assert_eq!(
            runtime.single_configured_text_adapter_id().as_deref(),
            Some("cmd-adapter")
        );

        let state = runtime.configured_text_adapter_state_for_runtime();
        assert_eq!(state.adapter_count, 1);
        assert_eq!(state.adapter_ids, ["cmd-adapter"]);
        assert_eq!(state.single_adapter_id.as_deref(), Some("cmd-adapter"));
        assert_eq!(state.adapters[0].kind, "command");
        assert_eq!(state.adapters[0].command, "vinput-postprocess");
        assert_eq!(state.adapters[0].args, ["--json"]);
        assert_eq!(state.adapters[0].env_count, 1);
        assert!(!state.adapters[0].is_running);
        assert_eq!(state.adapters[0].pid, None);
        assert!(state.adapters[0].has_working_dir);
    }

    #[test]
    fn configured_capture_target_defaults_to_backend_default() {
        let config = VinputConfig::bundled_default().unwrap();
        let runtime = RuntimeState::new(config.clone()).unwrap();

        assert_eq!(
            RuntimeState::configured_capture_target(&config).unwrap(),
            CaptureTarget::Default
        );
        assert_eq!(
            runtime.capture_target_for_runtime().unwrap(),
            CaptureTarget::Default
        );
    }

    #[test]
    fn configured_capture_target_preserves_concrete_target_object() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.global.capture_device = "  alsa_input.usb-mic  ".to_owned();
        let runtime = RuntimeState::new(config.clone()).unwrap();
        let expected = CaptureTarget::Object("alsa_input.usb-mic".to_owned());

        assert_eq!(
            RuntimeState::configured_capture_target(&config).unwrap(),
            expected
        );
        assert_eq!(runtime.capture_target_for_runtime().unwrap(), expected);
    }

    #[test]
    fn configured_text_adapter_state_preserves_multiple_config_order() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "first".to_owned(),
            command: "first-helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "second".to_owned(),
            command: "second-helper".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });

        let state = RuntimeState::configured_text_adapter_state(&config);
        assert_eq!(state.adapter_count, 2);
        assert_eq!(state.adapter_ids, ["first", "second"]);
        assert!(state.single_adapter_id.is_none());
        assert_eq!(state.adapters[0].command, "first-helper");
        assert_eq!(state.adapters[1].command, "second-helper");
        assert_eq!(state.adapters[1].args, ["--json"]);
    }

    #[test]
    fn dropping_runtime_cleans_up_supervised_adapter() {
        let runtime_dir = unique_adapter_runtime_dir("drop-cleanup");
        let pid_path = runtime_dir.join("cmd-adapter.pid");
        let mut runtime = RuntimeState::new(config_with_sleep_adapter("cmd-adapter"))
            .unwrap()
            .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

        assert!(!runtime.is_text_adapter_running("cmd-adapter"));
        assert_eq!(runtime.text_adapter_pid("cmd-adapter"), None);
        let pid = runtime.start_text_adapter("cmd-adapter").unwrap();
        assert!(pid_path.exists());
        let state = runtime.configured_text_adapter_state_for_runtime();
        assert!(state.adapters[0].is_running);
        assert_eq!(state.adapters[0].pid, Some(pid));

        drop(runtime);

        assert!(!pid_path.exists());
        let _ = std::fs::remove_dir_all(runtime_dir);
    }

    #[test]
    fn refresh_text_adapters_reaps_exited_processes() {
        let runtime_dir = unique_adapter_runtime_dir("refresh-exited");
        let pid_path = runtime_dir.join("cmd-adapter.pid");
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "true".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let mut runtime = RuntimeState::new(config)
            .unwrap()
            .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

        runtime.start_text_adapter("cmd-adapter").unwrap();
        assert!(pid_path.exists());

        let mut exited = Vec::new();
        for _ in 0..20 {
            exited = runtime.refresh_text_adapters();
            if !exited.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(exited, ["cmd-adapter".to_owned()]);
        assert!(!runtime.is_text_adapter_running("cmd-adapter"));
        assert_eq!(runtime.text_adapter_pid("cmd-adapter"), None);
        assert!(!pid_path.exists());
        let _ = std::fs::remove_dir_all(runtime_dir);
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
                r"cat >/dev/null; printf '%s
' 'runtime command final'"
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
' 'runtime command final'"#
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

        let bytes = std::fs::read(&capture_path).unwrap();
        std::fs::remove_file(&capture_path).unwrap();
        let expected_samples = [4000_i16, -8000, 12000, -16000];
        let expected_bytes = expected_samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(bytes, expected_bytes);
        assert_eq!(runtime.status(), ServiceStatus::Idle);
    }

    #[test]
    fn configured_legacy_command_asr_report_has_no_stop_partial() {
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
                r"cat >/dev/null; printf '%s
' 'runtime final'"
                    .to_owned(),
            ],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

        runtime.start_recording().unwrap();
        let report = runtime.stop_recording_report(None).unwrap();

        assert_eq!(report.payload.commit_text, "runtime final");
        assert!(report.partial_text.is_none());
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
        assert!(runtime.partial_text().is_none());
        let report = runtime.stop_recording_report(None).unwrap();
        assert_eq!(report.partial_text.as_deref(), Some("listening"));
        assert_eq!(report.payload.commit_text, "custom final");
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
    fn configured_text_adapter_processes_prompted_scene() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.active_scene = "needs-adapter".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "needs-adapter".to_owned(),
                label: "Needs adapter".to_owned(),
                prompt: Some("polish text".to_owned()),
                provider_id: None,
                model: None,
                candidate_count: 1,
                timeout_ms: None,
                context_lines: 0,
            });
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"text":"configured final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        let audio = super::default_mock_audio_source();
        let mut runtime =
            RuntimeState::with_configured_text(config, Box::new(backend), Box::new(audio)).unwrap();

        runtime.start_recording().unwrap();
        let payload = runtime.stop_recording(None).unwrap();

        assert_eq!(payload.commit_text, "configured final");
        assert_eq!(runtime.status(), ServiceStatus::Idle);
    }

    #[test]
    fn configured_backends_process_prompted_scene_with_mock_asr() {
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
        config.scenes.active_scene = "needs-adapter".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "needs-adapter".to_owned(),
                label: "Needs adapter".to_owned(),
                prompt: Some("polish text".to_owned()),
                provider_id: None,
                model: None,
                candidate_count: 1,
                timeout_ms: None,
                context_lines: 0,
            });
        config.llm.adapters.push(vinput_config::LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"text":"configured backend final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let mut runtime = RuntimeState::with_configured_backends(config).unwrap();

        runtime.start_recording().unwrap();
        let payload = runtime.stop_recording(None).unwrap();

        assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");
        assert_eq!(payload.commit_text, "configured backend final");
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
        assert!(message.contains("text adapter backend"));
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
