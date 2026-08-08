//! Runtime diagnostic state builders for ASR and text adapters.

use vinpst_asr::AsrBackendFactory;
use vinpst_config::{LlmAdapterConfig, VinpstConfig};
use vinpst_protocol::{AsrBackendState, TextAdapterState, TextAdapterSummary, dbus};
use vinpst_text::AdapterRegistry;

use super::RuntimeState;

fn text_adapter_summary(adapter: &LlmAdapterConfig, pid: Option<u32>) -> TextAdapterSummary {
    TextAdapterSummary {
        id: adapter.id.clone(),
        kind: "command".to_owned(),
        args_count: adapter.args.len(),
        env_count: adapter.env.len(),
        is_running: pid.is_some(),
        pid,
        has_working_dir: adapter.working_dir.is_some(),
    }
}

impl RuntimeState {
    /// Builds a diagnostic ASR state from config without constructing a runtime.
    #[must_use]
    pub fn configured_asr_state(config: &VinpstConfig) -> AsrBackendState {
        AsrBackendFactory::state_for_config(&config.asr)
    }

    /// Builds a diagnostic ASR state from this runtime's current config.
    #[must_use]
    pub fn configured_asr_state_for_runtime(&self) -> AsrBackendState {
        let mut state = Self::configured_asr_state(&self.config);
        state.reload_in_progress = self.pending_asr_reload.is_some() || self.asr_reload_preparing;
        state.last_error = self
            .asr_reload_last_error
            .clone()
            .unwrap_or(state.last_error);
        state
    }

    /// Builds a text adapter registry from this runtime's current config.
    #[must_use]
    pub fn configured_text_adapters(&self) -> AdapterRegistry {
        AdapterRegistry::from_configs(&self.config.llm.adapters)
    }

    /// Builds sanitized text adapter diagnostics from config without constructing a runtime.
    #[must_use]
    pub fn configured_text_adapter_state(config: &VinpstConfig) -> TextAdapterState {
        TextAdapterState::from_adapters(
            config
                .llm
                .adapters
                .iter()
                .map(|adapter| text_adapter_summary(adapter, None))
                .collect(),
        )
    }

    /// Builds sanitized text adapter diagnostics from this runtime's current config.
    #[must_use]
    pub fn configured_text_adapter_state_for_runtime(&self) -> TextAdapterState {
        TextAdapterState::from_adapters(
            self.config
                .llm
                .adapters
                .iter()
                .map(|adapter| {
                    let pid = self
                        .adapter_processes
                        .get(&adapter.id)
                        .map(|process| process.pid);
                    text_adapter_summary(adapter, pid)
                })
                .collect(),
        )
    }

    /// Returns the only configured command text adapter id, if exactly one exists.
    #[must_use]
    pub fn single_configured_text_adapter_id(&self) -> Option<String> {
        self.configured_text_adapter_state_for_runtime()
            .single_adapter_id
    }

    /// Returns the supervised process id for a currently managed text adapter.
    #[must_use]
    pub fn text_adapter_pid(&self, adapter_id: &str) -> Option<u32> {
        self.adapter_processes
            .get(adapter_id)
            .map(|process| process.pid)
    }

    /// Returns whether a text adapter is currently supervised by this runtime.
    #[must_use]
    pub fn is_text_adapter_running(&self, adapter_id: &str) -> bool {
        self.text_adapter_pid(adapter_id).is_some()
    }

    /// Builds a sanitized runtime status snapshot for CLI and D-Bus diagnostics.
    #[must_use]
    pub fn runtime_status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            "status": self.status,
            "uptime_ms": self.uptime().as_millis(),
            "active_session": self.active_session.is_some(),
            "current_scene": self.current_scene,
            "selected_text_present": self.selected_text.is_some(),
            "partial_text": self.partial_text(),
            "capture_device": self.config.global.capture_device,
            "dbus": {
                "service": dbus::SERVICE_BUS_NAME,
                "object_path": dbus::SERVICE_OBJECT_PATH,
                "interface": dbus::SERVICE_INTERFACE,
            },
            "config": self.config.summary(),
            "asr": self.asr_backend_state(),
            "text_adapters": self.configured_text_adapter_state_for_runtime(),
        })
    }

    /// Returns an ASR backend state derived from config and backend descriptor.
    #[must_use]
    pub fn asr_backend_state(&self) -> AsrBackendState {
        let descriptor = self.asr_backend.describe();
        let configured = Self::configured_asr_state(&self.config);
        if descriptor.provider_id.is_empty() {
            let mut state = AsrBackendState::unavailable(
                configured.target_provider_id,
                configured.target_model_id,
                self.asr_disabled_reason
                    .clone()
                    .or_else(|| self.asr_reload_last_error.clone())
                    .unwrap_or(configured.last_error),
            );
            state.remote_endpoints = configured.remote_endpoints;
            state.reload_in_progress =
                self.pending_asr_reload.is_some() || self.asr_reload_preparing;
            return state;
        }
        let mut state = AsrBackendState::ready(descriptor.provider_id, descriptor.model_id);
        state.target_provider_id = configured.target_provider_id;
        state.target_model_id = configured.target_model_id;
        state.remote_endpoints = configured.remote_endpoints;
        state.reload_in_progress = self.pending_asr_reload.is_some() || self.asr_reload_preparing;
        state.last_error = self.asr_reload_last_error.clone().unwrap_or_default();
        state
    }
}
