//! Typed ASR provider editor state, validation, and persistence.

mod view;

use std::{collections::HashMap, fmt};

use iced::Task;
use vinpst_config::{AsrProviderConfig, AsrProviderKind, VinpstConfig, redact_url_for_diagnostics};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, GuiText, Message, OperationState, SecretInput,
    load_config_document, save_updated_config_with_daemon,
    script_management::managed_provider_script_path,
};

/// One editable field in the ASR provider form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrProviderEditorField {
    /// Stable id for a new custom provider.
    Id,
    /// Optional provider timeout in milliseconds.
    TimeoutMs,
    /// Optional provider model identifier.
    Model,
    /// Command executable for command providers.
    Command,
    /// JSON array of command arguments.
    Args,
    /// Endpoint for remote providers.
    Endpoint,
}

/// One ASR provider lifecycle interaction handled by the Resources page.
#[derive(Debug, Clone)]
pub enum AsrProviderMessage {
    /// Open an empty custom provider form.
    BeginAdd,
    /// Open one configured provider for editing.
    BeginEdit(String),
    /// Select the provider kind while creating a custom entry.
    KindChanged(AsrProviderKind),
    /// Update one field without exposing entered values through `Debug`.
    EditorChanged {
        /// Typed field being edited.
        field: AsrProviderEditorField,
        /// Redacted user-entered value.
        value: SecretInput,
    },
    /// Update one visible environment-variable key.
    EnvironmentKeyChanged {
        /// Stable row index in the current form.
        index: usize,
        /// Visible environment-variable key.
        key: String,
    },
    /// Update one redacted environment-variable value.
    EnvironmentValueChanged {
        /// Stable row index in the current form.
        index: usize,
        /// Secret environment-variable value.
        value: SecretInput,
    },
    /// Append one empty environment-variable row.
    AddEnvironment,
    /// Remove one environment-variable row.
    RemoveEnvironment(usize),
    /// Restore the form to its initially loaded values.
    ResetEdit,
    /// Close the provider form without saving.
    CancelEdit,
    /// Validate and persist the provider form.
    Save,
    /// Remove one inactive user-defined provider from configuration only.
    Remove(String),
    /// Result of one asynchronous provider mutation.
    MutationFinished(Result<AsrProviderMutationOutcome, String>),
    /// Result of one asynchronous user-defined provider removal.
    RemovalFinished(Result<AsrProviderRemovalOutcome, String>),
}

/// Result of one persisted ASR provider mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrProviderMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable provider id that was updated.
    pub provider_id: String,
    /// Whether the mutation created a new custom provider.
    pub created: bool,
}

/// Result of removing one user-defined ASR provider from configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrProviderRemovalOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Stable provider id removed from configuration.
    pub provider_id: String,
}

#[derive(Clone, PartialEq, Eq)]
struct AsrProviderEnvironmentEntry {
    key: String,
    value: SecretInput,
}

impl fmt::Debug for AsrProviderEnvironmentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEnvironmentEntry")
            .field("key", &self.key)
            .field("value", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct AsrProviderEditorFields {
    id: String,
    timeout_ms: String,
    model: String,
    command: SecretInput,
    args: SecretInput,
    environment: Vec<AsrProviderEnvironmentEntry>,
    endpoint: SecretInput,
}

impl fmt::Debug for AsrProviderEditorFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEditorFields")
            .field("id", &self.id)
            .field("timeout_ms", &self.timeout_ms)
            .field("model", &self.model)
            .field("command", &"<redacted command>")
            .field("args", &"<redacted arguments>")
            .field("environment", &self.environment)
            .field(
                "endpoint",
                &redact_url_for_diagnostics(self.endpoint.as_str()),
            )
            .finish()
    }
}

/// Active ASR provider editor state.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct AsrProviderEditorState {
    original: Option<AsrProviderConfig>,
    kind: AsrProviderKind,
    baseline: AsrProviderEditorFields,
    fields: AsrProviderEditorFields,
    endpoint_secure: bool,
}

impl fmt::Debug for AsrProviderEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsrProviderEditorState")
            .field("provider_id", &self.fields.id)
            .field("provider_kind", &self.kind)
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
            .field("endpoint_secure", &self.endpoint_secure)
            .finish()
    }
}

