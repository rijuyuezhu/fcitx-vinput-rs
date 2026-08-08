//! LLM provider form state, validation, and persistence.

mod view;

use std::fmt;

use iced::Task;
use vinpst_config::{LlmProviderConfig, SceneDefinition, VinpstConfig, redact_url_for_diagnostics};
use vinpst_text::{
    OpenAiCompatibleChatTransport, OpenAiCompatibleTextAdapter,
    ReqwestOpenAiCompatibleChatTransport, TextAdapter, TextError, TextRequest,
};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, GuiText, Message, OperationState, SecretInput,
    load_config_document, save_updated_config_with_daemon,
};

/// One editable field in the LLM provider form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderEditorField {
    /// Stable id for a newly created provider.
    Id,
    /// OpenAI-compatible base URL.
    BaseUrl,
    /// Optional API key or environment-reference expression.
    ApiKey,
    /// Optional default model name.
    Model,
    /// Extra JSON request body.
    ExtraBody,
}

/// One LLM provider lifecycle interaction handled by the LLM page.
#[derive(Debug, Clone)]
pub enum LlmProviderMessage {
    /// Open an empty provider creation form.
    BeginAdd,
    /// Open an existing provider for editing.
    BeginEdit(String),
    /// Remove one configured provider when the full config remains valid.
    Remove(String),
    /// Update the connectivity-test input without exposing it through `Debug`.
    TestInputChanged(SecretInput),
    /// Test one configured OpenAI-compatible provider.
    Test(String),
    /// Result of one asynchronous provider test.
    TestFinished(Result<LlmProviderTestOutcome, String>),
    /// Update one field without exposing the entered value through `Debug`.
    EditorChanged {
        /// Typed field being edited.
        field: LlmProviderEditorField,
        /// Redacted user-entered value.
        value: SecretInput,
    },
    /// Restore the active form to its initially loaded values.
    ResetEdit,
    /// Close the provider form without saving.
    CancelEdit,
    /// Validate and persist the active provider form.
    Save,
    /// Result of one asynchronous provider mutation.
    MutationFinished(Result<LlmProviderMutationOutcome, String>),
}

/// Result of one persisted LLM provider mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Secret-free user-facing mutation summary.
    pub summary: String,
}

/// Secret-free result of one LLM provider connectivity test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmProviderTestOutcome {
    /// Stable configured provider id.
    pub provider_id: String,
    /// Number of parsed response candidates.
    pub candidate_count: usize,
}

#[derive(Clone, PartialEq, Eq)]
struct LlmProviderEditorFields {
    id: String,
    base_url: String,
    api_key: SecretInput,
    model: String,
    extra_body: String,
}

impl fmt::Debug for LlmProviderEditorFields {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmProviderEditorFields")
            .field("id", &self.id)
            .field("base_url", &redact_url_for_diagnostics(&self.base_url))
            .field("api_key", &self.api_key)
            .field("model", &self.model)
            .field("extra_body", &"<redacted JSON>")
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct LlmProviderEditorState {
    original_id: Option<String>,
    original_provider: Option<LlmProviderConfig>,
    base_url_secure: bool,
    baseline: LlmProviderEditorFields,
    fields: LlmProviderEditorFields,
    preserved_extra: std::collections::HashMap<String, serde_json::Value>,
}

impl fmt::Debug for LlmProviderEditorState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmProviderEditorState")
            .field("original_id", &self.original_id)
            .field(
                "original_provider",
                &self
                    .original_provider
                    .as_ref()
                    .map(|_| "<redacted provider>"),
            )
            .field("base_url_secure", &self.base_url_secure)
            .field("baseline", &self.baseline)
            .field("fields", &self.fields)
            .field("preserved_extra_count", &self.preserved_extra.len())
            .finish()
    }
}

