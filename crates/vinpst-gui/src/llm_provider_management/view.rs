//! Localized LLM provider list, connectivity controls, and editor rendering.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, row, text, text_input},
};
use vinpst_config::redact_url_for_diagnostics;

use super::{
    LlmProviderEditorField, LlmProviderEditorState, LlmProviderMessage, extra_body_input_is_secure,
    llm_provider_test_target,
};
use crate::{App, GuiLocale, GuiText, Message, SecretInput};

impl App {
    pub(crate) fn llm_provider_management_view(&self, busy: bool) -> Element<'_, Message> {
        let editor_open = self.llm_provider_editor.is_some();
        let test_input_enabled = !busy && !editor_open;
        let mut body = column![
            row![
                text(self.locale.text(GuiText::ProvidersTitle))
                    .size(22)
                    .width(Length::Fill),
                keyboard_button(self.locale.text(GuiText::AddProvider)).on_press_maybe(
                    (!busy && !editor_open)
                        .then_some(Message::LlmProvider(LlmProviderMessage::BeginAdd)),
                ),
            ]
            .spacing(10),
            row![
                text(self.locale.text(GuiText::TestInput)).width(160),
                text_input(
                    self.locale.text(GuiText::TestInputPlaceholder),
                    self.llm_provider_test_text.as_str()
                )
                .on_input_maybe(test_input_enabled.then_some(|value| {
                    Message::LlmProvider(LlmProviderMessage::TestInputChanged(SecretInput::new(
                        value,
                    )))
                }))
                .width(Length::Fill),
            ]
            .spacing(10),
        ]
        .spacing(10);

        match &self.config {
            Ok(document) => {
                for provider in &document.config.llm.providers {
                    let test_target = llm_provider_test_target(&document.config, &provider.id).ok();
                    let model = test_target
                        .as_ref()
                        .and_then(|provider| provider.model.as_deref())
                        .unwrap_or_else(|| self.locale.text(GuiText::NotConfigured));
                    let endpoint = if provider.base_url.is_empty() {
                        self.locale.text(GuiText::AdapterLocal).to_owned()
                    } else {
                        redact_url_for_diagnostics(&provider.base_url)
                    };
                    body = body.push(llm_provider_row(
                        self.locale,
                        format!("{} · {} · {}", provider.id, model, endpoint),
                        &provider.id,
                        !busy && !editor_open,
                        !self.llm_provider_test_text.as_str().trim().is_empty(),
                        test_target.is_some(),
                    ));
                }
                if document.config.llm.providers.is_empty() {
                    body = body.push(text(self.locale.text(GuiText::NoLlmProviders)));
                }
            }
            Err(error) => body = body.push(text(self.locale.config_error(error))),
        }

        if let Some(editor) = &self.llm_provider_editor {
            body = body.push(llm_provider_editor_view(self.locale, editor, busy));
        }
        body.into()
    }
}

fn llm_provider_row(
    locale: GuiLocale,
    label: String,
    provider_id: &str,
    controls_enabled: bool,
    test_input_present: bool,
    test_target_available: bool,
) -> Element<'static, Message> {
    row![
        text(label).width(Length::Fill),
        keyboard_button(locale.text(GuiText::Details))
            .on_press(Message::SelectLlmProviderDetail(provider_id.to_owned())),
        keyboard_button(locale.text(GuiText::Test)).on_press_maybe(
            (controls_enabled && test_input_present && test_target_available).then_some(
                Message::LlmProvider(LlmProviderMessage::Test(provider_id.to_owned())),
            ),
        ),
        keyboard_button(locale.text(GuiText::Edit)).on_press_maybe(controls_enabled.then_some(
            Message::LlmProvider(LlmProviderMessage::BeginEdit(provider_id.to_owned())),
        )),
        keyboard_button(locale.text(GuiText::Remove)).on_press_maybe(
            controls_enabled.then_some(Message::RequestRemoveLlmProvider(provider_id.to_owned()),)
        ),
    ]
    .spacing(10)
    .into()
}

fn llm_provider_editor_view(
    locale: GuiLocale,
    editor: &LlmProviderEditorState,
    busy: bool,
) -> Element<'_, Message> {
    let adding = editor.original_id.is_none();
    let action = if adding {
        GuiText::AddProvider
    } else {
        GuiText::UpdateProvider
    };
    let id_field: Element<'_, Message> = if adding {
        labeled_input(
            locale.text(GuiText::ProviderId),
            locale.text(GuiText::StableUniqueId),
            &editor.fields.id,
            LlmProviderEditorField::Id,
            false,
        )
    } else {
        text(locale.provider_id_immutable(&editor.fields.id)).into()
    };
    let dirty = editor.is_dirty();
    column![
        text(locale.text(action)).size(22),
        id_field,
        labeled_input(
            locale.text(GuiText::BaseUrl),
            locale.text(GuiText::BaseUrlPlaceholder),
            &editor.fields.base_url,
            LlmProviderEditorField::BaseUrl,
            editor.base_url_secure,
        ),
        labeled_input(
            locale.text(GuiText::ApiKey),
            locale.text(GuiText::OptionalKeyExpression),
            editor.fields.api_key.as_str(),
            LlmProviderEditorField::ApiKey,
            true,
        ),
        labeled_input(
            locale.text(GuiText::DefaultModel),
            locale.text(GuiText::OptionalModelId),
            &editor.fields.model,
            LlmProviderEditorField::Model,
            false,
        ),
        labeled_input(
            locale.text(GuiText::ExtraBody),
            locale.text(GuiText::MaskedJsonObjectBlank),
            &editor.fields.extra_body,
            LlmProviderEditorField::ExtraBody,
            extra_body_input_is_secure(),
        ),
        row![
            keyboard_button(locale.text(action)).on_press_maybe(
                (dirty && !busy).then_some(Message::LlmProvider(LlmProviderMessage::Save)),
            ),
            keyboard_button(locale.text(GuiText::ResetForm)).on_press_maybe(
                (dirty && !busy).then_some(Message::LlmProvider(LlmProviderMessage::ResetEdit)),
            ),
            keyboard_button(locale.text(GuiText::Cancel)).on_press_maybe(
                (!busy).then_some(Message::LlmProvider(LlmProviderMessage::CancelEdit)),
            ),
            text(locale.text(if dirty {
                GuiText::UnsavedProviderChanges
            } else {
                GuiText::ProviderFormUnchanged
            })),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn labeled_input<'a>(
    label: &'static str,
    placeholder: &'static str,
    value: &'a str,
    field: LlmProviderEditorField,
    secure: bool,
) -> Element<'a, Message> {
    row![
        text(label).width(160),
        text_input(placeholder, value)
            .secure(secure)
            .on_input(move |value| {
                Message::LlmProvider(LlmProviderMessage::EditorChanged {
                    field,
                    value: SecretInput::new(value),
                })
            })
            .width(Length::Fill),
    ]
    .spacing(10)
    .into()
}
