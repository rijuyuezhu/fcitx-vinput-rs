//! Localized scene list and editor rendering.

use crate::keyboard_action::{adjacent_values, keyboard_button, keyboard_select};

use std::fmt;

use iced::{
    Element, Length,
    widget::{column, pick_list, row, text, text_input},
};
use vinpst_config::VinpstConfig;

use super::{
    SceneEditorField, SceneEditorState, SceneMessage, SceneProviderSelection, scene_is_built_in,
    scene_provider_selections,
};
use crate::{App, GuiLocale, GuiText, Message};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneProviderChoice {
    selection: SceneProviderSelection,
    label: String,
}

impl SceneProviderChoice {
    fn new(locale: GuiLocale, selection: SceneProviderSelection) -> Self {
        let label = match &selection {
            SceneProviderSelection::None => locale.scene_provider_choice(None),
            SceneProviderSelection::Configured(provider_id) => {
                locale.scene_provider_choice(Some(provider_id))
            }
        };
        Self { selection, label }
    }
}

impl fmt::Display for SceneProviderChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

impl App {
    pub(crate) fn scene_management_view(&self, busy: bool) -> Element<'_, Message> {
        let editor_open = self.scene_editor.is_some();
        let mut body = column![
            row![
                text(self.locale.text(GuiText::Scenes))
                    .size(22)
                    .width(Length::Fill),
                keyboard_button(self.locale.text(GuiText::AddScene)).on_press_maybe(
                    (!busy && !editor_open).then_some(Message::Scene(SceneMessage::BeginAdd)),
                ),
            ]
            .spacing(10),
        ]
        .spacing(10);

        match &self.config {
            Ok(document) => {
                let filter = self.filter.to_ascii_lowercase();
                let mut visible = 0_usize;
                for scene in &document.config.scenes.definitions {
                    let active = scene.id == document.config.scenes.active_scene;
                    let removable = !active && !scene_is_built_in(&scene.id);
                    let marker = self.locale.text(if active {
                        GuiText::Active
                    } else {
                        GuiText::Available
                    });
                    let label = format!("{} · {} · {marker}", scene.id, scene.label);
                    if !label.to_ascii_lowercase().contains(&filter) {
                        continue;
                    }
                    visible += 1;
                    body = body.push(scene_row(
                        self.locale,
                        label,
                        &scene.id,
                        active,
                        removable,
                        !busy && !editor_open,
                    ));
                }
                if visible == 0 {
                    body = body.push(text(self.locale.text(GuiText::NoScenesMatch)));
                }
            }
            Err(error) => body = body.push(text(self.locale.config_error(error))),
        }

        if let Some(editor) = &self.scene_editor {
            let provider_options = self.config.as_ref().map_or_else(
                |_| {
                    vec![SceneProviderChoice::new(
                        self.locale,
                        SceneProviderSelection::None,
                    )]
                },
                |document| scene_provider_options(self.locale, &document.config),
            );
            body = body.push(scene_editor_view(
                self.locale,
                editor,
                busy,
                provider_options,
            ));
        }
        body.into()
    }
}

fn scene_row(
    locale: GuiLocale,
    label: String,
    scene_id: &str,
    active: bool,
    removable: bool,
    controls_enabled: bool,
) -> Element<'static, Message> {
    row![
        text(label).width(Length::Fill),
        keyboard_button(locale.text(GuiText::Use)).on_press_maybe(
            (controls_enabled && !active)
                .then_some(Message::Scene(SceneMessage::Use(scene_id.to_owned()))),
        ),
        keyboard_button(locale.text(GuiText::Edit)).on_press_maybe(
            controls_enabled
                .then_some(Message::Scene(SceneMessage::BeginEdit(scene_id.to_owned()))),
        ),
        keyboard_button(locale.text(GuiText::Remove)).on_press_maybe(
            (controls_enabled && removable)
                .then_some(Message::RequestRemoveScene(scene_id.to_owned())),
        ),
    ]
    .spacing(10)
    .into()
}