impl AsrProviderEditorState {
    fn add() -> Self {
        let fields = AsrProviderEditorFields {
            id: String::new(),
            timeout_ms: String::new(),
            model: String::new(),
            command: SecretInput::new(String::new()),
            args: SecretInput::new("[]".to_owned()),
            environment: Vec::new(),
            endpoint: SecretInput::new(String::new()),
        };
        Self {
            original: None,
            kind: AsrProviderKind::Command,
            endpoint_secure: false,
            baseline: fields.clone(),
            fields,
        }
    }

    fn edit(provider: &AsrProviderConfig) -> Self {
        let fields = AsrProviderEditorFields {
            id: provider.id.clone(),
            timeout_ms: provider
                .timeout_ms
                .map_or_else(String::new, |value| value.to_string()),
            model: provider.model.clone().unwrap_or_default(),
            command: SecretInput::new(provider.command.clone().unwrap_or_default()),
            args: SecretInput::new(
                serde_json::to_string_pretty(&provider.args).unwrap_or_else(|_| "[]".to_owned()),
            ),
            environment: environment_entries(&provider.env),
            endpoint: SecretInput::new(provider.endpoint.clone().unwrap_or_default()),
        };
        Self {
            original: Some(provider.clone()),
            kind: provider.kind.clone(),
            endpoint_secure: endpoint_input_is_secure(fields.endpoint.as_str()),
            baseline: fields.clone(),
            fields,
        }
    }

    fn update(&mut self, field: AsrProviderEditorField, value: SecretInput) {
        let value = value.into_inner();
        match field {
            AsrProviderEditorField::Id if self.original.is_none() => self.fields.id = value,
            AsrProviderEditorField::Id => {}
            AsrProviderEditorField::TimeoutMs => self.fields.timeout_ms = value,
            AsrProviderEditorField::Model => self.fields.model = value,
            AsrProviderEditorField::Command => self.fields.command = SecretInput::new(value),
            AsrProviderEditorField::Args => self.fields.args = SecretInput::new(value),
            AsrProviderEditorField::Endpoint => {
                self.endpoint_secure |= endpoint_input_is_secure(&value);
                self.fields.endpoint = SecretInput::new(value);
            }
        }
    }

    fn update_environment_key(&mut self, index: usize, key: String) {
        if let Some(entry) = self.fields.environment.get_mut(index) {
            entry.key = key;
        }
    }

    fn update_environment_value(&mut self, index: usize, value: SecretInput) {
        if let Some(entry) = self.fields.environment.get_mut(index) {
            entry.value = value;
        }
    }

    fn add_environment(&mut self) {
        self.fields.environment.push(AsrProviderEnvironmentEntry {
            key: String::new(),
            value: SecretInput::new(String::new()),
        });
    }

    fn remove_environment(&mut self, index: usize) {
        if index < self.fields.environment.len() {
            self.fields.environment.remove(index);
        }
    }

    fn set_kind(&mut self, kind: AsrProviderKind) {
        if self.original.is_none() {
            self.kind = kind;
        }
    }

    fn reset(&mut self) {
        self.fields = self.baseline.clone();
        if let Some(original) = &self.original {
            self.kind = original.kind.clone();
        } else {
            self.kind = AsrProviderKind::Command;
        }
        self.endpoint_secure = endpoint_input_is_secure(self.baseline.endpoint.as_str());
    }

    fn is_dirty(&self) -> bool {
        let baseline_kind = self
            .original
            .as_ref()
            .map_or(AsrProviderKind::Command, |provider| provider.kind.clone());
        self.fields != self.baseline || self.kind != baseline_kind
    }

