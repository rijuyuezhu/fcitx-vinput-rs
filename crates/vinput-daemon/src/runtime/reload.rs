//! ASR backend reload and deferred reload handling.

use vinput_asr::AsrBackendFactory;
use vinput_protocol::{AsrBackendState, ServiceStatus};

use super::{RuntimeError, RuntimeState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingAsrReload {
    MetadataOnly,
    ConfiguredBackend,
}

impl RuntimeState {
    /// Reloads the ASR backend state after validating config.
    ///
    /// The prototype keeps the injected runtime backend, but the returned
    /// state includes the config-selected target provider, model, and remote
    /// endpoint metadata.
    pub fn reload_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        if self.status != ServiceStatus::Idle {
            return Ok(self.defer_asr_backend_reload(PendingAsrReload::MetadataOnly));
        }
        self.reload_asr_backend_now()
    }

    /// Rebuilds the runtime ASR backend from the validated active provider.
    pub fn reload_configured_asr_backend(&mut self) -> Result<AsrBackendState, RuntimeError> {
        if self.status != ServiceStatus::Idle {
            return Ok(self.defer_asr_backend_reload(PendingAsrReload::ConfiguredBackend));
        }
        self.reload_configured_asr_backend_now()
    }

    fn defer_asr_backend_reload(&mut self, pending: PendingAsrReload) -> AsrBackendState {
        self.pending_asr_reload = Some(pending);
        self.asr_backend_state()
    }

    fn reload_asr_backend_now(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        self.asr_reload_last_error = None;
        Ok(self.asr_backend_state())
    }

    fn reload_configured_asr_backend_now(&mut self) -> Result<AsrBackendState, RuntimeError> {
        self.config
            .validate()
            .map_err(RuntimeError::InvalidConfig)?;
        match AsrBackendFactory::build_active(&self.config.asr) {
            Ok(backend) => {
                self.asr_backend = backend;
                self.asr_reload_last_error = None;
                Ok(self.asr_backend_state())
            }
            Err(error) => {
                let error = RuntimeError::Asr(error);
                self.asr_reload_last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    pub(super) fn apply_pending_asr_backend_reload(&mut self) {
        if self.status != ServiceStatus::Idle {
            return;
        }
        let Some(pending) = self.pending_asr_reload.take() else {
            return;
        };

        let result = match pending {
            PendingAsrReload::MetadataOnly => self.reload_asr_backend_now(),
            PendingAsrReload::ConfiguredBackend => self.reload_configured_asr_backend_now(),
        };
        if let Err(error) = result {
            self.asr_reload_last_error = Some(format!(
                "Failed to apply deferred ASR backend reload. {error}"
            ));
        }
    }
}
