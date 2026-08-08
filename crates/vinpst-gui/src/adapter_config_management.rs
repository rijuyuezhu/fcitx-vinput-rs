//! Typed text-adapter configuration state and persistence for the LLM page.

mod view;

use std::{collections::HashMap, fmt};

use iced::Task;
use vinpst_config::{LlmAdapterConfig, VinpstConfig};
use vinpst_text::validate_adapter_id;

use crate::{
    App, ConfigSaveOutcome, GuiText, Message, OperationState, SecretInput, load_config_document,
    save_updated_config_with_daemon, script_management::managed_adapter_script_path,
};

/// One editable text-adapter field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterConfigEditorField {
    /// Stable adapter id for a new custom adapter.
    Id,
    /// Executable path or command name.
    Command,
    /// JSON array of command arguments.
    Args,
    /// JSON object of environment variables.
    Environment,
    /// Optional working directory.
    WorkingDirectory,
}

/// One text-adapter configuration interaction.
#[derive(Clone)]
pub enum AdapterConfigMessage {
    /// Open an empty form for a custom adapter.
    BeginAdd,
    /// Open one configured adapter for editing.
    BeginEdit(String),
    /// Update one editor field with a redacted value wrapper.
    EditorChanged {
        /// Typed field being edited.
        field: AdapterConfigEditorField,
        /// User-entered value excluded from generic Debug output.
        value: SecretInput,
    },
    /// Restore the loaded adapter values.
    ResetEdit,
    /// Close the editor without saving.
    CancelEdit,
    /// Validate and persist the adapter form.
    Save,
    /// Remove one user-defined adapter from configuration only.
    Remove(String),
    /// Result of one asynchronous adapter config mutation.
    MutationFinished(Result<AdapterConfigMutationOutcome, String>),
    /// Result of one asynchronous user-defined adapter removal.
    RemovalFinished(Result<AdapterConfigRemovalOutcome, String>),
}

impl fmt::Debug for AdapterConfigMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeginAdd => formatter.write_str("BeginAdd"),
            Self::BeginEdit(id) => formatter.debug_tuple("BeginEdit").field(id).finish(),
            Self::EditorChanged { field, .. } => formatter
                .debug_struct("EditorChanged")
                .field("field", field)
                .field("value", &"<redacted>")
                .finish(),
            Self::ResetEdit => formatter.write_str("ResetEdit"),
            Self::CancelEdit => formatter.write_str("CancelEdit"),
            Self::Save => formatter.write_str("Save"),
            Self::Remove(id) => formatter.debug_tuple("Remove").field(id).finish(),
            Self::MutationFinished(Ok(outcome)) => formatter
                .debug_struct("MutationFinished")
                .field("adapter_id", &outcome.adapter_id)
                .field("status", &"saved")
                .finish(),
            Self::MutationFinished(Err(_)) => formatter
                .debug_struct("MutationFinished")
                .field("status", &"failed")
                .finish(),
            Self::RemovalFinished(Ok(outcome)) => formatter
                .debug_struct("RemovalFinished")
                .field("adapter_id", &outcome.adapter_id)
                .field("status", &"removed")
                .finish(),
            Self::RemovalFinished(Err(_)) => formatter
                .debug_struct("RemovalFinished")
                .field("status", &"failed")
                .finish(),
        }
    }
}

/// Result of one persisted text-adapter configuration mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterConfigMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable adapter id that was updated.
    pub adapter_id: String,
    /// Whether the operation created a new custom adapter.
    pub created: bool,
}

/// Result of removing one user-defined text adapter from configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterConfigRemovalOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable adapter id removed from configuration.
    pub adapter_id: String,
}

#[derive(Clone, PartialEq, Eq)]
struct AdapterConfigEditorFields {
    id: String,
    command: SecretInput,
    args: SecretInput,
    environment: SecretInput,
    working_directory: SecretInput,
}

impl fmt::Debug for AdapterConfigEditorFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdapterConfigEditorFields")
            .field("command", &"<redacted command>")
            .field("args", &"<redacted arguments>")
            .field("environment", &"<redacted environment>")
            .field("working_directory", &"<redacted working directory>")
            .finish()
    }
}

