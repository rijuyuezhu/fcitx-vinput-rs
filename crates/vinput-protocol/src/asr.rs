//! ASR backend state JSON shared with the frontend and CLI.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Snapshot returned by the legacy `GetAsrBackendState` method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct AsrBackendState {
    /// Provider requested by config.
    #[serde(default)]
    pub target_provider_id: String,
    /// Model requested by config.
    #[serde(default)]
    pub target_model_id: String,
    /// Provider currently loaded by the daemon.
    #[serde(default)]
    pub effective_provider_id: String,
    /// Model currently loaded by the daemon.
    #[serde(default)]
    pub effective_model_id: String,
    /// Last reload or runtime error, if any.
    #[serde(default)]
    pub last_error: String,
    /// Whether a reload worker is currently replacing the backend.
    #[serde(default)]
    pub reload_in_progress: bool,
    /// Whether the daemon has a usable backend.
    #[serde(default)]
    pub has_effective_backend: bool,
    /// Remote ASR endpoint labels known to the daemon.
    #[serde(default)]
    pub remote_endpoints: Vec<String>,
}

impl AsrBackendState {
    /// Creates a ready local backend snapshot.
    #[must_use]
    pub fn ready(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        let provider_id = provider_id.into();
        let model_id = model_id.into();
        Self {
            target_provider_id: provider_id.clone(),
            target_model_id: model_id.clone(),
            effective_provider_id: provider_id,
            effective_model_id: model_id,
            has_effective_backend: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AsrBackendState;

    #[test]
    fn missing_fields_default_for_legacy_tolerance() {
        let state: AsrBackendState =
            serde_json::from_str(r#"{"target_provider_id":"sherpa-onnx"}"#).unwrap();
        assert_eq!(state.target_provider_id, "sherpa-onnx");
        assert!(!state.has_effective_backend);
        assert!(state.remote_endpoints.is_empty());
    }
}
