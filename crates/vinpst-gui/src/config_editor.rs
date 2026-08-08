//! Control-page typed config draft updates and editor rendering.

use crate::audio_devices::{AudioDeviceState, capture_device_choices};
use crate::keyboard_action::{keyboard_action, keyboard_button, keyboard_select};

use iced::{
    Element, Length,
    widget::{checkbox, column, pick_list, row, slider, text},
};

use crate::{App, ConfigDocument, ConfigDraft, ConfigDraftMessage, GuiText, Message};

impl App {
    pub(super) fn update_config_draft(&mut self, message: ConfigDraftMessage) {
        match message {
            ConfigDraftMessage::DefaultLanguage(value) => {
                self.update_draft(|draft| draft.default_language = value);
            }
            ConfigDraftMessage::CaptureDevice(value) => {
                self.update_draft(|draft| draft.capture_device = value);
            }
            ConfigDraftMessage::NormalizeAudio(value) => {
                self.update_draft(|draft| draft.normalize_audio = value);
            }
            ConfigDraftMessage::InputGain(value) => {
                self.update_draft(|draft| draft.input_gain = value);
            }
            ConfigDraftMessage::DuckOutput(value) => {
                self.update_draft(|draft| draft.duck_output_while_recording = value);
            }
            ConfigDraftMessage::DuckVolume(value) => {
                self.update_draft(|draft| {
                    if draft.duck_output_while_recording {
                        draft.duck_output_volume = value;
                    }
                });
            }
            ConfigDraftMessage::VadEnabled(value) => {
                self.update_draft(|draft| draft.vad_enabled = value);
            }
            ConfigDraftMessage::VadThreshold(value) => {
                self.update_draft(|draft| draft.vad_threshold = value);
            }
            ConfigDraftMessage::ActiveProvider(value) => {
                self.update_draft(|draft| draft.active_provider = value);
            }
            ConfigDraftMessage::ActiveScene(value) => {
                self.update_draft(|draft| draft.active_scene = value);
            }
        }
    }

    fn update_draft(&mut self, update: impl FnOnce(&mut ConfigDraft)) {
        if let Some(draft) = &mut self.draft {
            update(draft);
        }
    }