/// Active text-adapter editor state.
#[derive(Clone, PartialEq)]
pub(super) struct AdapterConfigEditorState {
    original: Option<LlmAdapterConfig>,
    baseline: AdapterConfigEditorFields,
    fields: AdapterConfigEditorFields,
}

impl fmt::Debug for AdapterConfigEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdapterConfigEditorState")
            .field("adapter_id", &self.fields.id)
            .field(
                "mode",
                &if self.original.is_some() {
                    "edit"
                } else {
                    "add"
                },
            )
            .field("baseline", &self.baseline)
            .field("fields", &self.fields)
            .finish()
    }
}

impl AdapterConfigEditorState {
    fn add() -> Self {
        let fields = AdapterConfigEditorFields {
            id: String::new(),
            command: SecretInput::new(String::new()),
            args: SecretInput::new("[]".to_owned()),
            environment: SecretInput::new("{}".to_owned()),
            working_directory: SecretInput::new(String::new()),
        };
        Self {
            original: None,
            baseline: fields.clone(),
            fields,
        }
    }

    fn edit(adapter: &LlmAdapterConfig) -> Self {
        let fields = AdapterConfigEditorFields {
            id: adapter.id.clone(),
            command: SecretInput::new(adapter.command.clone()),
            args: SecretInput::new(
                serde_json::to_string_pretty(&adapter.args).unwrap_or_else(|_| "[]".to_owned()),
            ),
            environment: SecretInput::new(
                serde_json::to_string_pretty(&adapter.env).unwrap_or_else(|_| "{}".to_owned()),
            ),
            working_directory: SecretInput::new(adapter.working_dir.clone().unwrap_or_default()),
        };
        Self {
            original: Some(adapter.clone()),
            baseline: fields.clone(),
            fields,
        }
    }

    fn update(&mut self, field: AdapterConfigEditorField, value: SecretInput) {
        match field {
            AdapterConfigEditorField::Id if self.original.is_none() => {
                self.fields.id = value.into_inner();
            }
            AdapterConfigEditorField::Id => {}
            AdapterConfigEditorField::Command => self.fields.command = value,
            AdapterConfigEditorField::Args => self.fields.args = value,
            AdapterConfigEditorField::Environment => self.fields.environment = value,
            AdapterConfigEditorField::WorkingDirectory => self.fields.working_directory = value,
        }
    }

    fn reset(&mut self) {
        self.fields = self.baseline.clone();
    }

    fn is_dirty(&self) -> bool {
        self.fields != self.baseline
    }

    fn adapter(&self) -> Result<LlmAdapterConfig, String> {
        let id = self
            .original
            .as_ref()
            .map_or_else(|| self.fields.id.trim(), |adapter| adapter.id.as_str());
        if id.is_empty() {
            return Err("Text adapter id cannot be empty.".to_owned());
        }
        validate_adapter_id(id).map_err(|_| {
            format!("Text adapter id `{id}` cannot be used for daemon runtime files.")
        })?;
        let command = self.fields.command.as_str().trim();
        if command.is_empty() {
            return Err("Text adapter command cannot be empty.".to_owned());
        }
        let mut adapter = self.original.clone().unwrap_or_else(|| LlmAdapterConfig {
            id: id.to_owned(),
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
            extra: HashMap::new(),
        });
        id.clone_into(&mut adapter.id);
        command.clone_into(&mut adapter.command);
        adapter.args = parse_string_array(self.fields.args.as_str())?;
        adapter.env = parse_string_map(self.fields.environment.as_str())?;
        adapter.working_dir = optional_trimmed(self.fields.working_directory.as_str());
        Ok(adapter)
    }
}

