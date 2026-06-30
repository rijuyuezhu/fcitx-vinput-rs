//! ASR backend factory and config-derived diagnostic state.

use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};
use vinput_protocol::AsrBackendState;

use crate::{
    AsrBackend, AsrError, BackendCapabilities, CommandAsrBackend, CommandAsrSpec,
    LegacyCommandBatchRunner, LegacyCommandStreamingRunner, MockAsrBackend,
    SHERPA_ONNX_PROVIDER_ID, SherpaOnnxSpec,
};

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
            if is_legacy_streaming_command_provider(&provider.id) {
                return Ok(Box::new(CommandAsrBackend::with_config_and_capabilities(
                    provider,
                    LegacyCommandStreamingRunner,
                    BackendCapabilities::streaming(),
                )?));
            }
            return Ok(Box::new(CommandAsrBackend::with_config(
                provider,
                LegacyCommandBatchRunner,
            )?));
        }
        if provider.id == SHERPA_ONNX_PROVIDER_ID && provider.kind == AsrProviderKind::Local {
            let spec = SherpaOnnxSpec::from_provider(provider)?;
            return Err(spec.runtime_unavailable_error());
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

fn is_legacy_streaming_command_provider(provider_id: &str) -> bool {
    provider_id.ends_with(".streaming")
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

pub(crate) fn provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}
