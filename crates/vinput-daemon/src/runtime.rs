//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

use std::time::{Duration, Instant};
use thiserror::Error;
use vinput_asr::{AsrBackend, AsrError, MockAsrBackend, RecognitionSession, events_to_payload};
use vinput_audio::{AudioError, AudioSource, MockAudioSource, PcmBuffer};
use vinput_config::VinputConfig;
use vinput_protocol::{AsrBackendState, RecognitionPayload, ServiceStatus};
use vinput_text::{TextFinisher, TextRequest};

const MOCK_PCM: &[i16] = &[256, -128, 64, -32];
const MOCK_SILENCE_THRESHOLD: i16 = 8;

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
    active_session: Option<Box<dyn RecognitionSession>>,
}

impl RuntimeState {
    /// Builds an idle runtime from validated config and a deterministic mock ASR backend.
    pub fn new(config: VinputConfig) -> Result<Self, RuntimeError> {
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        Self::with_asr_backend(config, Box::new(backend))
    }

    /// Builds an idle runtime from validated config and an injected ASR backend.
    pub fn with_asr_backend(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
    ) -> Result<Self, RuntimeError> {
        Self::with_backends(config, asr_backend, Box::new(default_mock_audio_source()))
    }

    /// Builds an idle runtime from validated config and injected backend seams.
    pub fn with_backends(
        config: VinputConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_source: Box<dyn AudioSource>,
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
            active_session: None,
        })
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
        if self.status != ServiceStatus::Recording {
            return Err(RuntimeError::NotRecording(self.status));
        }

        self.status = ServiceStatus::Inferring;
        let scene = scene_id
            .map(ToOwned::to_owned)
            .or_else(|| self.current_scene.clone())
            .unwrap_or_else(|| self.config.scenes.active_scene.clone());

        let mut session = self
            .active_session
            .take()
            .ok_or(RuntimeError::MissingAsrSession)?;
        let pcm = self.read_captured_pcm()?;
        session
            .push_audio(pcm.samples())
            .map_err(RuntimeError::Asr)?;
        self.capture_partial_events(&mut *session)?;
        session.finish().map_err(RuntimeError::Asr)?;
        let events = session.poll_events().map_err(RuntimeError::Asr)?;
        let raw_payload = events_to_payload(&events).map_err(RuntimeError::Asr)?;
        let scene_definition = self.scene_definition(&scene);
        let payload = TextFinisher::finish(&TextRequest {
            raw_text: &raw_payload.commit_text,
            scene: &scene_definition,
            selected_text: self.selected_text.as_deref(),
        })
        .map_err(RuntimeError::Finish)?;

        self.reset_to_idle();
        Ok(payload)
    }

    /// Returns the latest partial text, if any.
    #[must_use]
    pub fn partial_text(&self) -> Option<&str> {
        self.partial_text.as_deref()
    }

    /// Returns a mock ASR backend state derived from config and backend descriptor.
    #[must_use]
    pub fn asr_backend_state(&self) -> AsrBackendState {
        let descriptor = self.asr_backend.describe();
        let mut state = AsrBackendState::ready(descriptor.provider_id, descriptor.model_id);
        state
            .target_provider_id
            .clone_from(&self.config.asr.active_provider);
        state
    }

    /// Reloads the ASR backend. The mock implementation only validates config.
    pub fn reload_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        Ok(self.asr_backend_state())
    }

    fn start_recording_internal(
        &mut self,
        scene_id: String,
        selected_text: Option<String>,
    ) -> Result<(), RuntimeError> {
        self.ensure_idle()?;
        let mut session = self
            .asr_backend
            .create_session()
            .map_err(RuntimeError::Asr)?;
        let pcm = self.read_captured_pcm()?;
        session
            .push_audio(pcm.samples())
            .map_err(RuntimeError::Asr)?;
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

    fn read_captured_pcm(&mut self) -> Result<PcmBuffer, RuntimeError> {
        let pcm = self
            .audio_source
            .read_buffer()
            .map_err(RuntimeError::Audio)?;
        Ok(self.process_captured_pcm(&pcm))
    }

    fn process_captured_pcm(&self, pcm: &PcmBuffer) -> PcmBuffer {
        let mut pcm = pcm.trimmed_silence(MOCK_SILENCE_THRESHOLD);
        if self.config.asr.normalize_audio {
            pcm.normalize_to_peak(16_000);
        }
        pcm.apply_gain(self.config.asr.input_gain);
        pcm
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
                candidate_count: 0,
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
    let frame = PcmBuffer::at_default_rate(MOCK_PCM.to_vec());
    MockAudioSource::from_frames(vec![frame.clone(), frame])
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
    use super::RuntimeState;
    use vinput_asr::MockAsrBackend;
    use vinput_audio::{MockAudioSource, PcmBuffer};
    use vinput_config::VinputConfig;
    use vinput_protocol::ServiceStatus;

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
            PcmBuffer::at_default_rate(vec![0, 32, -32, 0]),
            PcmBuffer::at_default_rate(vec![0, 64, -64, 0]),
        ]);
        let mut runtime =
            RuntimeState::with_backends(config, Box::new(backend), Box::new(source)).unwrap();
        runtime.start_recording().unwrap();
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "custom final");
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
