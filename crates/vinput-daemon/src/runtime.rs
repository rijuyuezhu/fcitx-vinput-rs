//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

mod adapter_process;
mod diagnostics;
mod errors;
mod reload;

pub use errors::RuntimeError;
use reload::PendingAsrReload;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use vinput_asr::{
    AsrBackend, AsrBackendFactory, MockAsrBackend, RecognitionContext, RecognitionEvent,
    RecognitionSession, events_to_payload,
};
use vinput_audio::{
    AudioProcessingOptions, AudioRecorder, AudioSource, CaptureTarget, CapturedAudio,
    MockAudioSource, PcmBuffer, SourceAudioRecorder,
};
use vinput_config::VinputConfig;
use vinput_protocol::{RecognitionPayload, ServiceStatus};
use vinput_text::{
    AdapterRuntimePaths, CommandTextProcessor, MockTextProcessor, ProcessCommandTextRunner,
    StartedAdapterProcess, TextProcessor, TextRequest,
};

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
    audio_recorder: Box<dyn AudioRecorder>,
    text_processor: Box<dyn TextProcessor>,
    active_session: Option<Box<dyn RecognitionSession>>,
    pending_asr_reload: Option<PendingAsrReload>,
    asr_reload_last_error: Option<String>,
    adapter_runtime_paths: AdapterRuntimePaths,
    adapter_processes: HashMap<String, StartedAdapterProcess>,
}

impl Drop for RuntimeState {
    fn drop(&mut self) {
        if let Some(mut session) = self.active_session.take() {
            let _ = session.cancel();
        }
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
            pending_asr_reload: None,
            asr_reload_last_error: None,
            adapter_runtime_paths: AdapterRuntimePaths::for_current_user(),
            adapter_processes: HashMap::new(),
        })
    }

    /// Parses the configured desktop capture target.
    pub fn configured_capture_target(config: &VinputConfig) -> Result<CaptureTarget, RuntimeError> {
        CaptureTarget::from_config_value(&config.global.capture_device).map_err(RuntimeError::Audio)
    }

    /// Parses this runtime's configured desktop capture target.
    pub fn capture_target_for_runtime(&self) -> Result<CaptureTarget, RuntimeError> {
        Self::configured_capture_target(&self.config)
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
            let pcm = match self.stop_and_process_recording() {
                Ok(pcm) => pcm,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            };
            if let Err(error) = session.push_pcm(&pcm) {
                let _ = session.cancel();
                return Err(RuntimeError::Asr(error));
            }
            let mut events = match self.drain_pending_events(&mut *session) {
                Ok(events) => events,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            };
            if let Err(error) = session.finish() {
                let _ = session.cancel();
                return Err(RuntimeError::Asr(error));
            }
            match session.poll_events() {
                Ok(new_events) => events.extend(new_events),
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Asr(error));
                }
            }
            let partial_text = latest_partial_text(&events).or_else(|| self.partial_text.clone());
            let raw_payload = match events_to_payload(&events) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Asr(error));
                }
            };
            let scene_definition = self.scene_definition(&scene);
            let payload = match self.text_processor.finish(&TextRequest {
                raw_text: &raw_payload.commit_text,
                scene: &scene_definition,
                selected_text: self.selected_text.as_deref(),
            }) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Finish(error));
                }
            };
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

    fn start_recording_internal(
        &mut self,
        scene_id: String,
        selected_text: Option<String>,
    ) -> Result<(), RuntimeError> {
        self.ensure_idle()?;
        let capture_target = self.capture_target_for_runtime()?;
        let context = self.recognition_context(&scene_id, selected_text.as_deref());
        let mut session = self
            .asr_backend
            .create_session(context)
            .map_err(RuntimeError::Asr)?;
        if let Err(error) = self.audio_recorder.begin_recording(capture_target) {
            let _ = session.cancel();
            return Err(RuntimeError::Audio(error));
        }
        self.status = ServiceStatus::Recording;
        self.current_scene = Some(scene_id);
        self.selected_text = selected_text;
        self.active_session = Some(session);
        Ok(())
    }

    fn drain_pending_events(
        &mut self,
        session: &mut dyn RecognitionSession,
    ) -> Result<Vec<RecognitionEvent>, RuntimeError> {
        let mut events = Vec::new();
        for event in session.poll_events().map_err(RuntimeError::Asr)? {
            if let vinput_asr::RecognitionEvent::PartialText { text } = &event {
                self.partial_text = Some(text.clone());
            }
            events.push(event);
        }
        Ok(events)
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
        self.apply_pending_asr_backend_reload();
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

#[cfg(test)]
mod tests;