impl LlmProviderEditorState {
    fn add() -> Self {
        let fields = LlmProviderEditorFields {
            id: String::new(),
            base_url: String::new(),
            api_key: SecretInput::new(String::new()),
            model: String::new(),
            extra_body: "{}".to_owned(),
        };
        Self {
            original_id: None,
            original_provider: None,
            base_url_secure: false,
            baseline: fields.clone(),
            fields,
            preserved_extra: std::collections::HashMap::new(),
        }
    }

    fn edit(provider: &LlmProviderConfig) -> Self {
        let fields = LlmProviderEditorFields {
            id: provider.id.clone(),
            base_url: provider.base_url.clone(),
            api_key: SecretInput::new(provider.api_key.clone()),
            model: provider.model.clone().unwrap_or_default(),
            extra_body: serde_json::to_string_pretty(&provider.extra_body)
                .unwrap_or_else(|_| "{}".to_owned()),
        };
        Self {
            original_id: Some(provider.id.clone()),
            original_provider: Some(provider.clone()),
            base_url_secure: base_url_input_is_secure(&provider.base_url),
            baseline: fields.clone(),
            fields,
            preserved_extra: provider.extra.clone(),
        }
    }

    fn update(&mut self, field: LlmProviderEditorField, value: SecretInput) {
        let value = value.into_inner();
        match field {
            LlmProviderEditorField::Id if self.original_id.is_none() => self.fields.id = value,
            LlmProviderEditorField::Id => {}
            LlmProviderEditorField::BaseUrl => {
                self.base_url_secure |= base_url_input_is_secure(&value);
                self.fields.base_url = value;
            }
            LlmProviderEditorField::ApiKey => self.fields.api_key = SecretInput::new(value),
            LlmProviderEditorField::Model => self.fields.model = value,
            LlmProviderEditorField::ExtraBody => self.fields.extra_body = value,
        }
    }

    fn reset(&mut self) {
        self.fields = self.baseline.clone();
        self.base_url_secure = base_url_input_is_secure(&self.baseline.base_url);
    }

    fn is_dirty(&self) -> bool {
        self.fields != self.baseline
    }

    fn provider(&self) -> Result<LlmProviderConfig, String> {
        let id = self.original_id.as_ref().map_or_else(
            || self.fields.id.trim().to_owned(),
            std::clone::Clone::clone,
        );
        if id.trim().is_empty() {
            return Err("LLM provider id cannot be empty.".to_owned());
        }
        let original_provider = self.original_provider.as_ref();
        let base_url = original_provider
            .filter(|_| self.fields.base_url == self.baseline.base_url)
            .map_or_else(
                || self.fields.base_url.trim().to_owned(),
                |provider| provider.base_url.clone(),
            );
        if base_url.trim().is_empty() {
            return Err("LLM provider base URL cannot be empty.".to_owned());
        }
        let extra_body = if let Some(provider) =
            original_provider.filter(|_| self.fields.extra_body == self.baseline.extra_body)
        {
            provider.extra_body.clone()
        } else {
            let extra_body_text = self.fields.extra_body.trim();
            if extra_body_text.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(extra_body_text)
                    .map_err(|error| format!("Parse extra body as JSON object: {error}"))?
            }
        };
        if !extra_body.is_object() {
            return Err("LLM provider extra body must be a JSON object.".to_owned());
        }
        let api_key = original_provider
            .filter(|_| self.fields.api_key == self.baseline.api_key)
            .map_or_else(
                || self.fields.api_key.as_str().trim().to_owned(),
                |provider| provider.api_key.clone(),
            );
        let model = original_provider
            .filter(|_| self.fields.model == self.baseline.model)
            .map_or_else(
                || optional_trimmed(&self.fields.model),
                |provider| provider.model.clone(),
            );
        Ok(LlmProviderConfig {
            id,
            base_url,
            api_key,
            model,
            extra_body,
            extra: self.preserved_extra.clone(),
        })
    }
}

