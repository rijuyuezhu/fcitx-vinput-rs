//! Keyboard interaction and audited desktop capability reporting.

use iced::{
    Subscription, Task,
    advanced::widget::{operate, operation::focusable},
    event,
    keyboard::{self, Key, Modifiers, key},
    widget::operation,
};
use serde_json::{Value, json};

use crate::{App, Message, Page};

/// Keyboard-only interactions owned by the application shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMessage {
    /// Clear focus from every control and restart traversal from a known state.
    ClearFocus,
    /// Focus the active registry workflow input or primary action.
    FocusRegistryWorkflow,
    /// Move focus to the next enabled control.
    FocusNext,
    /// Move focus to the previous enabled control.
    FocusPrevious,
    /// Select one top-level page through a stable command shortcut.
    SelectPage(Page),
}

/// Listens to ignored shell keys and the captured F6 focus shortcut.
pub(crate) fn subscription() -> Subscription<Message> {
    event::listen_with(keyboard_event_message)
}

pub(crate) fn capability_snapshot() -> Value {
    json!({
        "toolkit": "iced-0.14",
        "accessibility_tree": {
            "available": false,
            "status": "blocked-by-toolkit",
        },
        "assistive_technology": {
            "screen_reader_supported": false,
            "release_policy": "unsupported-in-0.1.0",
            "fallbacks": {
                "management_command": "vinpst",
                "fcitx_configuration_command": "fcitx5-configtool",
                "fcitx_configuration_file": "fcitx5/conf/vinpst.conf under XDG_CONFIG_HOME",
                "fcitx_reload_command": "fcitx5-remote --check -r",
            },
        },
        "keyboard": {
            "tab_focus_traversal": true,
            "focus_reset": "Escape",
            "registry_workflow_focus": "F6",
            "focus_scope": "all-enabled-controls",
            "button_focus_traversal": true,
            "button_activation": ["Enter", "Space"],
            "checkbox_activation": ["Enter", "Space"],
            "selector_adjustment": ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"],
            "slider_adjustment": ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"],
            "page_shortcuts": ["Command+1", "Command+2", "Command+3", "Command+4"],
        },
        "input_method": {
            "preedit_commit": true,
            "backends": ["wayland", "x11"],
        },
        "clipboard": {
            "standard_text_editing": true,
            "backends": ["wayland", "x11"],
        },
    })
}

impl App {
    pub(crate) fn handle_interaction_message(
        &mut self,
        message: InteractionMessage,
    ) -> Task<Message> {
        if self.has_error_dialog() {
            if message == InteractionMessage::ClearFocus {
                self.dismiss_error();
            }
            return Task::none();
        }
        if self.has_resource_detail() {
            if message == InteractionMessage::ClearFocus {
                self.clear_resource_detail();
            }
            return Task::none();
        }
        match message {
            InteractionMessage::ClearFocus => operate(focusable::unfocus()),
            InteractionMessage::FocusRegistryWorkflow => self
                .script_install
                .primary_action_focus_id()
                .map_or_else(operation::focus_next, operation::focus),
            InteractionMessage::FocusNext => operation::focus_next(),
            InteractionMessage::FocusPrevious => operation::focus_previous(),
            InteractionMessage::SelectPage(page) => {
                self.select_page(page);
                Task::none()
            }
        }
    }
}

fn keyboard_event_message(
    event: iced::Event,
    status: event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    let iced::Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modifiers,
        repeat,
        ..
    }) = event
    else {
        return None;
    };
    let interaction = interaction_for_key(&key, modifiers, repeat)?;
    match status {
        event::Status::Ignored => Some(Message::Interaction(interaction)),
        event::Status::Captured if interaction == InteractionMessage::FocusRegistryWorkflow => {
            Some(Message::Interaction(interaction))
        }
        event::Status::Captured => None,
    }
}

