//! Scene lifecycle state, validation, and persistence.

mod view;

use iced::Task;
use vinpst_config::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition, VinpstConfig};

use crate::{
    App, ConfigDocument, ConfigSaveOutcome, GuiText, Message, OperationState, load_config_document,
    save_updated_config_with_daemon,
};

/// One editable field in the scene form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneEditorField {
    /// Stable id for a newly created scene.
    Id,
    /// User-visible scene label.
    Label,
    /// Optional prompt template.
    Prompt,
    /// Optional LLM provider id retained for message compatibility.
    ProviderId,
    /// Optional model override.
    Model,
    /// Number of requested candidates.
    CandidateCount,
    /// Optional timeout in milliseconds.
    TimeoutMs,
    /// Number of recent context lines.
    ContextLines,
}

/// One configured LLM provider choice exposed by the scene editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneProviderSelection {
    /// Do not bind the scene to an LLM provider.
    None,
    /// Bind the scene to one configured provider id.
    Configured(String),
}

impl SceneProviderSelection {
    fn from_provider_id(provider_id: &str) -> Self {
        if provider_id.is_empty() {
            Self::None
        } else {
            Self::Configured(provider_id.to_owned())
        }
    }

    fn into_provider_id(self) -> String {
        match self {
            Self::None => String::new(),
            Self::Configured(provider_id) => provider_id,
        }
    }
}

/// One scene lifecycle interaction handled by the Resources page.
#[derive(Debug, Clone)]
pub enum SceneMessage {
    /// Open an empty scene creation form.
    BeginAdd,
    /// Open an existing scene for editing.
    BeginEdit(String),
    /// Update one field in the active scene form.
    EditorChanged {
        /// Typed field being edited.
        field: SceneEditorField,
        /// New user-entered value.
        value: String,
    },
    /// Select one configured LLM provider or clear the scene binding.
    ProviderSelected(SceneProviderSelection),
    /// Close the scene form without saving.
    CancelEdit,
    /// Validate and persist the active scene form.
    Save,
    /// Select and persist one configured scene.
    Use(String),
    /// Remove one inactive configured scene.
    Remove(String),
    /// Result of one asynchronous scene lifecycle mutation.
    MutationFinished(Result<SceneMutationOutcome, String>),
}

/// Result of one persisted scene lifecycle mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneMutationOutcome {
    /// Shared atomic config-save receipt and daemon reload summary.
    pub save: ConfigSaveOutcome,
    /// Secret-free user-facing mutation summary.
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SceneEditorState {
    original_id: Option<String>,
    id: String,
    label: String,
    prompt: String,
    provider_id: String,
    model: String,
    candidate_count: String,
    timeout_ms: String,
    context_lines: String,
}

impl SceneEditorState {
    fn add() -> Self {
        Self {
            original_id: None,
            id: String::new(),
            label: String::new(),
            prompt: String::new(),
            provider_id: String::new(),
            model: String::new(),
            candidate_count: "1".to_owned(),
            timeout_ms: String::new(),
            context_lines: "0".to_owned(),
        }
    }

    fn edit(scene: &SceneDefinition) -> Self {
        Self {
            original_id: Some(scene.id.clone()),
            id: scene.id.clone(),
            label: scene.label.clone(),
            prompt: scene.prompt.clone().unwrap_or_default(),
            provider_id: scene.provider_id.clone().unwrap_or_default(),
            model: scene.model.clone().unwrap_or_default(),
            candidate_count: scene.candidate_count.to_string(),
            timeout_ms: scene
                .timeout_ms
                .map_or_else(String::new, |value| value.to_string()),
            context_lines: scene.context_lines.to_string(),
        }
    }

    fn update(&mut self, field: SceneEditorField, value: String) {
        match field {
            SceneEditorField::Id if self.original_id.is_none() => self.id = value,
            SceneEditorField::Id => {}
            SceneEditorField::Label => self.label = value,
            SceneEditorField::Prompt => self.prompt = value,
            SceneEditorField::ProviderId => self.provider_id = value,
            SceneEditorField::Model => self.model = value,
            SceneEditorField::CandidateCount => self.candidate_count = value,
            SceneEditorField::TimeoutMs => self.timeout_ms = value,
            SceneEditorField::ContextLines => self.context_lines = value,
        }
    }

    fn definition(&self) -> Result<SceneDefinition, String> {
        let id = self
            .original_id
            .as_deref()
            .unwrap_or(&self.id)
            .trim()
            .to_owned();
        let candidate_count = parse_required_u8("candidate count", &self.candidate_count)?;
        let timeout_ms = parse_optional_u64("timeout", &self.timeout_ms)?;
        let context_lines = parse_required_u8("context lines", &self.context_lines)?;
        Ok(SceneDefinition {
            id,
            label: self.label.trim().to_owned(),
            prompt: optional_trimmed(&self.prompt),
            provider_id: optional_trimmed(&self.provider_id),
            model: optional_trimmed(&self.model),
            candidate_count,
            timeout_ms,
            context_lines,
        })
    }
}