impl App {
    pub(super) fn intercept_adapter_config_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        if let Message::AdapterConfig(message) = message {
            if self.is_busy()
                && !matches!(
                    message,
                    AdapterConfigMessage::MutationFinished(_)
                        | AdapterConfigMessage::RemovalFinished(_)
                )
            {
                return Some(Task::none());
            }
            return Some(self.handle_adapter_config_message(message.clone()));
        }
        let Some(editor) = &self.adapter_config_editor else {
            return None;
        };
        let blocks_open_editor = matches!(
            message,
            Message::ReloadConfig
                | Message::SaveConfig
                | Message::InstallRegistryModel(_)
                | Message::RetryModelInstall
                | Message::RequestRemoveInstalledModel(_)
                | Message::RemoveInstalledModel(_)
                | Message::RequestRemoveAsrProvider { .. }
                | Message::RequestRemoveTextAdapter { .. }
                | Message::RequestRemoveLlmProvider(_)
                | Message::RequestRemoveScene(_)
                | Message::Scene(_)
                | Message::AsrProvider(_)
                | Message::LlmProvider(_)
                | Message::Hotword(_)
                | Message::AdapterRuntime(_)
                | Message::InstallProvider(_)
                | Message::InstallAdapter(_)
                | Message::ConfirmScriptInstall
                | Message::RetryScriptInstall
                | Message::RetryScriptConfigUpdate
                | Message::EditProviderScript(_)
                | Message::RemoveProvider(_)
                | Message::RemoveAdapter(_)
        ) || matches!(message, Message::SelectPage(page) if *page != crate::Page::Llm && editor.is_dirty());
        if blocks_open_editor {
            self.operation = OperationState::Failed(
                "Save or cancel the open text-adapter form before continuing.".to_owned(),
            );
            return Some(Task::none());
        }
        None
    }

    fn handle_adapter_config_message(&mut self, message: AdapterConfigMessage) -> Task<Message> {
        match message {
            AdapterConfigMessage::BeginAdd => {
                self.begin_add_adapter_config();
                Task::none()
            }
            AdapterConfigMessage::BeginEdit(adapter_id) => {
                self.begin_edit_adapter_config(&adapter_id);
                Task::none()
            }
            AdapterConfigMessage::EditorChanged { field, value } => {
                if let Some(editor) = &mut self.adapter_config_editor {
                    editor.update(field, value);
                }
                Task::none()
            }
            AdapterConfigMessage::ResetEdit => {
                if let Some(editor) = &mut self.adapter_config_editor {
                    editor.reset();
                }
                self.operation = OperationState::Idle;
                Task::none()
            }
            AdapterConfigMessage::CancelEdit => {
                self.adapter_config_editor = None;
                self.operation = OperationState::Idle;
                Task::none()
            }
            AdapterConfigMessage::Save => self.begin_adapter_config_save(),
            AdapterConfigMessage::Remove(adapter_id) => {
                self.begin_custom_adapter_removal(&adapter_id)
            }
            AdapterConfigMessage::MutationFinished(result) => {
                self.finish_adapter_config_mutation(result)
            }
            AdapterConfigMessage::RemovalFinished(result) => {
                self.finish_custom_adapter_removal(result)
            }
        }
    }

    fn begin_add_adapter_config(&mut self) {
        if self.adapter_config_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_adapter_config_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("adding a text adapter") {
            return;
        }
        self.adapter_config_editor = Some(AdapterConfigEditorState::add());
        self.operation = OperationState::Idle;
    }

    fn begin_edit_adapter_config(&mut self, adapter_id: &str) {
        if self.adapter_config_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_adapter_config_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("editing a text adapter") {
            return;
        }
        let Some(adapter) = self
            .config
            .as_ref()
            .ok()
            .and_then(|document| {
                document
                    .config
                    .llm
                    .adapters
                    .iter()
                    .find(|adapter| adapter.id == adapter_id)
            })
            .cloned()
        else {
            self.operation = OperationState::Failed(format!(
                "Text adapter `{adapter_id}` is no longer configured."
            ));
            return;
        };
        self.adapter_config_editor = Some(AdapterConfigEditorState::edit(&adapter));
        self.operation = OperationState::Idle;
    }

    fn begin_adapter_config_save(&mut self) -> Task<Message> {
        let Some(editor) = self.adapter_config_editor.clone() else {
            return Task::none();
        };
        if !editor.is_dirty() {
            return Task::none();
        }
        if let Err(error) = self.ensure_adapter_config_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("saving a text adapter") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let updated = match upsert_adapter_config(&document.config, &editor) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.operation = OperationState::Running(self.locale.text(GuiText::SavingTextAdapter));
        let document = document.clone();
        let adapter_id = editor.fields.id.trim().to_owned();
        let created = editor.original.is_none();
        crate::blocking_task::perform(
            "vinpst-gui-adapter-config-mutation",
            move || {
                save_updated_config_with_daemon(&document, &updated).map(|save| {
                    AdapterConfigMutationOutcome {
                        save,
                        adapter_id,
                        created,
                    }
                })
            },
            |result| {
                Message::AdapterConfig(AdapterConfigMessage::MutationFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
        )
    }

    fn finish_adapter_config_mutation(
        &mut self,
        result: Result<AdapterConfigMutationOutcome, String>,
    ) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let summary = self
            .locale
            .text_adapter_changed(outcome.created, &outcome.adapter_id);
        let path = outcome.save.path.display().to_string();
        let backup = outcome
            .save
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string());
        self.replace_config(load_config_document(Some(&outcome.save.path)));
        self.operation = OperationState::Succeeded(self.locale.save_receipt(
            &summary,
            &path,
            backup.as_deref(),
            &outcome.save.daemon_reload,
        ));
        self.begin_daemon_refresh(false)
    }

    fn begin_custom_adapter_removal(&mut self, adapter_id: &str) -> Task<Message> {
        if self.adapter_config_editor.is_some() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SaveOrCancelAdapterBeforeRemoval)
                    .to_owned(),
            );
            return Task::none();
        }
        if let Err(error) = self.ensure_adapter_config_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("removing a text adapter") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let updated = match remove_custom_adapter_config(&document.config, adapter_id) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.operation = OperationState::Running(self.locale.text(GuiText::RemovingTextAdapter));
        let document = document.clone();
        let adapter_id = adapter_id.to_owned();
        crate::blocking_task::perform(
            "vinpst-gui-adapter-config-remove",
            move || {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| AdapterConfigRemovalOutcome { save, adapter_id })
            },
            |result| {
                Message::AdapterConfig(AdapterConfigMessage::RemovalFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
        )
    }

    fn finish_custom_adapter_removal(
        &mut self,
        result: Result<AdapterConfigRemovalOutcome, String>,
    ) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let summary = self.locale.text_adapter_removed(&outcome.adapter_id);
        let path = outcome.save.path.display().to_string();
        let backup = outcome
            .save
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string());
        self.replace_config(load_config_document(Some(&outcome.save.path)));
        self.operation = OperationState::Succeeded(self.locale.save_receipt(
            &summary,
            &path,
            backup.as_deref(),
            &outcome.save.daemon_reload,
        ));
        self.begin_daemon_refresh(false)
    }

    fn ensure_adapter_config_editor_allowed(&self) -> Result<(), String> {
        self.ensure_no_unsaved_config_draft()?;
        self.ensure_no_open_scene_editor()?;
        self.ensure_no_open_asr_provider_editor()?;
        self.ensure_no_open_llm_provider_editor()?;
        Ok(())
    }
}