fn interaction_for_key(
    key: &Key,
    modifiers: Modifiers,
    repeat: bool,
) -> Option<InteractionMessage> {
    if repeat {
        return None;
    }
    match key.as_ref() {
        Key::Named(key::Named::Escape) if modifiers == Modifiers::NONE => {
            Some(InteractionMessage::ClearFocus)
        }
        Key::Named(key::Named::F6) if modifiers == Modifiers::NONE => {
            Some(InteractionMessage::FocusRegistryWorkflow)
        }
        Key::Named(key::Named::Tab) if modifiers == Modifiers::NONE => {
            Some(InteractionMessage::FocusNext)
        }
        Key::Named(key::Named::Tab) if modifiers == Modifiers::SHIFT => {
            Some(InteractionMessage::FocusPrevious)
        }
        Key::Character("1") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Control))
        }
        Key::Character("2") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Resources))
        }
        Key::Character("3") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Llm))
        }
        Key::Character("4") if modifiers == Modifiers::COMMAND => {
            Some(InteractionMessage::SelectPage(Page::Hotwords))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_keyboard_shortcuts_map_without_stealing_text_editing_commands() {
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Escape), Modifiers::NONE, false,),
            Some(InteractionMessage::ClearFocus)
        );
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Tab), Modifiers::NONE, false,),
            Some(InteractionMessage::FocusNext)
        );
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Tab), Modifiers::SHIFT, false,),
            Some(InteractionMessage::FocusPrevious)
        );
        assert_eq!(
            interaction_for_key(&Key::Character("3".into()), Modifiers::COMMAND, false,),
            Some(InteractionMessage::SelectPage(Page::Llm))
        );
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::F6), Modifiers::NONE, false,),
            Some(InteractionMessage::FocusRegistryWorkflow)
        );
        assert_eq!(
            interaction_for_key(&Key::Character("c".into()), Modifiers::COMMAND, false,),
            None
        );
        assert_eq!(
            interaction_for_key(&Key::Named(key::Named::Tab), Modifiers::CTRL, false,),
            None
        );
        assert_eq!(
            interaction_for_key(&Key::Character("1".into()), Modifiers::COMMAND, true,),
            None
        );
    }

    #[test]
    fn page_shortcuts_obey_busy_guards_while_focus_traversal_remains_available() {
        assert!(
            Message::Interaction(InteractionMessage::SelectPage(Page::Resources))
                .blocked_while_busy()
        );
        assert!(!Message::Interaction(InteractionMessage::ClearFocus).blocked_while_busy());
        assert!(
            !Message::Interaction(InteractionMessage::FocusRegistryWorkflow).blocked_while_busy()
        );
        assert!(!Message::Interaction(InteractionMessage::FocusNext).blocked_while_busy());
        assert!(!Message::Interaction(InteractionMessage::FocusPrevious).blocked_while_busy());
    }

    #[test]
    fn dynamic_title_distinguishes_pages_and_locales() {
        let mut app = crate::test_support::GuiHarness::new();
        let control_title = app.window_title();
        app.page = Page::Resources;
        let resources_title = app.window_title();
        assert_ne!(control_title, resources_title);
        app.locale = crate::GuiLocale::ZhCn;
        assert_ne!(resources_title, app.window_title());
    }

    #[test]
    fn capability_snapshot_reports_supported_and_blocked_boundaries() {
        let snapshot = capability_snapshot();
        assert_eq!(snapshot["accessibility_tree"]["available"], false);
        assert_eq!(
            snapshot["assistive_technology"]["screen_reader_supported"],
            false
        );
        assert_eq!(
            snapshot["assistive_technology"]["release_policy"],
            "unsupported-in-0.1.0"
        );
        assert_eq!(
            snapshot["assistive_technology"]["fallbacks"]["management_command"],
            "vinpst"
        );
        assert_eq!(
            snapshot["assistive_technology"]["fallbacks"]["fcitx_configuration_command"],
            "fcitx5-configtool"
        );
        assert_eq!(
            snapshot["assistive_technology"]["fallbacks"]["fcitx_configuration_file"],
            "fcitx5/conf/vinpst.conf under XDG_CONFIG_HOME"
        );
        assert_eq!(
            snapshot["assistive_technology"]["fallbacks"]["fcitx_reload_command"],
            "fcitx5-remote --check -r"
        );
        assert_eq!(snapshot["keyboard"]["tab_focus_traversal"], true);
        assert_eq!(snapshot["keyboard"]["focus_reset"], "Escape");
        assert_eq!(snapshot["keyboard"]["registry_workflow_focus"], "F6");
        assert_eq!(snapshot["keyboard"]["focus_scope"], "all-enabled-controls");
        assert_eq!(snapshot["keyboard"]["button_focus_traversal"], true);
        assert_eq!(snapshot["keyboard"]["button_activation"][0], "Enter");
        assert_eq!(snapshot["keyboard"]["selector_adjustment"][1], "ArrowDown");
        assert_eq!(snapshot["input_method"]["preedit_commit"], true);
        assert_eq!(snapshot["clipboard"]["standard_text_editing"], true);
    }
}