impl App {
    pub(super) fn handle_scene_message(&mut self, message: SceneMessage) -> Task<Message> {
        match message {
            SceneMessage::BeginAdd => self.begin_add_scene(),
            SceneMessage::BeginEdit(id) => self.begin_edit_scene(&id),
            SceneMessage::EditorChanged { field, value } => {
                self.update_scene_editor(field, value);
            }
            SceneMessage::ProviderSelected(selection) => {
                self.select_scene_provider(selection);
            }
            SceneMessage::CancelEdit => self.cancel_scene_editor(),
            SceneMessage::Save => return self.begin_scene_save(),
            SceneMessage::Use(id) => return self.begin_scene_use(&id),
            SceneMessage::Remove(id) => return self.begin_scene_remove(&id),
            SceneMessage::MutationFinished(result) => return self.finish_scene_mutation(result),
        }
        Task::none()
    }

    pub(super) fn begin_add_scene(&mut self) {
        if self.is_busy() || self.scene_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return;
        }
        self.scene_editor = Some(SceneEditorState::add());
        self.operation = OperationState::Idle;
    }

    pub(super) fn begin_edit_scene(&mut self, scene_id: &str) {
        if self.is_busy() || self.scene_editor.is_some() {
            return;
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return;
        }
        let Some(scene) = self
            .config
            .as_ref()
            .ok()
            .and_then(|document| {
                document
                    .config
                    .scenes
                    .definitions
                    .iter()
                    .find(|scene| scene.id == scene_id)
            })
            .cloned()
        else {
            self.operation =
                OperationState::Failed(format!("Scene `{scene_id}` is no longer configured."));
            return;
        };
        self.scene_editor = Some(SceneEditorState::edit(&scene));
        self.operation = OperationState::Idle;
    }

    pub(super) fn update_scene_editor(&mut self, field: SceneEditorField, value: String) {
        if let Some(editor) = &mut self.scene_editor {
            editor.update(field, value);
        }
    }

    fn select_scene_provider(&mut self, selection: SceneProviderSelection) {
        if let Some(editor) = &mut self.scene_editor {
            editor.provider_id = selection.into_provider_id();
        }
    }

    pub(super) fn cancel_scene_editor(&mut self) {
        if !self.is_busy() {
            self.scene_editor = None;
            self.operation = OperationState::Idle;
        }
    }

    pub(super) fn begin_scene_save(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        let Some(editor) = self.scene_editor.clone() else {
            return Task::none();
        };
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let result = if editor.original_id.is_some() {
            edit_scene(&document.config, &editor).map(|updated| {
                let scene_id = editor.original_id.clone().unwrap_or_default();
                (updated, self.locale.scene_updated(&scene_id))
            })
        } else {
            add_scene(&document.config, &editor).map(|updated| {
                let scene_id = editor.id.trim();
                (updated, self.locale.scene_added(scene_id))
            })
        };
        let (updated, summary) = match result {
            Ok(result) => result,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.begin_scene_mutation(
            document.clone(),
            updated,
            summary,
            self.locale.text(GuiText::SavingSceneConfiguration),
        )
    }

    pub(super) fn begin_scene_use(&mut self, scene_id: &str) -> Task<Message> {
        if self.is_busy() || self.scene_editor.is_some() {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let updated = match use_scene(&document.config, scene_id) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.begin_scene_mutation(
            document.clone(),
            updated,
            self.locale.scene_selected(scene_id),
            self.locale.text(GuiText::SelectingScene),
        )
    }

    pub(super) fn begin_scene_remove(&mut self, scene_id: &str) -> Task<Message> {
        if self.is_busy() || self.scene_editor.is_some() {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let updated = match remove_scene(&document.config, scene_id) {
            Ok(updated) => updated,
            Err(error) => {
                self.operation = OperationState::Failed(error);
                return Task::none();
            }
        };
        self.begin_scene_mutation(
            document.clone(),
            updated,
            self.locale.scene_removed(scene_id),
            self.locale.text(GuiText::RemovingScene),
        )
    }

    fn begin_scene_mutation(
        &mut self,
        document: ConfigDocument,
        updated: VinpstConfig,
        summary: String,
        progress: &'static str,
    ) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        self.operation = OperationState::Running(progress);
        crate::blocking_task::perform(
            "vinpst-gui-scene-mutation",
            move || {
                save_updated_config_with_daemon(&document, &updated)
                    .map(|save| SceneMutationOutcome { save, summary })
            },
            |result| {
                Message::Scene(SceneMessage::MutationFinished(
                    result.unwrap_or_else(|failure| Err(failure.to_string())),
                ))
            },
        )
    }

    pub(super) fn finish_scene_mutation(
        &mut self,
        result: Result<SceneMutationOutcome, String>,
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

fn scene_provider_selections(config: &VinpstConfig) -> Vec<SceneProviderSelection> {
    std::iter::once(SceneProviderSelection::None)
        .chain(
            config
                .llm
                .providers
                .iter()
                .map(|provider| SceneProviderSelection::Configured(provider.id.clone())),
        )
        .collect()
}

fn add_scene(config: &VinpstConfig, editor: &SceneEditorState) -> Result<VinpstConfig, String> {
    let definition = editor.definition()?;
    if config
        .scenes
        .definitions
        .iter()
        .any(|scene| scene.id == definition.id)
    {
        return Err(format!("Scene `{}` already exists.", definition.id));
    }
    let mut updated = config.clone();
    updated.scenes.definitions.push(definition);
    validate_scene_update(updated)
}

fn edit_scene(config: &VinpstConfig, editor: &SceneEditorState) -> Result<VinpstConfig, String> {
    let original_id = editor
        .original_id
        .as_deref()
        .ok_or_else(|| "No existing scene is selected for editing.".to_owned())?;
    let definition = editor.definition()?;
    let mut updated = config.clone();
    let scene = updated
        .scenes
        .definitions
        .iter_mut()
        .find(|scene| scene.id == original_id)
        .ok_or_else(|| format!("Scene `{original_id}` is no longer configured."))?;
    *scene = definition;
    validate_scene_update(updated)
}

fn use_scene(config: &VinpstConfig, scene_id: &str) -> Result<VinpstConfig, String> {
    if !config
        .scenes
        .definitions
        .iter()
        .any(|scene| scene.id == scene_id)
    {
        return Err(format!("Scene `{scene_id}` is not configured."));
    }
    let mut updated = config.clone();
    scene_id.clone_into(&mut updated.scenes.active_scene);
    validate_scene_update(updated)
}

fn remove_scene(config: &VinpstConfig, scene_id: &str) -> Result<VinpstConfig, String> {
    if scene_is_built_in(scene_id) {
        return Err(format!("Refusing to remove built-in scene `{scene_id}`."));
    }
    if config.scenes.active_scene == scene_id {
        return Err(format!(
            "Scene `{scene_id}` is active; select another scene before removing it."
        ));
    }
    let mut updated = config.clone();
    let before = updated.scenes.definitions.len();
    updated
        .scenes
        .definitions
        .retain(|scene| scene.id != scene_id);
    if updated.scenes.definitions.len() == before {
        return Err(format!("Scene `{scene_id}` is not configured."));
    }
    validate_scene_update(updated)
}

fn validate_scene_update(updated: VinpstConfig) -> Result<VinpstConfig, String> {
    updated
        .validate()
        .map_err(|error| format!("Validate scene configuration: {error}"))?;
    Ok(updated)
}

fn scene_is_built_in(scene_id: &str) -> bool {
    matches!(scene_id, RAW_SCENE_ID | COMMAND_SCENE_ID)
}

fn optional_trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_required_u8(label: &str, value: &str) -> Result<u8, String> {
    value
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("Scene {label} must be an integer from 0 to 255."))
}

fn parse_optional_u64(label: &str, value: &str) -> Result<Option<u64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| format!("Scene {label} must be a non-negative integer."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vinpst_config::LlmProviderConfig;

    fn new_scene_editor() -> SceneEditorState {
        let mut editor = SceneEditorState::add();
        editor.id = " meeting ".to_owned();
        editor.label = " Meeting notes ".to_owned();
        editor.prompt = " Summarize {{context}} ".to_owned();
        editor.model = " model-override ".to_owned();
        editor.candidate_count = "2".to_owned();
        editor.timeout_ms = "5000".to_owned();
        editor.context_lines = "3".to_owned();
        editor
    }

    #[test]
    fn add_scene_builds_trimmed_typed_definition() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let updated = add_scene(&config, &new_scene_editor()).expect("add scene");
        let scene = updated
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == "meeting")
            .expect("added scene");
        assert_eq!(scene.label, "Meeting notes");
        assert_eq!(scene.prompt.as_deref(), Some("Summarize {{context}}"));
        assert_eq!(scene.provider_id, None);
        assert_eq!(scene.model.as_deref(), Some("model-override"));
        assert_eq!(scene.candidate_count, 2);
        assert_eq!(scene.timeout_ms, Some(5000));
        assert_eq!(scene.context_lines, 3);
    }

    #[test]
    fn add_scene_rejects_duplicate_id() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let mut editor = new_scene_editor();
        editor.id = config.scenes.active_scene.clone();
        let error = add_scene(&config, &editor).expect_err("reject duplicate");
        assert!(error.contains("already exists"));
    }

    #[test]
    fn edit_scene_keeps_stable_id_and_validates_fields() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let existing = config.scenes.definitions[0].clone();
        let mut editor = SceneEditorState::edit(&existing);
        editor.label = "Updated raw".to_owned();
        editor.candidate_count = "4".to_owned();
        editor.update(SceneEditorField::Id, "ignored".to_owned());
        let updated = edit_scene(&config, &editor).expect("edit scene");
        let scene = &updated.scenes.definitions[0];
        assert_eq!(scene.id, existing.id);
        assert_eq!(scene.label, "Updated raw");
        assert_eq!(scene.candidate_count, 4);
    }

    #[test]
    fn edit_scene_rejects_unknown_provider() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let mut editor = SceneEditorState::edit(&config.scenes.definitions[0]);
        editor.provider_id = "missing-provider".to_owned();
        let error = edit_scene(&config, &editor).expect_err("reject unknown provider");
        assert!(error.contains("missing-provider"));
    }

    #[test]
    fn use_and_remove_scene_enforce_active_and_built_in_boundaries() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let with_custom = add_scene(&config, &new_scene_editor()).expect("add custom scene");
        let selected = use_scene(&with_custom, "meeting").expect("select custom scene");
        assert_eq!(selected.scenes.active_scene, "meeting");

        let active_error = remove_scene(&selected, "meeting").expect_err("reject active removal");
        assert!(active_error.contains("is active"));
        for built_in in [RAW_SCENE_ID, COMMAND_SCENE_ID] {
            assert!(scene_is_built_in(built_in));
            let error = remove_scene(&selected, built_in).expect_err("reject built-in removal");
            assert!(error.contains("built-in scene"));
        }
        assert!(!scene_is_built_in("meeting"));

        let raw_selected = use_scene(&selected, RAW_SCENE_ID).expect("restore raw scene");
        let removed = remove_scene(&raw_selected, "meeting").expect("remove inactive custom scene");
        assert!(
            removed
                .scenes
                .definitions
                .iter()
                .all(|scene| scene.id != "meeting")
        );
    }

    #[test]
    fn numeric_scene_fields_report_input_errors_before_mutation() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let mut editor = new_scene_editor();
        editor.candidate_count = "many".to_owned();
        let error = add_scene(&config, &editor).expect_err("reject invalid number");
        assert!(error.contains("candidate count"));
        assert_eq!(config.scenes.definitions.len(), 2);
    }

    #[test]
    fn provider_selection_lists_configured_ids_and_updates_typed_scene() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.providers.push(LlmProviderConfig {
            id: "cloud".to_owned(),
            base_url: "https://example.invalid/v1".to_owned(),
            api_key: String::new(),
            model: Some("provider-default".to_owned()),
            extra_body: serde_json::json!({}),
            extra: std::collections::HashMap::new(),
        });

        assert_eq!(
            scene_provider_selections(&config),
            vec![
                SceneProviderSelection::None,
                SceneProviderSelection::Configured("cloud".to_owned()),
            ]
        );
        assert_ne!(
            crate::GuiLocale::EnUs.scene_provider_choice(None),
            crate::GuiLocale::EnUs.scene_provider_choice(Some("No provider"))
        );

        let mut app = crate::test_support::GuiHarness::new();
        app.scene_editor = Some(new_scene_editor());
        drop(app.handle_scene_message(SceneMessage::ProviderSelected(
            SceneProviderSelection::Configured("cloud".to_owned()),
        )));
        assert_eq!(
            app.scene_editor.as_ref().expect("open editor").provider_id,
            "cloud"
        );

        let mut editor = new_scene_editor();
        editor.model = "scene-model".to_owned();
        editor.provider_id =
            SceneProviderSelection::Configured("cloud".to_owned()).into_provider_id();
        let updated = add_scene(&config, &editor).expect("add provider-backed scene");
        let scene = updated
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == "meeting")
            .expect("added scene");
        assert_eq!(scene.provider_id.as_deref(), Some("cloud"));
        assert_eq!(scene.model.as_deref(), Some("scene-model"));

        editor.provider_id = SceneProviderSelection::None.into_provider_id();
        assert_eq!(
            editor.definition().expect("cleared provider").provider_id,
            None
        );
    }
}
