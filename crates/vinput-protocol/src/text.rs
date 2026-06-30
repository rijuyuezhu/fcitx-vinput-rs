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
    /// Number of configured adapter arguments without exposing values.
    #[serde(default)]
    pub args_count: usize,
    /// Number of configured environment entries without exposing keys or values.
    #[serde(default)]
    pub env_count: usize,
    /// Whether the daemon currently supervises this adapter as running.
    #[serde(default)]
    pub is_running: bool,
    /// Supervised process id when the adapter is running.
    #[serde(default)]
    pub pid: Option<u32>,
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
            args_count: 1,
            env_count: 2,
            is_running: true,
            pid: Some(1234),
            has_working_dir: true,
        }]);

        assert_eq!(state.adapter_count, 1);
        assert_eq!(state.adapter_ids, ["cmd"]);
        assert_eq!(state.single_adapter_id.as_deref(), Some("cmd"));
        assert_eq!(state.adapters[0].args_count, 1);
        assert_eq!(state.adapters[0].env_count, 2);
        assert!(state.adapters[0].is_running);
        assert_eq!(state.adapters[0].pid, Some(1234));
        let ambiguous = TextAdapterState::from_adapters(vec![
            TextAdapterSummary {
                id: "first".to_owned(),
                ..TextAdapterSummary::default()
            },
            TextAdapterSummary {
                id: "second".to_owned(),
                ..TextAdapterSummary::default()
            },
        ]);
        assert_eq!(ambiguous.adapter_count, 2);
        assert_eq!(ambiguous.adapter_ids, ["first", "second"]);
        assert!(ambiguous.single_adapter_id.is_none());
    }

    #[test]
    fn adapter_summary_serializes_sanitized_shape() {
        let summary = TextAdapterSummary {
            id: "cmd".to_owned(),
            kind: "command".to_owned(),
            args_count: 2,
            env_count: 1,
            is_running: false,
            pid: None,
            has_working_dir: true,
        };

        let value = serde_json::to_value(summary).unwrap();
        assert_eq!(value["id"], "cmd");
        assert_eq!(value["kind"], "command");
        assert_eq!(value["args_count"], 2);
        assert_eq!(value["env_count"], 1);
        assert_eq!(value["has_working_dir"], true);

        let json = serde_json::to_string(&value).unwrap();
        for forbidden_key in ["command", "args", "env", "working_dir"] {
            assert!(
                !json.contains(&format!("\"{forbidden_key}\":")),
                "text adapter summary must not expose {forbidden_key}"
            );
        }
    }

    #[test]
    fn adapter_summary_schema_uses_sanitized_shape() {
        let schema = schemars::schema_for!(TextAdapterSummary);
        let json = serde_json::to_string(&schema).unwrap();

        assert!(json.contains("args_count"));
        assert!(!json.contains("\"command\""));
        assert!(!json.contains("\"args\""));
        assert!(!json.contains("\"working_dir\""));
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
