//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

mod active_session;
mod adapter_process;
mod asr_menu;
mod capture;
mod config_io;
mod diagnostics;
mod errors;
mod output_ducker;
mod recording;
mod reload;
mod scene;

use active_session::{ActiveRecognitionSession, CaptureStartGate};
pub(crate) use asr_menu::{
    locale_candidates_from_environment, select_asr_provider, select_asr_target,
};
pub(crate) use config_io::persist_config_atomically;
pub use errors::RuntimeError;
use output_ducker::OutputDucker;
pub(crate) use reload::AsrReloadWorkerStep;
use reload::PendingAsrReload;

use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use vinpst_asr::{AsrBackend, AsrBackendFactory, MockAsrBackend, UnavailableAsrBackend};
use vinpst_audio::{
    AudioRecorder, AudioSource, CaptureTarget, CapturedAudio, MockAudioSource, PcmBuffer,
    SourceAudioRecorder,
};
use vinpst_config::VinpstConfig;
use vinpst_protocol::{RecognitionPayload, ServiceStatus};
use vinpst_text::{
    AdapterRuntimePaths, CommandTextProcessor, MockTextProcessor, OpenAiCompatibleTextProcessor,
    ProcessCommandTextRunner, ReqwestOpenAiCompatibleChatTransport, StartedAdapterProcess,
    TextProcessor,
};

const MOCK_PCM: &[i16] = &[256, -128, 64, -32];
const MOCK_SILENCE_THRESHOLD: i16 = 8;
const DEFAULT_MOCK_AUDIO_FRAMES: usize = 4;

/// In-memory runtime state for the first daemon milestone.
pub struct RuntimeState {
    config: VinpstConfig,
    status: ServiceStatus,
    started_at: Instant,
    current_scene: Option<String>,
    selected_text: Option<String>,
    partial_text: Option<String>,
    asr_backend: Box<dyn AsrBackend>,
    audio_recorder: Box<dyn AudioRecorder>,
    output_ducker: OutputDucker,
    text_processor: Box<dyn TextProcessor>,
    reload_configured_text: bool,
    active_session: Option<ActiveRecognitionSession>,
    pending_asr_reload: Option<PendingAsrReload>,
    pending_asr_reload_config: Option<(u64, VinpstConfig)>,
    asr_reload_worker_running: bool,
    asr_reload_preparing: bool,
    asr_reload_generation: u64,
    asr_reload_last_error: Option<String>,
    asr_disabled_reason: Option<String>,
    config_path: Option<PathBuf>,
    model_root: Option<PathBuf>,
    adapter_runtime_paths: AdapterRuntimePaths,
    adapter_processes: HashMap<String, StartedAdapterProcess>,
}