impl App {
    pub(super) fn handle_llm_provider_message(
        &mut self,
        message: LlmProviderMessage,
    ) -> Task<Message> {
        match message {
            LlmProviderMessage::BeginAdd => self.begin_add_llm_provider(),
            LlmProviderMessage::BeginEdit(id) => self.begin_edit_llm_provider(&id),
            LlmProviderMessage::Remove(id) => return self.begin_llm_provider_remove(&id),
            LlmProviderMessage::TestInputChanged(value) => {
                self.llm_provider_test_text = value;
            }
            LlmProviderMessage::Test(id) => return self.begin_llm_provider_test(&id),
            LlmProviderMessage::TestFinished(result) => {
                return self.finish_llm_provider_test(result);
            }
            LlmProviderMessage::EditorChanged { field, value } => {
                self.update_llm_provider_editor(field, value);
            }
            LlmProviderMessage::ResetEdit => self.reset_llm_provider_editor(),
            LlmProviderMessage::CancelEdit => self.cancel_llm_provider_editor(),
            LlmProviderMessage::Save => return self.begin_llm_provider_save(),
            LlmProviderMessage::MutationFinished(result) => {
                return self.finish_llm_provider_mutation(result);
            }
        }
        Task::none()
    }

    fn begin_add_llm_provider(&mut self) {
        if self.is_busy() || self.llm_provider_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return;
        }
        self.llm_provider_editor = Some(LlmProviderEditorState::add());
        self.operation = OperationState::Idle;
    }

    fn begin_edit_llm_provider(&mut self, provider_id: &str) {
        if self.is_busy() || self.llm_provider_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return;
        }
        let Some(provider) = self
            .config
            .as_ref()
            .ok()
            .and_then(|document| {
                document
                    .config
                    .llm
                    .providers
                    .iter()
                    .find(|provider| provider.id == provider_id)
            })
            .cloned()
        else {
            self.operation = OperationState::Failed(format!(
                "LLM provider `{provider_id}` is no longer configured."
            ));
            return;
        };
        self.llm_provider_editor = Some(LlmProviderEditorState::edit(&provider));
        self.operation = OperationState::Idle;
    }

    fn begin_llm_provider_remove(&mut self, provider_id: &str) -> Task<Message> {
        if self.is_busy() || self.llm_provider_editor.is_some() {
            return Task::none();
        }
        if !self.ensure_llm_provider_mutation_allowed() {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let removal = match remove_llm_provider(&document.config, provider_id) {
            Ok(removal) => removal,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.begin_llm_provider_mutation(
            document.clone(),
            removal.updated,
            self.locale
                .llm_provider_removed(provider_id, removal.cleared_scene_references),
        )
    }

    fn begin_llm_provider_test(&mut self, provider_id: &str) -> Task<Message> {
        if self.is_busy() || self.llm_provider_editor.is_some() {
            return Task::none();
        }
        let test_text = self.llm_provider_test_text.as_str().trim().to_owned();
        if test_text.is_empty() {
            self.operation = OperationState::Failed(
                self.locale
                    .text(GuiText::ConnectivityInputRequired)
                    .to_owned(),
            );
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let provider = match llm_provider_test_target(&document.config, provider_id) {
            Ok(provider) => provider,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };

        self.operation = OperationState::Running(self.locale.text(GuiText::TestingLlmProvider));
        crate::blocking_task::perform(
            "vinpst-gui-llm-provider-test",
            move || test_llm_provider(provider, &test_text),
            |result| {
                Message::LlmProvider(LlmProviderMessage::TestFinished(result.unwrap_or_else(
                    |failure| Err(format!("LLM provider test worker failed: {failure}")),
                )))
            },
        )
    }

    fn finish_llm_provider_test(
        &mut self,
        result: Result<LlmProviderTestOutcome, String>,
    ) -> Task<Message> {
        match result {
            Ok(outcome) => {
                self.operation = OperationState::Succeeded(
                    self.locale
                        .llm_provider_test_succeeded(&outcome.provider_id, outcome.candidate_count),
                );
            }
            Err(error) => self.operation = OperationState::Failed(error),
        }
        Task::none()
    }

    fn update_llm_provider_editor(&mut self, field: LlmProviderEditorField, value: SecretInput) {
        if let Some(editor) = &mut self.llm_provider_editor {
            editor.update(field, value);
        }
    }

    fn reset_llm_provider_editor(&mut self) {
        if !self.is_busy() {
            if let Some(editor) = &mut self.llm_provider_editor {
                editor.reset();
            }
            self.operation = OperationState::Idle;
        }
    }

    fn cancel_llm_provider_editor(&mut self) {
        if !self.is_busy() {
            self.llm_provider_editor = None;
            self.operation = OperationState::Idle;
        }
    }

    fn begin_llm_provider_save(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(editor) = self.llm_provider_editor.clone() else {
            return Task::none();
        };
        if !editor.is_dirty() {
            return Task::none();
        }
        if !self.ensure_llm_provider_mutation_allowed() {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let result = if editor.original_id.is_some() {
            edit_llm_provider(&document.config, &editor).map(|updated| {
                let provider_id = editor.original_id.clone().unwrap_or_default();
                (
                    updated,
                    self.locale.llm_provider_changed("update", &provider_id),
                )
            })
        } else {
            add_llm_provider(&document.config, &editor).map(|updated| {
                let provider_id = editor.fields.id.trim();
                (
                    updated,
                    self.locale.llm_provider_changed("add", provider_id),
                )
            })
        };
        let (updated, summary) = match result {
            Ok(result) => result,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.begin_llm_provider_mutation(document.clone(), updated, summary)
    }

    fn ensure_llm_provider_mutation_allowed(&mut self) -> bool {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return false;
        }
        if let Err(error) = self.ensure_no_open_asr_provider_editor() {
            self.operation = OperationState::Failed(error);
            return false;
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return false;
        }
        true
    }

    fn begin_llm_provider_mutation(
        &mut self,
        document: ConfigDocument,
        updated: VinpstConfig,
        summary: String,
    ) -> Task<Message> {
        self.operation = OperationState::Running(self.locale.text(GuiText::SavingLlmProvider));
        crate::blocking_task::perform(
            "vinpst-gui-llm-provider-mutation",
            move || {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| LlmProviderMutationOutcome { save, summary })
            },
            |result| {
                Message::LlmProvider(LlmProviderMessage::MutationFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
        )
    }

    fn finish_llm_provider_mutation(
        &mut self,
        result: Result<LlmProviderMutationOutcome, String>,
    ) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        let path = outcome.save.path.display().to_string();
        let backup = outcome
            .save
            .backup_path
            .as_ref()
            .map(|path| path.display().to_string());
        self.replace_config(load_config_document(Some(&outcome.save.path)));
        self.operation = OperationState::Succeeded(self.locale.save_receipt(
            &outcome.summary,
            &path,
            backup.as_deref(),
            &outcome.save.daemon_reload,
        ));
        self.begin_daemon_refresh(false)
    }
}

pub(super) const fn extra_body_input_is_secure() -> bool {
    true
}

fn base_url_input_is_secure(value: &str) -> bool {
    if let Ok(url) = url::Url::parse(value) {
        return !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some_and(|query| !query.is_empty())
            || url.fragment().is_some_and(|fragment| !fragment.is_empty());
    }

    let query_present = value
        .split_once('?')
        .is_some_and(|(_, query)| !query.split('#').next().unwrap_or_default().is_empty());
    let fragment_present = value
        .split_once('#')
        .is_some_and(|(_, fragment)| !fragment.is_empty());
    let authority = value
        .split_once(':')
        .map_or(value, |(_, remainder)| remainder.trim_start_matches('/'));
    let authority_end = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    let userinfo_present = authority[..authority_end].contains('@');
    query_present || fragment_present || userinfo_present
}

fn add_llm_provider(
    config: &VinpstConfig,
    editor: &LlmProviderEditorState,
) -> Result<VinpstConfig, String> {
    let provider = editor.provider()?;
    if config
        .llm
        .providers
        .iter()
        .any(|configured| configured.id == provider.id)
    {
        return Err(format!("LLM provider `{}` already exists.", provider.id));
    }
    let mut updated = config.clone();
    updated.llm.providers.push(provider);
    validate_llm_provider_update(updated)
}

fn edit_llm_provider(
    config: &VinpstConfig,
    editor: &LlmProviderEditorState,
) -> Result<VinpstConfig, String> {
    let original_id = editor
        .original_id
        .as_deref()
        .ok_or_else(|| "No existing LLM provider is selected for editing.".to_owned())?;
    let provider = editor.provider()?;
    let mut updated = config.clone();
    let configured = updated
        .llm
        .providers
        .iter_mut()
        .find(|configured| configured.id == original_id)
        .ok_or_else(|| format!("LLM provider `{original_id}` is no longer configured."))?;
    *configured = provider;
    validate_llm_provider_update(updated)
}

fn llm_provider_test_target(
    config: &VinpstConfig,
    provider_id: &str,
) -> Result<LlmProviderConfig, String> {
    let mut provider = config
        .llm
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .cloned()
        .ok_or_else(|| format!("LLM provider `{provider_id}` is no longer configured."))?;
    if provider.model.is_some() {
        return Ok(provider);
    }

    let mut scene_model: Option<&String> = None;
    for model in config
        .scenes
        .definitions
        .iter()
        .filter(|scene| scene.provider_id.as_deref() == Some(provider_id))
        .filter_map(|scene| scene.model.as_ref())
    {
        if scene_model.is_some_and(|configured| configured != model) {
            return Err(format!(
                "LLM provider `{provider_id}` has multiple Scene models and no default model; configure one provider model before testing."
            ));
        }
        scene_model = Some(model);
    }
    provider.model = scene_model.cloned();
    if provider.model.is_none() {
        return Err(format!(
            "LLM provider `{provider_id}` has no default model and no referencing Scene supplies one."
        ));
    }
    Ok(provider)
}

fn llm_provider_test_scene(provider: &LlmProviderConfig) -> SceneDefinition {
    SceneDefinition {
        id: "__llm_test__".to_owned(),
        label: "LLM Test".to_owned(),
        prompt: Some(
            "Return a JSON object with a candidates array containing one short connectivity result."
                .to_owned(),
        ),
        provider_id: Some(provider.id.clone()),
        model: provider.model.clone(),
        candidate_count: 1,
        timeout_ms: None,
        context_lines: 0,
    }
}

fn test_llm_provider(
    provider: LlmProviderConfig,
    test_text: &str,
) -> Result<LlmProviderTestOutcome, String> {
    test_llm_provider_with_transport(
        provider,
        test_text,
        ReqwestOpenAiCompatibleChatTransport::new(),
    )
}

fn test_llm_provider_with_transport<T: OpenAiCompatibleChatTransport>(
    provider: LlmProviderConfig,
    test_text: &str,
    transport: T,
) -> Result<LlmProviderTestOutcome, String> {
    let scene = llm_provider_test_scene(&provider);
    let request = TextRequest {
        raw_text: test_text,
        scene: &scene,
        selected_text: None,
    };
    let payload = OpenAiCompatibleTextAdapter::new(provider.clone(), transport)
        .finish(&request)
        .map_err(|error| llm_provider_test_error(&provider.id, &error))?;
    Ok(LlmProviderTestOutcome {
        provider_id: provider.id,
        candidate_count: payload.candidates.len(),
    })
}

fn llm_provider_test_error(provider_id: &str, error: &TextError) -> String {
    let category = match error {
        TextError::AdapterFailed(message) => safe_provider_failure_category(message),
        _ => "provider test could not be completed".to_owned(),
    };
    format!("Test LLM provider `{provider_id}` failed: {category}.")
}

fn safe_provider_failure_category(message: &str) -> String {
    const HTTP_PREFIX: &str = "OpenAI-compatible provider returned HTTP ";
    if let Some(status_and_body) = message.strip_prefix(HTTP_PREFIX) {
        let status = status_and_body
            .split_once(':')
            .map_or(status_and_body, |(status, _)| status)
            .trim();
        if !status.is_empty()
            && status.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, ' ' | '-')
            })
        {
            return format!("provider returned HTTP {status}");
        }
    }
    if message.contains("timed out") {
        "provider request timed out".to_owned()
    } else if message.contains("exceeds") {
        "provider response exceeded the size limit".to_owned()
    } else if message.contains("not valid UTF-8") {
        "provider response was not valid UTF-8".to_owned()
    } else if message.contains("did not contain candidates") {
        "provider response did not contain candidates".to_owned()
    } else if message.contains("client setup failed") {
        "HTTP client setup failed".to_owned()
    } else if message.contains("response body read failed") {
        "provider response could not be read".to_owned()
    } else {
        "provider request failed".to_owned()
    }
}