    fn provider(&self) -> Result<AsrProviderConfig, String> {
        let id = self
            .original
            .as_ref()
            .map_or_else(|| self.fields.id.trim(), |provider| provider.id.as_str());
        if id.is_empty() {
            return Err("ASR provider id cannot be empty.".to_owned());
        }
        let mut provider = self.original.clone().unwrap_or_else(|| AsrProviderConfig {
            id: id.to_owned(),
            kind: self.kind.clone(),
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            endpoint: None,
        });
        id.clone_into(&mut provider.id);
        provider.kind = self.kind.clone();
        provider.timeout_ms = parse_optional_timeout(&self.fields.timeout_ms)?;
        provider.model = optional_trimmed(&self.fields.model);

        match provider.kind {
            AsrProviderKind::Local => {
                provider.command = None;
                provider.args.clear();
                provider.env.clear();
                provider.endpoint = None;
            }
            AsrProviderKind::Command => {
                let command = self.fields.command.as_str().trim();
                if command.is_empty() {
                    return Err("Command ASR provider command cannot be empty.".to_owned());
                }
                provider.command = Some(command.to_owned());
                provider.args = parse_string_array(self.fields.args.as_str(), "arguments")?;
                provider.env = environment_map(&self.fields.environment)?;
                provider.endpoint = None;
            }
            AsrProviderKind::Remote => {
                let endpoint = self.fields.endpoint.as_str().trim();
                if endpoint.is_empty() {
                    return Err("Remote ASR provider endpoint cannot be empty.".to_owned());
                }
                provider.endpoint = Some(endpoint.to_owned());
                provider.command = None;
                provider.args.clear();
                provider.env.clear();
            }
        }
        Ok(provider)
    }
}

