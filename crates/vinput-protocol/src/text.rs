//! Text adapter diagnostic state shared with the frontend and CLI.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Snapshot of configured text adapter diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct TextAdapterState {
    /// Number of configured text adapters.
    #[serde(default)]
    pub adapter_count: usize,
    /// Configured adapter ids in config order.
    #[serde(default)]
    pub adapter_ids: Vec<String>,
    /// Adapter id when exactly one adapter is configured.
    #[serde(default)]
    pub single_adapter_id: Option<String>,
    /// Sanitized adapter summaries in config order.
    #[serde(default)]
    pub adapters: Vec<TextAdapterSummary>,
}

impl TextAdapterState {
    /// Builds a diagnostic state from sanitized adapter summaries.
    #[must_use]
    pub fn from_adapters(adapters: Vec<TextAdapterSummary>) -> Self {
        let adapter_ids: Vec<_> = adapters.iter().map(|adapter| adapter.id.clone()).collect();
        let single_adapter_id = if adapter_ids.len() == 1 {
            Some(adapter_ids[0].clone())
        } else {
            None
        };

        Self {
            adapter_count: adapter_ids.len(),
            adapter_ids,
            single_adapter_id,
            adapters,
        }
    }
}

/// Sanitized summary for one configured text adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct TextAdapterSummary {
    /// Stable adapter id.
    #[serde(default)]
    pub id: String,
    /// Adapter implementation kind.
    #[serde(default)]
    pub kind: String,
    /// Adapter executable path or command name.
    #[serde(default)]
    pub command: String,
    /// Arguments passed to the adapter process.
    #[serde(default)]
    pub args: Vec<String>,
    /// Number of configured environment entries without exposing values.
    #[serde(default)]
    pub env_count: usize,
    /// Whether a custom working directory is configured without exposing its path.
    #[serde(default)]
    pub has_working_dir: bool,
}

#[cfg(test)]
mod tests {
    use super::{TextAdapterState, TextAdapterSummary};

    #[test]
    fn state_derives_adapter_ids_from_summaries() {
        let state = TextAdapterState::from_adapters(vec![TextAdapterSummary {
            id: "cmd".to_owned(),
            kind: "command".to_owned(),
            command: "helper".to_owned(),
            args: vec!["--json".to_owned()],
            env_count: 2,
            has_working_dir: true,
        }]);

        assert_eq!(state.adapter_count, 1);
        assert_eq!(state.adapter_ids, ["cmd"]);
        assert_eq!(state.single_adapter_id.as_deref(), Some("cmd"));
        assert_eq!(state.adapters[0].command, "helper");
        assert_eq!(state.adapters[0].env_count, 2);
    }

    #[test]
    fn missing_fields_default_for_legacy_tolerance() {
        let state: TextAdapterState = serde_json::from_str(r#"{"adapter_count":0}"#).unwrap();
        assert_eq!(state.adapter_count, 0);
        assert!(state.adapter_ids.is_empty());
        assert!(state.adapters.is_empty());
        assert!(state.single_adapter_id.is_none());
    }
}