struct LlmProviderRemoval {
    updated: VinpstConfig,
    cleared_scene_references: usize,
}

fn remove_llm_provider(
    config: &VinpstConfig,
    provider_id: &str,
) -> Result<LlmProviderRemoval, String> {
    let mut updated = config.clone();
    let before = updated.llm.providers.len();
    updated
        .llm
        .providers
        .retain(|provider| provider.id != provider_id);
    if updated.llm.providers.len() == before {
        return Err(format!(
            "LLM provider `{provider_id}` is no longer configured."
        ));
    }
    let mut cleared_scene_references = 0;
    for scene in &mut updated.scenes.definitions {
        if scene.provider_id.as_deref() == Some(provider_id) {
            scene.provider_id = None;
            scene.model = None;
            cleared_scene_references += 1;
        }
    }
    Ok(LlmProviderRemoval {
        updated: validate_llm_provider_update(updated)?,
        cleared_scene_references,
    })
}

fn validate_llm_provider_update(config: VinpstConfig) -> Result<VinpstConfig, String> {
    config
        .validate()
        .map_err(|error| format!("Validate edited LLM provider: {error}"))?;
    Ok(config)
}

fn optional_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
#[path = "llm_provider_preservation_tests.rs"]
mod preservation_tests;

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use vinpst_text::{OpenAiCompatibleChatRequest, TextError};

    use super::*;

    #[derive(Debug, Clone)]
    struct StaticTransport {
        response_body: String,
        seen_request: Arc<Mutex<Option<OpenAiCompatibleChatRequest>>>,
        seen_timeout_ms: Arc<Mutex<Option<u64>>>,
    }

    impl StaticTransport {
        fn one_candidate() -> Self {
            Self {
                response_body: serde_json::json!({
                    "choices": [{
                        "message": {
                            "content": serde_json::json!({
                                "candidates": ["connected"]
                            })
                            .to_string()
                        }
                    }]
                })
                .to_string(),
                seen_request: Arc::new(Mutex::new(None)),
                seen_timeout_ms: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl OpenAiCompatibleChatTransport for StaticTransport {
        fn send(
            &self,
            request: &OpenAiCompatibleChatRequest,
            timeout_ms: Option<u64>,
        ) -> Result<String, TextError> {
            *self.seen_request.lock().expect("request lock") = Some(request.clone());
            *self.seen_timeout_ms.lock().expect("timeout lock") = timeout_ms;
            Ok(self.response_body.clone())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct SensitiveFailureTransport;

    impl OpenAiCompatibleChatTransport for SensitiveFailureTransport {
        fn send(
            &self,
            _request: &OpenAiCompatibleChatRequest,
            _timeout_ms: Option<u64>,
        ) -> Result<String, TextError> {
            Err(TextError::AdapterFailed(
                "OpenAI-compatible provider returned HTTP 401 Unauthorized: sensitive response body api-secret sensitive connectivity input"
                    .to_owned(),
            ))
        }
    }

    fn provider(id: &str) -> LlmProviderConfig {
        LlmProviderConfig {
            id: id.to_owned(),
            base_url: "https://user:secret@example.invalid/v1?key=hidden".to_owned(),
            api_key: "api-secret".to_owned(),
            model: Some("model-a".to_owned()),
            extra_body: serde_json::json!({"secret": "body-secret"}),
            extra: HashMap::from([("future".to_owned(), serde_json::json!({"v": 1}))]),
        }
    }

    #[test]
    fn editor_debug_redacts_credentials_and_extra_body() {
        let editor = LlmProviderEditorState::edit(&provider("cloud"));
        let debug = format!("{editor:?}");
        assert!(!debug.contains("api-secret"));
        assert!(!debug.contains("body-secret"));
        assert!(!debug.contains("user:secret"));
        assert!(!debug.contains("hidden"));
    }

    #[test]
    fn extra_body_input_is_always_secure() {
        assert!(extra_body_input_is_secure());
    }

    #[test]
    fn add_provider_builds_trimmed_typed_config() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let mut editor = LlmProviderEditorState::add();
        editor.update(
            LlmProviderEditorField::Id,
            SecretInput::new(" cloud ".to_owned()),
        );
        editor.update(
            LlmProviderEditorField::BaseUrl,
            SecretInput::new(" https://example.invalid/v1 ".to_owned()),
        );
        editor.update(
            LlmProviderEditorField::ApiKey,
            SecretInput::new(" key ".to_owned()),
        );
        editor.update(
            LlmProviderEditorField::Model,
            SecretInput::new(" model-a ".to_owned()),
        );
        editor.update(
            LlmProviderEditorField::ExtraBody,
            SecretInput::new("{\"temperature\":0.2}".to_owned()),
        );

        let updated = add_llm_provider(&config, &editor).expect("add provider");
        let added = updated
            .llm
            .providers
            .iter()
            .find(|provider| provider.id == "cloud")
            .expect("added provider");
        assert_eq!(added.base_url, "https://example.invalid/v1");
        assert_eq!(added.api_key, "key");
        assert_eq!(added.model.as_deref(), Some("model-a"));
        assert_eq!(added.extra_body, serde_json::json!({"temperature": 0.2}));
    }

    #[test]
    fn add_provider_rejects_duplicates_and_non_object_extra_body() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.providers.push(provider("cloud"));
        let editor = LlmProviderEditorState::edit(&provider("cloud"));
        assert!(add_llm_provider(&config, &editor).is_err());

        let mut invalid = LlmProviderEditorState::add();
        invalid.update(
            LlmProviderEditorField::Id,
            SecretInput::new("other".to_owned()),
        );
        invalid.update(
            LlmProviderEditorField::BaseUrl,
            SecretInput::new("https://example.invalid/v1".to_owned()),
        );
        invalid.update(
            LlmProviderEditorField::ExtraBody,
            SecretInput::new("[]".to_owned()),
        );
        assert!(invalid.provider().is_err());
    }

    #[test]
    fn edit_provider_keeps_id_and_forward_compatible_fields() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.providers.push(provider("cloud"));
        let configured = config.llm.providers.last().expect("configured provider");
        let mut editor = LlmProviderEditorState::edit(configured);
        editor.update(
            LlmProviderEditorField::Id,
            SecretInput::new("renamed".to_owned()),
        );
        editor.update(
            LlmProviderEditorField::Model,
            SecretInput::new("model-b".to_owned()),
        );

        let updated = edit_llm_provider(&config, &editor).expect("edit provider");
        let edited = updated
            .llm
            .providers
            .iter()
            .find(|provider| provider.id == "cloud")
            .expect("edited provider");
        assert_eq!(edited.model.as_deref(), Some("model-b"));
        assert_eq!(edited.extra["future"], serde_json::json!({"v": 1}));
        assert!(
            !updated
                .llm
                .providers
                .iter()
                .any(|provider| provider.id == "renamed")
        );
    }

    #[test]
    fn remove_provider_clears_scene_references_like_upstream_gui() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.providers.push(provider("cloud"));

        let removed = remove_llm_provider(&config, "cloud").expect("remove provider");
        assert!(removed.updated.llm.providers.is_empty());
        assert_eq!(removed.cleared_scene_references, 0);

        config.scenes.definitions[0].provider_id = Some("cloud".to_owned());
        config.scenes.definitions[0].model = Some("model-a".to_owned());
        let removed = remove_llm_provider(&config, "cloud").expect("remove referenced provider");
        assert!(removed.updated.llm.providers.is_empty());
        assert_eq!(removed.cleared_scene_references, 1);
        assert_eq!(removed.updated.scenes.definitions[0].provider_id, None);
        assert_eq!(removed.updated.scenes.definitions[0].model, None);
    }

    #[test]
    fn connectivity_test_uses_production_request_contract_and_redacted_outcome() {
        let transport = StaticTransport::one_candidate();
        let request_capture = Arc::clone(&transport.seen_request);
        let timeout_capture = Arc::clone(&transport.seen_timeout_ms);
        let outcome = test_llm_provider_with_transport(
            provider("cloud"),
            "sensitive connectivity input",
            transport,
        )
        .expect("connectivity test");

        assert_eq!(outcome.provider_id, "cloud");
        assert_eq!(outcome.candidate_count, 1);
        assert_eq!(*timeout_capture.lock().expect("timeout lock"), Some(4000));
        let request = request_capture
            .lock()
            .expect("request lock")
            .clone()
            .expect("captured request");
        assert_eq!(
            request.url,
            "https://user:secret@example.invalid/v1/chat/completions?key=hidden"
        );
        assert!(
            request
                .body
                .to_string()
                .contains("sensitive connectivity input")
        );
        let debug = format!("{outcome:?}");
        assert!(!debug.contains("sensitive connectivity input"));
        assert!(!debug.contains("api-secret"));
    }

    #[test]
    fn connectivity_failure_exposes_only_safe_error_category() {
        let error = test_llm_provider_with_transport(
            provider("cloud"),
            "sensitive connectivity input",
            SensitiveFailureTransport,
        )
        .expect_err("connectivity failure");

        assert!(error.contains("HTTP 401 Unauthorized"));
        assert!(!error.contains("sensitive response body"));
        assert!(!error.contains("sensitive connectivity input"));
        assert!(!error.contains("api-secret"));
        assert!(!error.contains("user:secret"));
        assert!(!error.contains("hidden"));
    }

    #[test]
    fn connectivity_input_message_debug_is_redacted() {
        let message = LlmProviderMessage::TestInputChanged(SecretInput::new(
            "sensitive connectivity input".to_owned(),
        ));
        assert!(!format!("{message:?}").contains("sensitive connectivity input"));
    }

    #[test]
    fn editor_dirty_state_resets_to_loaded_provider() {
        let mut editor = LlmProviderEditorState::edit(&provider("cloud"));
        assert!(!editor.is_dirty());
        editor.update(
            LlmProviderEditorField::Model,
            SecretInput::new("model-b".to_owned()),
        );
        assert!(editor.is_dirty());
        editor.reset();
        assert!(!editor.is_dirty());
        assert_eq!(editor.fields.model, "model-a");
    }
}
