//! Typed confirmation state for destructive management actions.

use std::path::PathBuf;

use iced::{
    Element, Length, Task,
    widget::{column, container, opaque, row, text},
};

use crate::{
    AdapterConfigMessage, App, AsrProviderMessage, GuiLocale, GuiText, InteractionMessage,
    LlmProviderMessage, Message, SceneMessage, keyboard_action::keyboard_button,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RemovalTarget {
    InstalledModel(PathBuf),
    AsrProvider { id: String, managed: bool },
    TextAdapter { id: String, managed: bool },
    LlmProvider(String),
    Scene(String),
}

impl RemovalTarget {
    fn execution_message(self) -> Message {
        match self {
            Self::InstalledModel(path) => Message::RemoveInstalledModel(path),
            Self::AsrProvider { id, managed: true } => Message::RemoveProvider(id),
            Self::AsrProvider { id, managed: false } => {
                Message::AsrProvider(AsrProviderMessage::Remove(id))
            }
            Self::TextAdapter { id, managed: true } => Message::RemoveAdapter(id),
            Self::TextAdapter { id, managed: false } => {
                Message::AdapterConfig(AdapterConfigMessage::Remove(id))
            }
            Self::LlmProvider(id) => Message::LlmProvider(LlmProviderMessage::Remove(id)),
            Self::Scene(id) => Message::Scene(SceneMessage::Remove(id)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemovalConfirmation {
    target: RemovalTarget,
}

impl App {
    pub(super) fn request_removal(&mut self, target: RemovalTarget) {
        if self.is_busy() {
            return;
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = crate::OperationState::Failed(error);
            return;
        }
        if self.scene_editor.is_some()
            || self.asr_provider_editor.is_some()
            || self.llm_provider_editor.is_some()
            || self.adapter_config_editor.is_some()
        {
            return;
        }
        if matches!(target, RemovalTarget::AsrProvider { .. })
            && !self.guard_hotword_changes("removing an ASR provider")
        {
            return;
        }
        self.removal_confirmation = Some(RemovalConfirmation { target });
    }

    pub(super) fn intercept_removal_confirmation_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        let request = match message {
            Message::RequestRemoveInstalledModel(path) => {
                Some(RemovalTarget::InstalledModel(path.clone()))
            }
            Message::RequestRemoveAsrProvider { id, managed } => Some(RemovalTarget::AsrProvider {
                id: id.clone(),
                managed: *managed,
            }),
            Message::RequestRemoveTextAdapter { id, managed } => Some(RemovalTarget::TextAdapter {
                id: id.clone(),
                managed: *managed,
            }),
            Message::RequestRemoveLlmProvider(id) => Some(RemovalTarget::LlmProvider(id.clone())),
            Message::RequestRemoveScene(id) => Some(RemovalTarget::Scene(id.clone())),
            _ => None,
        };
        if let Some(target) = request {
            self.request_removal(target);
            return Some(Task::none());
        }
        let pending = self.removal_confirmation.is_some();
        match message {
            Message::ConfirmRemoval if pending => {
                let target = self
                    .removal_confirmation
                    .take()
                    .expect("pending removal checked above")
                    .target;
                Some(self.update(target.execution_message()))
            }
            Message::CancelRemoval | Message::Interaction(InteractionMessage::ClearFocus)
                if pending =>
            {
                self.removal_confirmation = None;
                Some(Task::none())
            }
            _ => None,
        }
    }

    pub(super) fn removal_confirmation_view(&self) -> Option<Element<'_, Message>> {
        let confirmation = self.removal_confirmation.as_ref()?;
        let question = self.removal_question(&confirmation.target);
        let dialog = container(
            column![
                text(self.locale.text(GuiText::Remove)).size(24),
                text(question),
                row![
                    keyboard_button(self.locale.text(GuiText::Remove))
                        .on_press(Message::ConfirmRemoval),
                    keyboard_button(self.locale.text(GuiText::Cancel))
                        .on_press(Message::CancelRemoval),
                ]
                .spacing(10),
            ]
            .spacing(16),
        )
        .padding(24)
        .max_width(560)
        .style(container::rounded_box);

        Some(opaque(
            container(dialog)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        ))
    }

    fn removal_question(&self, target: &RemovalTarget) -> String {
        match target {
            RemovalTarget::InstalledModel(path) => {
                let label = self
                    .installed_models
                    .as_ref()
                    .ok()
                    .and_then(|models| models.iter().find(|model| model.model_dir == *path))
                    .map_or_else(
                        || {
                            path.file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("model")
                                .to_owned()
                        },
                        |model| {
                            let locales = [self.locale.code().to_owned()];
                            model
                                .display_title(&locales)
                                .unwrap_or_else(|| model.stable_model_id())
                                .to_owned()
                        },
                    );
                self.locale.remove_model_question(&label)
            }
            RemovalTarget::AsrProvider { id, managed } => {
                self.locale.remove_asr_provider_question(id, *managed)
            }
            RemovalTarget::TextAdapter { id, managed } => {
                self.locale.remove_text_adapter_question(id, *managed)
            }
            RemovalTarget::LlmProvider(id) => {
                let scene_count = self.config.as_ref().map_or(0, |document| {
                    document
                        .config
                        .scenes
                        .definitions
                        .iter()
                        .filter(|scene| scene.provider_id.as_deref() == Some(id))
                        .count()
                });
                self.locale.remove_llm_provider_question(id, scene_count)
            }
            RemovalTarget::Scene(id) => self.locale.remove_scene_question(id),
        }
    }
}

impl GuiLocale {
    fn remove_model_question(self, label: &str) -> String {
        match self {
            Self::EnUs => format!("Remove model “{label}” from this device?"),
            Self::ZhCn => format!("从此设备移除模型“{label}”？"),
        }
    }

    fn remove_asr_provider_question(self, id: &str, managed: bool) -> String {
        match (self, managed) {
            (Self::EnUs, true) => {
                format!("Remove ASR provider “{id}” and its installed script?")
            }
            (Self::EnUs, false) => format!("Remove ASR provider “{id}”?"),
            (Self::ZhCn, true) => format!("移除 ASR 提供商“{id}”及其已安装脚本？"),
            (Self::ZhCn, false) => format!("移除 ASR 提供商“{id}”？"),
        }
    }

    fn remove_text_adapter_question(self, id: &str, managed: bool) -> String {
        match (self, managed) {
            (Self::EnUs, true) => {
                format!("Remove LLM adapter “{id}” and its installed script?")
            }
            (Self::EnUs, false) => format!("Remove LLM adapter “{id}”?"),
            (Self::ZhCn, true) => format!("移除 LLM 适配器“{id}”及其已安装脚本？"),
            (Self::ZhCn, false) => format!("移除 LLM 适配器“{id}”？"),
        }
    }

    fn remove_llm_provider_question(self, id: &str, scene_count: usize) -> String {
        match (self, scene_count) {
            (Self::EnUs, 0) => format!("Remove LLM provider “{id}”?"),
            (Self::EnUs, count) => format!(
                "Remove LLM provider “{id}”? {count} scene(s) will keep their definitions, but their provider and model choices will be cleared."
            ),
            (Self::ZhCn, 0) => format!("移除 LLM 提供商“{id}”？"),
            (Self::ZhCn, count) => format!(
                "移除 LLM 提供商“{id}”？{count} 个场景会保留，但其中的提供商和模型选择将被清除。"
            ),
        }
    }

    fn remove_scene_question(self, id: &str) -> String {
        match self {
            Self::EnUs => format!("Remove scene “{id}”?"),
            Self::ZhCn => format!("移除场景“{id}”？"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vinpst_config::{LlmProviderConfig, VinpstConfig};

    #[test]
    fn removal_targets_map_back_to_existing_validated_execution_messages() {
        assert!(matches!(
            RemovalTarget::AsrProvider {
                id: "managed".to_owned(),
                managed: true,
            }
            .execution_message(),
            Message::RemoveProvider(id) if id == "managed"
        ));
        assert!(matches!(
            RemovalTarget::AsrProvider {
                id: "custom".to_owned(),
                managed: false,
            }
            .execution_message(),
            Message::AsrProvider(AsrProviderMessage::Remove(id)) if id == "custom"
        ));
        assert!(matches!(
            RemovalTarget::TextAdapter {
                id: "custom-adapter".to_owned(),
                managed: false,
            }
            .execution_message(),
            Message::AdapterConfig(AdapterConfigMessage::Remove(id)) if id == "custom-adapter"
        ));
        assert!(matches!(
            RemovalTarget::LlmProvider("cloud".to_owned()).execution_message(),
            Message::LlmProvider(LlmProviderMessage::Remove(id)) if id == "cloud"
        ));
        assert!(matches!(
            RemovalTarget::Scene("rewrite".to_owned()).execution_message(),
            Message::Scene(SceneMessage::Remove(id)) if id == "rewrite"
        ));
    }

    #[test]
    fn cancellation_and_escape_clear_pending_removal_without_execution() {
        let mut app = crate::test_support::GuiHarness::new();
        app.request_removal(RemovalTarget::Scene("rewrite".to_owned()));
        assert!(app.removal_confirmation.is_some());
        app.send(Message::CancelRemoval);
        assert!(app.removal_confirmation.is_none());

        app.request_removal(RemovalTarget::LlmProvider("cloud".to_owned()));
        assert!(app.removal_confirmation.is_some());
        app.send(Message::Interaction(InteractionMessage::ClearFocus));
        assert!(app.removal_confirmation.is_none());
    }

    #[test]
    fn confirmation_reenters_the_existing_removal_path() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.llm.providers.push(LlmProviderConfig {
            id: "cloud".to_owned(),
            base_url: "https://example.invalid/v1".to_owned(),
            api_key: String::new(),
            model: Some("model".to_owned()),
            extra_body: serde_json::json!({}),
            extra: std::collections::HashMap::new(),
        });
        let mut app = crate::test_support::GuiHarness::with_config(
            config,
            "/tmp/vinpst-removal-confirmation.json",
            crate::Page::Llm,
        );
        app.send(Message::RequestRemoveLlmProvider("cloud".to_owned()));
        assert!(app.removal_confirmation.is_some());

        app.send(Message::ConfirmRemoval);

        assert!(app.removal_confirmation.is_none());
        assert!(matches!(app.operation, crate::OperationState::Running(_)));
    }
}