fn upsert_adapter_config(
    config: &VinpstConfig,
    editor: &AdapterConfigEditorState,
) -> Result<VinpstConfig, String> {
    let adapter = editor.adapter()?;
    let mut updated = config.clone();
    let Some(original) = &editor.original else {
        if updated
            .llm
            .adapters
            .iter()
            .any(|candidate| candidate.id == adapter.id)
        {
            return Err(format!(
                "Text adapter `{}` is already configured.",
                adapter.id
            ));
        }
        updated.llm.adapters.push(adapter);
        updated
            .validate()
            .map_err(|error| format!("Validate new text-adapter config: {error}"))?;
        return Ok(updated);
    };
    let Some(index) = updated
        .llm
        .adapters
        .iter()
        .position(|candidate| candidate.id == original.id)
    else {
        return Err(format!(
            "Text adapter `{}` is no longer configured.",
            original.id
        ));
    };
    if updated.llm.adapters[index] != *original {
        return Err(format!(
            "Text adapter `{}` changed after the form was opened; reopen it before saving.",
            original.id
        ));
    }
    updated.llm.adapters[index] = adapter;
    updated
        .validate()
        .map_err(|error| format!("Validate updated text-adapter config: {error}"))?;
    Ok(updated)
}

fn remove_custom_adapter_config(
    config: &VinpstConfig,
    adapter_id: &str,
) -> Result<VinpstConfig, String> {
    remove_custom_adapter_config_with(config, adapter_id, |adapter| {
        managed_adapter_script_path(adapter).is_some()
    })
}

