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

    /// Creates an unavailable backend snapshot for config-selected providers that failed to load.
    #[must_use]
    pub fn unavailable(
        target_provider_id: impl Into<String>,
        target_model_id: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            target_provider_id: target_provider_id.into(),
            target_model_id: target_model_id.into(),
            last_error: error.into(),
            has_effective_backend: false,
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

    #[test]
    fn ready_state_has_effective_backend_without_remote_endpoints() {
        let state = AsrBackendState::ready("mock", "mock-streaming");
        assert_eq!(state.target_provider_id, "mock");
        assert_eq!(state.target_model_id, "mock-streaming");
        assert_eq!(state.effective_provider_id, "mock");
        assert_eq!(state.effective_model_id, "mock-streaming");
        assert!(state.has_effective_backend);
        assert!(state.remote_endpoints.is_empty());
    }

    #[test]
    fn ready_state_serializes_command_provider_metadata() {
        let state = AsrBackendState::ready("cmd", "cmd-model");
        let value = serde_json::to_value(&state).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "target_provider_id": "cmd",
                "target_model_id": "cmd-model",
                "effective_provider_id": "cmd",
                "effective_model_id": "cmd-model",
                "last_error": "",
                "reload_in_progress": false,
                "has_effective_backend": true,
                "remote_endpoints": []
            })
        );
        assert_eq!(
            serde_json::from_value::<AsrBackendState>(value).unwrap(),
            state
        );
    }

    #[test]
    fn unavailable_state_keeps_target_without_effective_backend() {
        let state = AsrBackendState::unavailable("sherpa-onnx", "paraformer", "load failed");
        assert_eq!(state.target_provider_id, "sherpa-onnx");
        assert_eq!(state.target_model_id, "paraformer");
        assert_eq!(state.last_error, "load failed");
        assert!(!state.has_effective_backend);
        assert!(state.effective_provider_id.is_empty());
        assert!(state.effective_model_id.is_empty());
        assert!(state.remote_endpoints.is_empty());
    }

    #[test]
    fn unavailable_state_serializes_command_target_metadata() {
        let state = AsrBackendState::unavailable("cmd", "cmd-model", "command missing");
        let value = serde_json::to_value(&state).unwrap();

        assert_eq!(value["target_provider_id"], "cmd");
        assert_eq!(value["target_model_id"], "cmd-model");
        assert_eq!(value["effective_provider_id"], "");
        assert_eq!(value["effective_model_id"], "");
        assert_eq!(value["last_error"], "command missing");
        assert_eq!(value["has_effective_backend"], false);
        assert_eq!(value["remote_endpoints"], serde_json::json!([]));
        assert_eq!(
            serde_json::from_value::<AsrBackendState>(value).unwrap(),
            state
        );
    }
}
