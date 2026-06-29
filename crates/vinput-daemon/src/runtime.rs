//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

mod adapter_process;
mod diagnostics;
mod errors;
mod recording;
mod reload;

pub use errors::RuntimeError;
use reload::PendingAsrReload;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use vinput_asr::{AsrBackend, AsrBackendFactory, MockAsrBackend, RecognitionSession};
use vinput_audio::{
    AudioRecorder, AudioSource, CaptureTarget, CapturedAudio, MockAudioSource, PcmBuffer,
    SourceAudioRecorder,
};
use vinput_config::VinputConfig;
use vinput_protocol::{RecognitionPayload, ServiceStatus};
use vinput_text::{
    AdapterRuntimePaths, CommandTextProcessor, MockTextProcessor, ProcessCommandTextRunner,
    StartedAdapterProcess, TextProcessor,
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

    /// Returns the latest partial text, if any.
    #[must_use]
    pub fn partial_text(&self) -> Option<&str> {
        self.partial_text.as_deref()
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

#[cfg(test)]
mod tests;
