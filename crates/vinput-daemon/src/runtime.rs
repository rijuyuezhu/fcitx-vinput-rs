//! Minimal daemon runtime used before real PipeWire/ASR/D-Bus integration lands.

use std::time::{Duration, Instant};
use thiserror::Error;
use vinput_config::VinputConfig;
use vinput_protocol::{AsrBackendState, RecognitionPayload, ServiceStatus};

/// In-memory runtime state for the first daemon milestone.
#[derive(Debug, Clone)]
pub struct RuntimeState {
    config: VinputConfig,
    status: ServiceStatus,
    started_at: Instant,
    current_scene: Option<String>,
    selected_text: Option<String>,
    partial_text: Option<String>,
}

impl RuntimeState {
    /// Builds an idle runtime from validated config.
    pub fn new(config: VinputConfig) -> Result<Self, RuntimeError> {
        config.validate().map_err(RuntimeError::InvalidConfig)?;
        Ok(Self {
            config,
            status: ServiceStatus::Idle,
            started_at: Instant::now(),
            current_scene: None,
            selected_text: None,
            partial_text: None,
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
        self.ensure_idle()?;
        self.status = ServiceStatus::Recording;
        self.current_scene = Some(self.config.scenes.active_scene.clone());
        self.selected_text = None;
        self.partial_text = Some("mock partial".to_owned());
        Ok(())
    }

    /// Starts command-mode recording.
    pub fn start_command_recording(
        &mut self,
        selected_text: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        self.ensure_idle()?;
        self.status = ServiceStatus::Recording;
        self.current_scene = Some(vinput_config::COMMAND_SCENE_ID.to_owned());
        self.selected_text = Some(selected_text.into());
        self.partial_text = Some("mock command partial".to_owned());
        Ok(())
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

        let text = if scene == vinput_config::COMMAND_SCENE_ID {
            let selected = self.selected_text.as_deref().unwrap_or_default();
            if selected.is_empty() {
                "mock command result".to_owned()
            } else {
                format!("mock command result for: {selected}")
            }
        } else {
            "mock recognition result".to_owned()
        };

        self.status = ServiceStatus::Idle;
        self.current_scene = None;
        self.selected_text = None;
        self.partial_text = None;
        Ok(RecognitionPayload::raw(text))
    }

    /// Returns the latest partial text, if any.
    #[must_use]
    pub fn partial_text(&self) -> Option<&str> {
        self.partial_text.as_deref()
    }

    /// Returns a mock ASR backend state derived from config.
    #[must_use]
    pub fn asr_backend_state(&self) -> AsrBackendState {
        AsrBackendState::ready(self.config.asr.active_provider.clone(), "mock-model")
    }

    /// Reloads the ASR backend. The mock implementation only validates config.
    pub fn reload_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        Ok(self.asr_backend_state())
    }

    fn ensure_idle(&self) -> Result<(), RuntimeError> {
        if self.status == ServiceStatus::Idle {
            Ok(())
        } else {
            Err(RuntimeError::Busy(self.status))
        }
    }
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
}

#[cfg(test)]
mod tests {
    use super::RuntimeState;
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
    fn command_recording_uses_selected_text_context() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime = RuntimeState::new(config).unwrap();
        runtime.start_command_recording("hello").unwrap();
        let payload = runtime.stop_recording(None).unwrap();
        assert_eq!(payload.commit_text, "mock command result for: hello");
    }
}