impl App {
    pub(super) fn begin_asr_provider_use(&mut self, provider_id: String) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        self.update_config_draft(crate::ConfigDraftMessage::ActiveProvider(provider_id));
        self.begin_config_save()
    }

    pub(super) fn intercept_asr_provider_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        if let Message::AsrProvider(message) = message {
            if self.is_busy()
                && !matches!(
                    message,
                    AsrProviderMessage::MutationFinished(_)
                        | AsrProviderMessage::RemovalFinished(_)
                )
            {
                return Some(Task::none());
            }
            return Some(self.handle_asr_provider_message(message.clone()));
        }
        let Some(editor) = &self.asr_provider_editor else {
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
                | Message::LlmProvider(_)
                | Message::Hotword(_)
                | Message::InstallProvider(_)
                | Message::InstallAdapter(_)
                | Message::ConfirmScriptInstall
                | Message::RetryScriptInstall
                | Message::RetryScriptConfigUpdate
                | Message::EditProviderScript(_)
                | Message::RemoveProvider(_)
                | Message::RemoveAdapter(_)
        ) || matches!(message, Message::SelectPage(page) if *page != crate::Page::Resources && editor.is_dirty());
        if blocks_open_editor {
            self.operation = OperationState::Failed(
                "Save or cancel the open ASR provider form before continuing.".to_owned(),
            );
            return Some(Task::none());
        }
        None
    }

    pub(super) fn handle_asr_provider_message(
        &mut self,
        message: AsrProviderMessage,
    ) -> Task<Message> {
        match message {
            AsrProviderMessage::BeginAdd => self.begin_add_asr_provider(),
            AsrProviderMessage::BeginEdit(id) => self.begin_edit_asr_provider(&id),
            AsrProviderMessage::KindChanged(kind) => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.set_kind(kind);
                }
            }
            AsrProviderMessage::EditorChanged { field, value } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update(field, value);
                }
            }
            AsrProviderMessage::EnvironmentKeyChanged { index, key } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update_environment_key(index, key);
                }
            }
            AsrProviderMessage::EnvironmentValueChanged { index, value } => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.update_environment_value(index, value);
                }
            }
            AsrProviderMessage::AddEnvironment => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.add_environment();
                }
            }
            AsrProviderMessage::RemoveEnvironment(index) => {
                if let Some(editor) = &mut self.asr_provider_editor {
                    editor.remove_environment(index);
                }
            }
            AsrProviderMessage::ResetEdit => {
                if !self.is_busy() {
                    if let Some(editor) = &mut self.asr_provider_editor {
                        editor.reset();
                    }
                    self.operation = OperationState::Idle;
                }
            }
            AsrProviderMessage::CancelEdit => {
                if !self.is_busy() {
                    self.asr_provider_editor = None;
                    self.operation = OperationState::Idle;
                }
            }
            AsrProviderMessage::Save => return self.begin_asr_provider_save(),
            AsrProviderMessage::Remove(provider_id) => {
                return self.begin_custom_asr_provider_removal(&provider_id);
            }
            AsrProviderMessage::MutationFinished(result) => {
                return self.finish_asr_provider_mutation(result);
            }
            AsrProviderMessage::RemovalFinished(result) => {
                return self.finish_custom_asr_provider_removal(result);
            }
        }
        Task::none()
    }

    fn begin_add_asr_provider(&mut self) {
        if self.is_busy() || self.asr_provider_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("adding an ASR provider") {
            return;
        }
        self.asr_provider_editor = Some(AsrProviderEditorState::add());
        self.operation = OperationState::Idle;
    }

    fn begin_edit_asr_provider(&mut self, provider_id: &str) {
        if self.is_busy() || self.asr_provider_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return;
        }
        if !self.guard_hotword_changes("editing an ASR provider") {
            return;
        }
        let Some(provider) = self
            .config
            .as_ref()
            .ok()
            .and_then(|document| {
                document
                    .config
                    .asr
                    .providers
                    .iter()
                    .find(|provider| provider.id == provider_id)
            })
            .cloned()
        else {
            self.operation = OperationState::Failed(format!(
                "ASR provider `{provider_id}` is no longer configured."
            ));
            return;
        };
        self.asr_provider_editor = Some(AsrProviderEditorState::edit(&provider));
        self.operation = OperationState::Idle;
    }

    fn begin_asr_provider_save(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(editor) = self.asr_provider_editor.clone() else {
            return Task::none();
        };
        if !editor.is_dirty() {
            return Task::none();
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("saving an ASR provider") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let updated = match upsert_asr_provider(&document.config, &editor) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let provider_id = editor.fields.id.trim().to_owned();
        let created = editor.original.is_none();
        self.begin_asr_provider_mutation(document.clone(), updated, provider_id, created)
    }

    fn ensure_asr_provider_editor_allowed(&self) -> Result<(), String> {
        self.ensure_no_unsaved_config_draft()?;
        self.ensure_no_open_scene_editor()?;
        self.ensure_no_open_llm_provider_editor()?;
        Ok(())
    }

    fn begin_asr_provider_mutation(
        &mut self,
        document: ConfigDocument,
        updated: VinpstConfig,
        provider_id: String,
        created: bool,
    ) -> Task<Message> {
        self.operation = OperationState::Running(self.locale.text(GuiText::SavingAsrProvider));
        crate::blocking_task::perform(
            "vinpst-gui-asr-provider-mutation",
            move || {
                save_updated_config_with_daemon(&document, &updated).map(|save| {
                    AsrProviderMutationOutcome {
                        save,
                        provider_id,
                        created,
                    }
                })
            },
            |result| {
                Message::AsrProvider(AsrProviderMessage::MutationFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
        )
    }

    fn finish_asr_provider_mutation(
        &mut self,
        result: Result<AsrProviderMutationOutcome, String>,
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
            .asr_provider_changed(outcome.created, &outcome.provider_id);
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

    fn begin_custom_asr_provider_removal(&mut self, provider_id: &str) -> Task<Message> {
        if self.asr_provider_editor.is_some() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::SaveOrCancelProviderBeforeRemoval)
                    .to_owned(),
            );
            return Task::none();
        }
        if let Err(error) = self.ensure_asr_provider_editor_allowed() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if !self.guard_hotword_changes("removing an ASR provider") {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let updated = match remove_custom_asr_provider_config(&document.config, provider_id) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.operation = OperationState::Running(self.locale.text(GuiText::RemovingAsrProvider));
        let document = document.clone();
        let provider_id = provider_id.to_owned();
        crate::blocking_task::perform(
            "vinpst-gui-asr-provider-remove",
            move || {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| AsrProviderRemovalOutcome { save, provider_id })
            },
            |result| {
                Message::AsrProvider(AsrProviderMessage::RemovalFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
        )
    }

    fn finish_custom_asr_provider_removal(
        &mut self,
        result: Result<AsrProviderRemovalOutcome, String>,
    ) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let summary = self.locale.asr_provider_removed(&outcome.provider_id);
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
}

fn upsert_asr_provider(
    config: &VinpstConfig,
    editor: &AsrProviderEditorState,
) -> Result<VinpstConfig, String> {
    let provider = editor.provider()?;
    let mut updated = config.clone();
    let Some(original) = &editor.original else {
        if updated
            .asr
            .providers
            .iter()
            .any(|candidate| candidate.id == provider.id)
        {
            return Err(format!(
                "ASR provider `{}` is already configured.",
                provider.id
            ));
        }
        updated.asr.providers.push(provider);
        updated
            .validate()
            .map_err(|error| format!("Validate new ASR provider config: {error}"))?;
        return Ok(updated);
    };
    let Some(index) = updated
        .asr
        .providers
        .iter()
        .position(|candidate| candidate.id == original.id)
    else {
        return Err(format!(
            "ASR provider `{}` is no longer configured.",
            original.id
        ));
    };
    if updated.asr.providers[index] != *original {
        return Err(format!(
            "ASR provider `{}` changed after the form was opened; reopen it before saving.",
            original.id
        ));
    }
    updated.asr.providers[index] = provider;
    updated
        .validate()
        .map_err(|error| format!("Validate updated ASR provider config: {error}"))?;
    Ok(updated)
}

fn remove_custom_asr_provider_config(
    config: &VinpstConfig,
    provider_id: &str,
) -> Result<VinpstConfig, String> {
    remove_custom_asr_provider_config_with(config, provider_id, |provider| {
        managed_provider_script_path(provider).is_some()
    })
}

fn remove_custom_asr_provider_config_with(
    config: &VinpstConfig,
    provider_id: &str,
    is_managed: impl FnOnce(&AsrProviderConfig) -> bool,
) -> Result<VinpstConfig, String> {
    if config.asr.active_provider == provider_id {
        return Err(format!(
            "Active ASR provider `{provider_id}` cannot be removed; select another provider first."
        ));
    }
    let mut updated = config.clone();
    let index = updated
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` is not configured."))?;
    if is_managed(&updated.asr.providers[index]) {
        return Err(format!(
            "ASR provider `{provider_id}` is registry-managed and must use managed removal."
        ));
    }
    updated.asr.providers.remove(index);
    updated
        .validate()
        .map_err(|error| format!("Validate configuration after removing {provider_id}: {error}"))?;
    Ok(updated)
}

fn parse_optional_timeout(value: &str) -> Result<Option<u64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let timeout = value.parse::<u64>().map_err(|_| {
        "ASR provider timeout must be a positive integer in milliseconds.".to_owned()
    })?;
    if timeout == 0 {
        return Err("ASR provider timeout must be greater than zero.".to_owned());
    }
    Ok(Some(timeout))
}

fn parse_string_array(value: &str, label: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<String>>(value)
        .map_err(|error| format!("Parse command {label} as a JSON string array: {error}"))
}

fn environment_entries(environment: &HashMap<String, String>) -> Vec<AsrProviderEnvironmentEntry> {
    let mut entries = environment
        .iter()
        .map(|(key, value)| AsrProviderEnvironmentEntry {
            key: key.clone(),
            value: SecretInput::new(value.clone()),
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    entries
}

fn environment_map(
    entries: &[AsrProviderEnvironmentEntry],
) -> Result<HashMap<String, String>, String> {
    let mut environment = HashMap::with_capacity(entries.len());
    for entry in entries {
        if entry.key.trim().is_empty() {
            return Err("Command environment variable names cannot be empty.".to_owned());
        }
        if environment
            .insert(entry.key.clone(), entry.value.as_str().to_owned())
            .is_some()
        {
            return Err(format!(
                "Command environment variable `{}` is duplicated.",
                entry.key
            ));
        }
    }
    Ok(environment)
}

fn optional_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn endpoint_input_is_secure(value: &str) -> bool {
    if let Ok(url) = url::Url::parse(value) {
        return !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some_and(|query| !query.is_empty())
            || url.fragment().is_some_and(|fragment| !fragment.is_empty());
    }
    value.contains('@') || value.contains('?') || value.contains('#')
}

#[cfg(test)]
#[path = "asr_provider_management/tests.rs"]
mod tests;
