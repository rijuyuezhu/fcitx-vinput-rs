//! Top-level management GUI page identifiers.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, text},
};

use crate::{App, GuiLocale, GuiText, Message};

/// Main GUI pages matching the legacy management surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// Audio, ASR, and daemon controls.
    Control,
    /// Installable models, ASR providers, and LLM adapters.
    Resources,
    /// LLM providers, configured adapters, and scenes.
    Llm,
    /// Hotword file configuration.
    Hotwords,
}

impl Page {
    pub(crate) const ALL: [Self; 4] = [Self::Control, Self::Resources, Self::Llm, Self::Hotwords];

    pub(crate) const fn machine_label(self) -> &'static str {
        match self {
            Self::Control => "Control",
            Self::Resources => "Resources",
            Self::Llm => "LLM",
            Self::Hotwords => "Hotwords",
        }
    }

    pub(crate) const fn display_label(self, locale: GuiLocale) -> &'static str {
        locale.text(match self {
            Self::Control => GuiText::Control,
            Self::Resources => GuiText::Resources,
            Self::Llm => GuiText::Llm,
            Self::Hotwords => GuiText::Hotwords,
        })
    }
}

impl App {
    pub(crate) fn window_title(&self) -> String {
        format!(
            "{} — {}",
            self.locale.text(GuiText::ApplicationTitle),
            self.page.display_label(self.locale),
        )
    }

    pub(super) fn navigation_view(&self, busy: bool) -> Element<'_, Message> {
        let navigation = Page::ALL.into_iter().fold(
            column![text(self.locale.text(GuiText::ApplicationTitle)).size(24)].spacing(10),
            |navigation, page| {
                navigation.push(
                    keyboard_button(text(page.display_label(self.locale)))
                        .width(Length::Fill)
                        .on_press_maybe((!busy).then_some(Message::SelectPage(page))),
                )
            },
        );
        navigation.push(self.desktop_action_button(busy)).into()
    }

    pub(super) fn select_page(&mut self, page: Page) {
        if self.page == page {
            return;
        }
        if !self.guard_hotword_changes("leaving the Hotwords page") {
            return;
        }
        self.page = page;
        self.selected_resource = None;
        self.scene_editor = None;
        self.asr_provider_editor = None;
        self.llm_provider_editor = None;
        self.adapter_config_editor = None;
    }
}