impl Drop for RuntimeState {
    fn drop(&mut self) {
        self.output_ducker.restore();
        self.audio_recorder.set_chunk_callback(None);
        if let Some(session) = self.active_session.take() {
            let _ = session.cancel();
        }
        let _ = self.audio_recorder.cancel_recording();
        for (_adapter_id, mut process) in self.adapter_processes.drain() {
            let _ = vinpst_text::stop_started_adapter_process(
                &mut process,
                &self.adapter_runtime_paths,
            );
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

/// ASR result waiting for scene text processing after capture has stopped.
pub(crate) struct PendingStopRecording {
    session: ActiveRecognitionSession,
    raw_payload: RecognitionPayload,
    scene: vinpst_config::SceneDefinition,
    selected_text: Option<String>,
    partial_text: Option<String>,
}

impl RuntimeState {
    /// Builds an idle runtime from validated config and a deterministic mock ASR backend.
    pub fn new(config: VinpstConfig) -> Result<Self, RuntimeError> {
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        Self::with_asr_backend(config, Box::new(backend))
    }

    /// Builds an idle runtime from config-selected ASR provider.
    pub fn with_configured_asr(config: VinpstConfig) -> Result<Self, RuntimeError> {
        let backend = AsrBackendFactory::build_active_prepared(
            &config.asr,
            Some(config.global.default_language.clone()),
        )
        .map_err(RuntimeError::Asr)?;
        Self::with_asr_backend(config, backend)
    }

    /// Builds an idle runtime from config-selected ASR and command text adapters.
    pub fn with_configured_backends(config: VinpstConfig) -> Result<Self, RuntimeError> {
        let backend = AsrBackendFactory::build_active_prepared(
            &config.asr,
            Some(config.global.default_language.clone()),
        )
        .map_err(RuntimeError::Asr)?;
        Self::with_configured_text(config, backend, Box::new(default_mock_audio_source()))
    }

    /// Builds a configured runtime that remains available when ASR initialization fails.
    pub fn with_configured_backends_or_unavailable(
        config: VinpstConfig,
    ) -> Result<Self, RuntimeError> {
        match AsrBackendFactory::build_active_prepared(
            &config.asr,
            Some(config.global.default_language.clone()),
        ) {
            Ok(backend) => {
                Self::with_configured_text(config, backend, Box::new(default_mock_audio_source()))
            }
            Err(error) => {
                let message = error.to_string();
                let backend = Box::new(UnavailableAsrBackend::new(&message));
                let mut runtime = Self::with_configured_text(
                    config,
                    backend,
                    Box::new(default_mock_audio_source()),
                )?;
                runtime.asr_reload_last_error = Some(message);
                Ok(runtime)
            }
        }
    }

    /// Builds an idle runtime from validated config and an injected ASR backend.
    pub fn with_asr_backend(
        config: VinpstConfig,
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
        config: VinpstConfig,
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
        config: VinpstConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_source: Box<dyn AudioSource>,
    ) -> Result<Self, RuntimeError> {
        let text_processor = configured_text_processor(&config);
        let mut runtime = Self::with_components(config, asr_backend, audio_source, text_processor)?;
        runtime.reload_configured_text = true;
        Ok(runtime)
    }

    /// Builds an idle runtime with an injected recorder and configured command text adapters.
    pub fn with_configured_audio_recorder(
        config: VinpstConfig,
        asr_backend: Box<dyn AsrBackend>,
        audio_recorder: Box<dyn AudioRecorder>,
    ) -> Result<Self, RuntimeError> {
        let text_processor = configured_text_processor(&config);
        let mut runtime =
            Self::with_recorder_components(config, asr_backend, audio_recorder, text_processor)?;
        runtime.reload_configured_text = true;
        Ok(runtime)
    }

    /// Builds a configured runtime with an injected recorder even when ASR is unavailable.
    pub fn with_configured_audio_recorder_or_unavailable(
        config: VinpstConfig,
        audio_recorder: Box<dyn AudioRecorder>,
    ) -> Result<Self, RuntimeError> {
        match AsrBackendFactory::build_active(&config.asr) {
            Ok(backend) => Self::with_configured_audio_recorder(config, backend, audio_recorder),
            Err(error) => {
                let message = error.to_string();
                let backend = Box::new(UnavailableAsrBackend::new(&message));
                let mut runtime =
                    Self::with_configured_audio_recorder(config, backend, audio_recorder)?;
                runtime.asr_reload_last_error = Some(message);
                Ok(runtime)
            }
        }
    }

    /// Disables ASR for this runtime until the process exits.
    ///
    /// Config and text-adapter reloads remain available, but ASR reloads keep
    /// the unavailable backend instead of constructing a configured backend.
    pub fn disable_asr(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.asr_backend = Box::new(UnavailableAsrBackend::new(&reason));
        self.asr_disabled_reason = Some(reason.clone());
        self.asr_reload_last_error = Some(reason);
        self.pending_asr_reload = None;
        self.pending_asr_reload_config = None;
        self.asr_reload_worker_running = false;
        self.asr_reload_preparing = false;
    }

    /// Builds an idle runtime from validated config and injected component seams.
    pub fn with_components(
        config: VinpstConfig,
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
        config: VinpstConfig,
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
        config: VinpstConfig,
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
            output_ducker: OutputDucker::default(),
            text_processor,
            reload_configured_text: false,
            active_session: None,
            pending_asr_reload: None,
            pending_asr_reload_config: None,
            asr_reload_worker_running: false,
            asr_reload_preparing: false,
            asr_reload_generation: 0,
            asr_reload_last_error: None,
            asr_disabled_reason: None,
            config_path: None,
            model_root: None,
            adapter_runtime_paths: AdapterRuntimePaths::for_current_user(),
            adapter_processes: HashMap::new(),
        })
    }

    /// Parses the configured desktop capture target.
    pub fn configured_capture_target(config: &VinpstConfig) -> Result<CaptureTarget, RuntimeError> {
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

fn configured_text_processor(config: &VinpstConfig) -> Box<dyn TextProcessor> {
    if config.llm.providers.is_empty() {
        Box::new(CommandTextProcessor::from_configs_with_runner(
            &config.llm.adapters,
            ProcessCommandTextRunner,
        ))
    } else {
        Box::new(OpenAiCompatibleTextProcessor::new(
            config.llm.providers.clone(),
            ReqwestOpenAiCompatibleChatTransport::new(),
        ))
    }
}

fn default_mock_audio_source() -> MockAudioSource {
    let frame = CapturedAudio::anonymous(PcmBuffer::at_default_rate(MOCK_PCM.to_vec()));
    MockAudioSource::from_frames(vec![frame; DEFAULT_MOCK_AUDIO_FRAMES])
}

#[cfg(test)]
mod tests;