    pub(super) fn config_editor(&self, busy: bool) -> Element<'_, Message> {
        match (&self.config, &self.draft) {
            (Ok(document), Some(draft)) => self.loaded_config_editor(document, draft, busy),
            (Err(error), _) => text(self.locale.config_error(error)).into(),
            (Ok(_), None) => text(self.locale.text(GuiText::ConfigDraftUnavailable)).into(),
        }
    }

    fn loaded_config_editor<'a>(
        &'a self,
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        column![
            text(self.locale.text(GuiText::AudioAndVad)).size(22),
            self.audio_vad_editor(draft, busy),
            self.config_save_controls(document, draft, busy),
        ]
        .spacing(12)
        .into()
    }

    fn audio_vad_editor(&self, draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        let input_gain_control = self.input_gain_control(draft, busy);
        let normalize_action = (!busy).then_some(Message::ConfigDraft(
            ConfigDraftMessage::NormalizeAudio(!draft.normalize_audio),
        ));
        let normalize_checkbox = keyboard_action(
            checkbox(draft.normalize_audio)
                .label(self.locale.text(GuiText::NormalizeAudio))
                .on_toggle_maybe((!busy).then_some(|value| {
                    Message::ConfigDraft(ConfigDraftMessage::NormalizeAudio(value))
                })),
            normalize_action,
        );
        let duck_action = (!busy).then_some(Message::ConfigDraft(ConfigDraftMessage::DuckOutput(
            !draft.duck_output_while_recording,
        )));
        let duck_checkbox = keyboard_action(
            checkbox(draft.duck_output_while_recording)
                .label(self.locale.text(GuiText::DuckOutput))
                .on_toggle_maybe((!busy).then_some(|value| {
                    Message::ConfigDraft(ConfigDraftMessage::DuckOutput(value))
                })),
            duck_action,
        );
        let vad_action = (!busy).then_some(Message::ConfigDraft(ConfigDraftMessage::VadEnabled(
            !draft.vad_enabled,
        )));
        let vad_checkbox = keyboard_action(
            checkbox(draft.vad_enabled)
                .label(self.locale.text(GuiText::EnableVad))
                .on_toggle_maybe((!busy).then_some(|value| {
                    Message::ConfigDraft(ConfigDraftMessage::VadEnabled(value))
                })),
            vad_action,
        );
        let mut body = column![
            self.capture_device_section(draft, busy),
            normalize_checkbox,
            row![
                text(self.locale.input_gain(draft.input_gain)).width(180),
                input_gain_control,
            ]
            .spacing(12),
            duck_checkbox,
        ]
        .spacing(12);
        if draft.duck_output_while_recording {
            body = body.push(
                row![
                    text(self.locale.duck_volume(draft.duck_output_volume * 100.0),).width(180),
                    self.duck_volume_control(draft, busy),
                ]
                .spacing(12),
            );
        }
        body.push(vad_checkbox).into()
    }

    fn capture_device_section(&self, draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        let mut section = column![
            row![
                text(self.locale.text(GuiText::CaptureDevice)).width(180),
                self.capture_device_control(draft, busy),
            ]
            .spacing(12)
        ]
        .spacing(8);
        if matches!(&self.audio_devices, AudioDeviceState::Failed(_)) {
            section = section.push(
                row![
                    text(self.locale.text(GuiText::AudioDevicesUnavailable)),
                    keyboard_button(self.locale.text(GuiText::Retry))
                        .on_press_maybe((!busy).then_some(Message::RefreshAudioDevices)),
                ]
                .spacing(10),
            );
        }
        section.into()
    }

    fn capture_device_control(&self, draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        if busy {
            return text(draft.capture_device.clone())
                .width(Length::Fill)
                .into();
        }
        let choices = match &self.audio_devices {
            AudioDeviceState::Ready(choices) => choices.clone(),
            AudioDeviceState::Loading | AudioDeviceState::Failed(_) => capture_device_choices(
                &draft.capture_device,
                self.locale.default_capture_device(),
                &[],
            ),
        };
        let selected = choices
            .iter()
            .find(|choice| choice.value == draft.capture_device)
            .cloned();
        pick_list(choices, selected, |choice| {
            Message::ConfigDraft(ConfigDraftMessage::CaptureDevice(choice.value))
        })
        .width(Length::Fill)
        .into()
    }

    fn input_gain_control(&self, draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        if busy {
            return text(self.locale.text(GuiText::LockedWhileFinishing))
                .width(Length::Fill)
                .into();
        }
        let previous = (draft.input_gain > 0.1).then(|| {
            Message::ConfigDraft(ConfigDraftMessage::InputGain(
                (draft.input_gain - 0.1).max(0.1),
            ))
        });
        let next = (draft.input_gain < 10.0).then(|| {
            Message::ConfigDraft(ConfigDraftMessage::InputGain(
                (draft.input_gain + 0.1).min(10.0),
            ))
        });
        keyboard_select(
            slider(0.1_f32..=10.0_f32, draft.input_gain, |value| {
                Message::ConfigDraft(ConfigDraftMessage::InputGain(value))
            })
            .step(0.1_f32)
            .width(Length::Fill),
            previous,
            next,
        )
    }

    fn duck_volume_control(&self, draft: &ConfigDraft, busy: bool) -> Element<'_, Message> {
        if busy {
            return text(self.locale.text(GuiText::LockedWhileFinishing))
                .width(Length::Fill)
                .into();
        }
        let previous = (draft.duck_output_volume > 0.0).then(|| {
            Message::ConfigDraft(ConfigDraftMessage::DuckVolume(
                (draft.duck_output_volume - 0.05).max(0.0),
            ))
        });
        let next = (draft.duck_output_volume < 1.0).then(|| {
            Message::ConfigDraft(ConfigDraftMessage::DuckVolume(
                (draft.duck_output_volume + 0.05).min(1.0),
            ))
        });
        keyboard_select(
            slider(0.0_f32..=1.0_f32, draft.duck_output_volume, |value| {
                Message::ConfigDraft(ConfigDraftMessage::DuckVolume(value))
            })
            .step(0.05_f32)
            .width(Length::Fill),
            previous,
            next,
        )
    }

    fn config_save_controls<'a>(
        &'a self,
        document: &'a ConfigDocument,
        draft: &'a ConfigDraft,
        busy: bool,
    ) -> Element<'a, Message> {
        let dirty = draft.is_dirty(&document.config);
        row![
            keyboard_button(self.locale.text(GuiText::SaveConfiguration))
                .on_press_maybe((dirty && !busy).then_some(Message::SaveConfig)),
            keyboard_button(self.locale.text(GuiText::ResetChanges))
                .on_press_maybe((dirty && !busy).then_some(Message::ResetConfigDraft)),
            text(if dirty {
                self.locale.text(GuiText::UnsavedChanges)
            } else {
                self.locale.text(GuiText::ConfigurationUpToDate)
            }),
        ]
        .spacing(10)
        .into()
    }
}
