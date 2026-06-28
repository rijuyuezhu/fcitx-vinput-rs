//! ASR backend state JSON shared with the frontend and CLI.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Legacy classification for a requested ASR backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequestedAsrBackendStatus {
    /// The requested provider/model does not match the target or effective backend.
    Unknown,
    /// The config target was saved but no reload result is visible yet.
    ConfigSaved,
    /// A reload is in progress for the requested provider/model.
    ReloadInProgress,
    /// The requested provider/model is the effective backend.
    Applied,
    /// Reload failed for the target, but a previous effective backend remains usable.
    FailedStillUsingPrevious,
    /// Reload failed and no effective backend is usable.
    FailedNoUsableBackend,
}

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

    /// Returns whether a requested provider/model matches the target or effective backend.
    #[must_use]
    pub fn matches_requested_backend(
        &self,
        provider_id: &str,
        model_id: &str,
        effective: bool,
    ) -> bool {
        let (state_provider, state_model) = if effective {
            (&self.effective_provider_id, &self.effective_model_id)
        } else {
            (&self.target_provider_id, &self.target_model_id)
        };
        state_provider == provider_id && state_model == model_id
    }

    /// Classifies a requested provider/model using the legacy C++ precedence rules.
    #[must_use]
    pub fn classify_requested_backend(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> RequestedAsrBackendStatus {
        let target_matches = self.matches_requested_backend(provider_id, model_id, false);
        let effective_matches = self.matches_requested_backend(provider_id, model_id, true);

        if self.reload_in_progress && target_matches {
            return RequestedAsrBackendStatus::ReloadInProgress;
        }
        if !self.last_error.is_empty() && target_matches && !effective_matches {
            return if self.has_effective_backend {
                RequestedAsrBackendStatus::FailedStillUsingPrevious
            } else {
                RequestedAsrBackendStatus::FailedNoUsableBackend
            };
        }
        if effective_matches {
            return RequestedAsrBackendStatus::Applied;
        }
        if target_matches {
            return RequestedAsrBackendStatus::ConfigSaved;
        }
        RequestedAsrBackendStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{AsrBackendState, RequestedAsrBackendStatus};

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
    fn requested_backend_classification_matches_legacy_precedence() {
        let mut state = AsrBackendState {
            target_provider_id: "new".to_owned(),
            target_model_id: "new-model".to_owned(),
            effective_provider_id: "old".to_owned(),
            effective_model_id: "old-model".to_owned(),
            has_effective_backend: true,
            reload_in_progress: true,
            ..AsrBackendState::default()
        };
        assert_eq!(
            state.classify_requested_backend("new", "new-model"),
            RequestedAsrBackendStatus::ReloadInProgress
        );

        state.reload_in_progress = false;
        state.last_error = "load failed".to_owned();
        assert_eq!(
            state.classify_requested_backend("new", "new-model"),
            RequestedAsrBackendStatus::FailedStillUsingPrevious
        );

        state.has_effective_backend = false;
        assert_eq!(
            state.classify_requested_backend("new", "new-model"),
            RequestedAsrBackendStatus::FailedNoUsableBackend
        );

        state.last_error.clear();
        assert_eq!(
            state.classify_requested_backend("new", "new-model"),
            RequestedAsrBackendStatus::ConfigSaved
        );

        state.effective_provider_id = "new".to_owned();
        state.effective_model_id = "new-model".to_owned();
        assert_eq!(
            state.classify_requested_backend("new", "new-model"),
            RequestedAsrBackendStatus::Applied
        );
        assert_eq!(
            state.classify_requested_backend("missing", "model"),
            RequestedAsrBackendStatus::Unknown
        );
    }

    #[test]
    fn requested_backend_match_can_target_effective_or_saved_config() {
        let state = AsrBackendState {
            target_provider_id: "target".to_owned(),
            target_model_id: "target-model".to_owned(),
            effective_provider_id: "effective".to_owned(),
            effective_model_id: "effective-model".to_owned(),
            ..AsrBackendState::default()
        };

        assert!(state.matches_requested_backend("target", "target-model", false));
        assert!(!state.matches_requested_backend("target", "target-model", true));
        assert!(state.matches_requested_backend("effective", "effective-model", true));
        assert!(!state.matches_requested_backend("effective", "effective-model", false));
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
