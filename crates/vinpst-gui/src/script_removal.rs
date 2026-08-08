//! Guarded provider and adapter removal tasks for the GUI.

use iced::Task;
use vinpst_registry::LiveScriptKind;

use crate::{
    App, GuiText, Message, OperationState, load_config_document,
    script_management::remove_managed_script_entry,
};

impl App {
    pub(crate) fn intercept_script_removal_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        match message {
            Message::RemoveProvider(id) => {
                Some(self.begin_script_remove(LiveScriptKind::AsrProvider, id.clone()))
            }
            Message::RemoveAdapter(id) => {
                Some(self.begin_script_remove(LiveScriptKind::LlmAdapter, id.clone()))
            }
            Message::ScriptRemoved(result) => Some(self.finish_script_remove(result.clone())),
            _ => None,
        }
    }

    pub(crate) fn begin_script_remove(
        &mut self,
        kind: LiveScriptKind,
        id: String,
    ) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_llm_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_asr_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if kind == LiveScriptKind::AsrProvider
            && !self.guard_hotword_changes("removing an ASR provider")
        {
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        if self.is_busy() {
            return Task::none();
        }
        self.operation = OperationState::Running(self.locale.text(match kind {
            LiveScriptKind::AsrProvider => GuiText::RemovingProvider,
            LiveScriptKind::LlmAdapter => GuiText::RemovingAdapter,
        }));
        let document = document.clone();
        let locale = self.locale;
        crate::blocking_task::perform(
            "vinpst-gui-script-remove",
            move || remove_managed_script_entry(&document, kind, &id, locale),
            move |result| {
                Message::ScriptRemoved(result.unwrap_or_else(|_| {
                    let resource = locale.text(match kind {
                        LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
                        LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
                    });
                    Err(locale.script_removal_worker_failed(resource))
                }))
            },
        )
    }

    pub(crate) fn finish_script_remove(&mut self, result: Result<String, String>) -> Task<Message> {
        match result {
            Ok(summary) => {
                let path = self
                    .config
                    .as_ref()
                    .ok()
                    .map(|document| document.path.clone());
                self.replace_config(load_config_document(path.as_deref()));
                self.operation = OperationState::Succeeded(summary);
            }
            Err(error) => self.operation = OperationState::Failed(error),
        }
        self.begin_daemon_refresh(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HotwordMessage, SecretInput};

    #[test]
    fn managed_asr_removal_cannot_discard_unsaved_hotword_changes() {
        let mut app = crate::test_support::GuiHarness::new();
        app.send(Message::Hotword(HotwordMessage::PathChanged(
            SecretInput::new("/tmp/pending-hotwords.txt".to_owned()),
        )));

        app.send(Message::RemoveProvider("managed-provider".to_owned()));

        assert!(matches!(app.operation, crate::OperationState::Failed(_)));
    }
}