fn remove_custom_adapter_config_with(
    config: &VinpstConfig,
    adapter_id: &str,
    is_managed: impl FnOnce(&LlmAdapterConfig) -> bool,
) -> Result<VinpstConfig, String> {
    let mut updated = config.clone();
    let index = updated
        .llm
        .adapters
        .iter()
        .position(|adapter| adapter.id == adapter_id)
        .ok_or_else(|| format!("Text adapter `{adapter_id}` is not configured."))?;
    if is_managed(&updated.llm.adapters[index]) {
        return Err(format!(
            "Text adapter `{adapter_id}` is registry-managed and must use managed removal."
        ));
    }
    updated.llm.adapters.remove(index);
    updated
        .validate()
        .map_err(|error| format!("Validate configuration after removing {adapter_id}: {error}"))?;
    Ok(updated)
}

fn parse_string_array(value: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(value)
        .map_err(|error| format!("Parse adapter arguments as a JSON string array: {error}"))
}

fn parse_string_map(value: &str) -> Result<HashMap<String, String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(HashMap::new());
    }
    serde_json::from_str::<HashMap<String, String>>(value)
        .map_err(|error| format!("Parse adapter environment as a JSON string object: {error}"))
}

fn optional_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn adapter() -> LlmAdapterConfig {
        LlmAdapterConfig {
            id: "adapter-a".to_owned(),
            command: "/usr/bin/adapter".to_owned(),
            args: vec!["--json".to_owned()],
            env: HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            working_dir: Some("/srv/adapter".to_owned()),
            extra: HashMap::from([("future".to_owned(), json!({"enabled": true}))]),
        }
    }

    #[test]
    fn editor_preserves_identity_and_extra_while_updating_typed_fields() {
        let original = adapter();
        let mut editor = AdapterConfigEditorState::edit(&original);
        editor.update(
            AdapterConfigEditorField::Command,
            SecretInput::new(" /opt/adapter ".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::Args,
            SecretInput::new("[\"--stream\",\"--lang=en\"]".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::Environment,
            SecretInput::new("{\"API_KEY\":\"value\"}".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::WorkingDirectory,
            SecretInput::new(" /opt/state ".to_owned()),
        );

        let updated = editor.adapter().expect("adapter should validate");
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.extra, original.extra);
        assert_eq!(updated.command, "/opt/adapter");
        assert_eq!(updated.args, ["--stream", "--lang=en"]);
        assert_eq!(
            updated.env.get("API_KEY").map(String::as_str),
            Some("value")
        );
        assert_eq!(updated.working_dir.as_deref(), Some("/opt/state"));
    }

    #[test]
    fn editor_rejects_empty_command_and_invalid_json_collections() {
        let mut editor = AdapterConfigEditorState::edit(&adapter());
        editor.update(
            AdapterConfigEditorField::Command,
            SecretInput::new("  ".to_owned()),
        );
        assert!(editor.adapter().is_err());
        assert!(parse_string_array("{\"not\":\"array\"}").is_err());
        assert!(parse_string_map("[\"not-object\"]").is_err());
    }

    #[test]
    fn editor_debug_and_messages_redact_process_configuration() {
        let editor = AdapterConfigEditorState::edit(&adapter());
        let debug = format!("{editor:?}");
        assert!(!debug.contains("/usr/bin/adapter"));
        assert!(!debug.contains("--json"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("/srv/adapter"));

        let message = AdapterConfigMessage::EditorChanged {
            field: AdapterConfigEditorField::Environment,
            value: SecretInput::new("message-secret".to_owned()),
        };
        assert!(!format!("{message:?}").contains("message-secret"));
    }

    #[test]
    fn edit_rejects_stale_adapter_without_pinning_error_prose() {
        let original = adapter();
        let editor = AdapterConfigEditorState::edit(&original);
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.adapters = vec![original.clone()];
        let updated = upsert_adapter_config(&config, &editor).expect("current adapter is valid");
        assert_eq!(
            updated.llm.adapters.as_slice(),
            std::slice::from_ref(&original)
        );

        config.llm.adapters[0].command = "/external/change".to_owned();
        assert!(upsert_adapter_config(&config, &editor).is_err());
    }

    #[test]
    fn add_builds_trimmed_adapter_and_rejects_duplicate_id() {
        let mut editor = AdapterConfigEditorState::add();
        editor.update(
            AdapterConfigEditorField::Id,
            SecretInput::new(" custom-adapter ".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::Command,
            SecretInput::new(" /opt/custom-adapter ".to_owned()),
        );
        editor.update(
            AdapterConfigEditorField::Args,
            SecretInput::new("[\"--json\"]".to_owned()),
        );

        let config = VinpstConfig::bundled_default().expect("bundled config");
        let updated = upsert_adapter_config(&config, &editor).expect("add custom adapter");
        let added = updated
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == "custom-adapter")
            .expect("custom adapter should be added");
        assert_eq!(added.command, "/opt/custom-adapter");
        assert_eq!(added.args, ["--json"]);
        assert!(added.extra.is_empty());

        assert!(upsert_adapter_config(&updated, &editor).is_err());
    }

    #[test]
    fn add_rejects_adapter_ids_unsafe_for_daemon_runtime_paths() {
        for adapter_id in [".", "..", "nested/id", r"nested\id"] {
            let mut editor = AdapterConfigEditorState::add();
            editor.update(
                AdapterConfigEditorField::Id,
                SecretInput::new(adapter_id.to_owned()),
            );
            editor.update(
                AdapterConfigEditorField::Command,
                SecretInput::new("/opt/custom-adapter".to_owned()),
            );
            assert!(editor.adapter().is_err());
        }
    }

    #[test]
    fn custom_adapter_removal_is_config_only_and_rejects_managed_entries() {
        let original = adapter();
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.adapters = vec![original.clone()];

        let updated = remove_custom_adapter_config_with(&config, &original.id, |_| false)
            .expect("custom adapter should be removed from config");
        assert!(updated.llm.adapters.is_empty());

        assert!(remove_custom_adapter_config_with(&config, &original.id, |_| true).is_err());
        assert!(remove_custom_adapter_config_with(&config, "missing", |_| false).is_err());
    }

    #[test]
    fn edit_mode_ignores_id_messages_and_preserves_identity() {
        let original = adapter();
        let mut editor = AdapterConfigEditorState::edit(&original);
        editor.update(
            AdapterConfigEditorField::Id,
            SecretInput::new("renamed-adapter".to_owned()),
        );
        assert_eq!(editor.adapter().expect("valid edit").id, original.id);
        assert_eq!(editor.fields.id, original.id);
    }

    #[test]
    fn dirty_adapter_form_blocks_runtime_control_and_cross_page_navigation() {
        let mut app = crate::test_support::GuiHarness::new();
        let original = adapter();
        app.config
            .as_mut()
            .expect("bundled config should load")
            .config
            .llm
            .adapters = vec![original.clone()];
        app.page = crate::Page::Llm;
        app.adapter_config_editor = Some(AdapterConfigEditorState::edit(&original));
        app.adapter_config_editor
            .as_mut()
            .expect("editor should remain open")
            .update(
                AdapterConfigEditorField::Command,
                SecretInput::new("/opt/changed-adapter".to_owned()),
            );

        let _ = app.update(Message::AdapterRuntime(
            crate::AdapterRuntimeMessage::Start(original.id.clone()),
        ));
        assert!(app.adapter_config_editor.is_some());
        assert!(matches!(app.operation, OperationState::Failed(_)));

        let _ = app.update(Message::SelectPage(crate::Page::Control));
        assert_eq!(app.page, crate::Page::Llm);
        assert!(app.adapter_config_editor.is_some());
    }
}