fn scene_editor_view(
    locale: GuiLocale,
    editor: &SceneEditorState,
    busy: bool,
    provider_options: Vec<SceneProviderChoice>,
) -> Element<'_, Message> {
    let action = if editor.original_id.is_some() {
        GuiText::UpdateScene
    } else {
        GuiText::AddScene
    };
    let id_field: Element<'_, Message> = if editor.original_id.is_some() {
        text(locale.scene_id_immutable(&editor.id)).into()
    } else {
        labeled_input(
            locale.text(GuiText::SceneId),
            locale.text(GuiText::StableUniqueId),
            &editor.id,
            SceneEditorField::Id,
            busy,
        )
    };
    let selected = SceneProviderChoice::new(
        locale,
        SceneProviderSelection::from_provider_id(&editor.provider_id),
    );
    let provider_control: Element<'_, Message> = if busy {
        text(selected.to_string()).width(Length::Fill).into()
    } else {
        let (previous, next) = adjacent_values(&provider_options, Some(&selected));
        keyboard_select(
            pick_list(provider_options, Some(selected), |choice| {
                Message::Scene(SceneMessage::ProviderSelected(choice.selection))
            })
            .width(Length::Fill),
            previous.map(|choice| Message::Scene(SceneMessage::ProviderSelected(choice.selection))),
            next.map(|choice| Message::Scene(SceneMessage::ProviderSelected(choice.selection))),
        )
    };
    column![
        text(locale.text(action)).size(22),
        id_field,
        labeled_input(
            locale.text(GuiText::LabelField),
            locale.text(GuiText::DisplayLabelPlaceholder),
            &editor.label,
            SceneEditorField::Label,
            busy,
        ),
        labeled_input(
            locale.text(GuiText::PromptField),
            locale.text(GuiText::OptionalPromptTemplate),
            &editor.prompt,
            SceneEditorField::Prompt,
            busy,
        ),
        row![
            text(locale.text(GuiText::LlmProvider)).width(160),
            provider_control
        ]
        .spacing(10),
        labeled_input(
            locale.text(GuiText::ModelOverride),
            locale.text(GuiText::OptionalModelId),
            &editor.model,
            SceneEditorField::Model,
            busy,
        ),
        labeled_input(
            locale.text(GuiText::CandidateCount),
            locale.text(GuiText::ZeroTo32),
            &editor.candidate_count,
            SceneEditorField::CandidateCount,
            busy,
        ),
        labeled_input(
            locale.text(GuiText::TimeoutMsLabel),
            locale.text(GuiText::BlankLegacyDefault),
            &editor.timeout_ms,
            SceneEditorField::TimeoutMs,
            busy,
        ),
        labeled_input(
            locale.text(GuiText::ContextLines),
            locale.text(GuiText::ZeroTo32),
            &editor.context_lines,
            SceneEditorField::ContextLines,
            busy,
        ),
        row![
            keyboard_button(locale.text(action))
                .on_press_maybe((!busy).then_some(Message::Scene(SceneMessage::Save))),
            keyboard_button(locale.text(GuiText::Cancel))
                .on_press_maybe((!busy).then_some(Message::Scene(SceneMessage::CancelEdit))),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn scene_provider_options(locale: GuiLocale, config: &VinpstConfig) -> Vec<SceneProviderChoice> {
    scene_provider_selections(config)
        .into_iter()
        .map(|selection| SceneProviderChoice::new(locale, selection))
        .collect()
}

fn labeled_input<'a>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: SceneEditorField,
    busy: bool,
) -> Element<'a, Message> {
    row![
        text(label).width(160),
        text_input(placeholder, value)
            .on_input_maybe((!busy).then_some(move |value| {
                Message::Scene(SceneMessage::EditorChanged { field, value })
            }))
            .width(Length::Fill),
    ]
    .spacing(10)
    .into()
}
