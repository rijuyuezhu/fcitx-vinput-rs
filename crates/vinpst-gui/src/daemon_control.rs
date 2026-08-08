//! Typed daemon Start/Stop/Restart lifecycle controls for the Control page.

use crate::keyboard_action::keyboard_button;

use std::fmt;

use iced::{Element, Task, widget::row};
use vinpst_daemon_control::{UserServiceAction, run_user_service_command, user_service_command};

use crate::{
    App, DaemonActionName, DaemonLoadState, DaemonSnapshot, GuiLocale, GuiText, Message,
    OperationState, daemon_state_from_poll, query_daemon_snapshot, query_daemon_snapshot_if_owned,
};

/// One daemon lifecycle action exposed by the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonControlAction {
    /// Activate the daemon through its D-Bus service.
    Start,
    /// Stop the systemd user service.
    Stop,
    /// Restart the systemd user service.
    Restart,
}

impl DaemonControlAction {
    const fn progress(self, locale: GuiLocale) -> &'static str {
        locale.text(match self {
            Self::Start => GuiText::StartingDaemon,
            Self::Stop => GuiText::StoppingDaemon,
            Self::Restart => GuiText::RestartingDaemon,
        })
    }

    const fn name(self) -> DaemonActionName {
        match self {
            Self::Start => DaemonActionName::Start,
            Self::Stop => DaemonActionName::Stop,
            Self::Restart => DaemonActionName::Restart,
        }
    }

    const fn expected_running(self) -> bool {
        !matches!(self, Self::Stop)
    }
}

/// One daemon lifecycle interaction handled by the Control page.
#[derive(Debug, Clone)]
pub enum DaemonControlMessage {
    /// Activate the daemon through D-Bus.
    Start,
    /// Stop the daemon user service.
    Stop,
    /// Restart the daemon user service.
    Restart,
    /// Complete one asynchronous daemon lifecycle action.
    Finished {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Action completed by the worker.
        action: DaemonControlAction,
        /// Secret-free typed outcome.
        result: Result<DaemonControlOutcome, DaemonControlFailure>,
    },
}

/// Post-action daemon state observed without accidentally activating a stopped service.
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonControlObservation {
    /// A current daemon owner returned a typed snapshot.
    Running(DaemonSnapshot),
    /// The well-known service had no owner.
    Stopped,
    /// The action succeeded but final owner state could not be queried.
    Unavailable,
}

/// Strength of the post-action state confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonControlConfirmation {
    /// The observed owner state matches the requested action.
    Confirmed,
    /// A current owner observation contradicted the requested state.
    NotConfirmed,
    /// The service action succeeded but owner state could not be queried.
    Unavailable,
}

/// Successful service action plus its non-activating final-state observation.
#[derive(Debug, Clone, PartialEq)]
pub struct DaemonControlOutcome {
    /// Final owner/runtime observation.
    pub observation: DaemonControlObservation,
    /// Whether the observation confirms the requested action.
    pub confirmation: DaemonControlConfirmation,
}

/// Fixed secret-free daemon lifecycle failure category.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DaemonControlFailure {
    /// D-Bus activation or status retrieval failed.
    ActivationFailed,
    /// The systemd user-service command could not be executed successfully.
    ServiceCommandFailed,
}

impl fmt::Debug for DaemonControlFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ActivationFailed => "ActivationFailed",
            Self::ServiceCommandFailed => "ServiceCommandFailed",
        })
    }
}

impl DaemonControlFailure {
    fn message(self, locale: GuiLocale, action: DaemonControlAction) -> String {
        match self {
            Self::ActivationFailed | Self::ServiceCommandFailed => {
                locale.daemon_action_failure(action.name())
            }
        }
    }
}

impl App {
    pub(super) fn daemon_control_actions(&self, busy: bool) -> Element<'_, Message> {
        let running = matches!(self.daemon, DaemonLoadState::Ready(_));
        let stopped = matches!(self.daemon, DaemonLoadState::Stopped);
        row![
            keyboard_button(self.locale.text(GuiText::RefreshDaemon))
                .on_press_maybe((!busy).then_some(Message::RefreshDaemon)),
            keyboard_button(self.locale.text(GuiText::StartDaemon)).on_press_maybe(
                (!busy && stopped).then_some(Message::DaemonControl(DaemonControlMessage::Start)),
            ),
            keyboard_button(self.locale.text(GuiText::StopDaemon)).on_press_maybe(
                (!busy && running).then_some(Message::DaemonControl(DaemonControlMessage::Stop)),
            ),
            keyboard_button(self.locale.text(GuiText::RestartDaemon)).on_press_maybe(
                (!busy && running).then_some(Message::DaemonControl(DaemonControlMessage::Restart)),
            ),
        ]
        .spacing(10)
        .into()
    }

    pub(super) fn intercept_daemon_control_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        let Message::DaemonControl(message) = message else {
            return None;
        };
        if self.is_busy() && !matches!(message, DaemonControlMessage::Finished { .. }) {
            return Some(Task::none());
        }
        Some(self.handle_daemon_control_message(message.clone()))
    }

    pub(super) fn handle_daemon_control_message(
        &mut self,
        message: DaemonControlMessage,
    ) -> Task<Message> {
        match message {
            DaemonControlMessage::Start => self.begin_daemon_control(DaemonControlAction::Start),
            DaemonControlMessage::Stop => self.begin_daemon_control(DaemonControlAction::Stop),
            DaemonControlMessage::Restart => {
                self.begin_daemon_control(DaemonControlAction::Restart)
            }
            DaemonControlMessage::Finished {
                operation_id,
                action,
                result,
            } => self.finish_daemon_control(operation_id, action, result),
        }
    }

    fn begin_daemon_control(&mut self, action: DaemonControlAction) -> Task<Message> {
        if self.active_daemon_control_id.is_some() || self.is_busy() {
            return Task::none();
        }
        let operation_id = self.next_daemon_control_id;
        self.next_daemon_control_id = self.next_daemon_control_id.wrapping_add(1).max(1);
        self.active_daemon_control_id = Some(operation_id);
        self.operation = OperationState::Running(action.progress(self.locale));
        crate::blocking_task::perform(
            "vinpst-gui-daemon-control",
            move || run_daemon_control(action),
            move |result| {
                Message::DaemonControl(DaemonControlMessage::Finished {
                    operation_id,
                    action,
                    result: result.unwrap_or_else(|_| {
                        Err(if action == DaemonControlAction::Start {
                            DaemonControlFailure::ActivationFailed
                        } else {
                            DaemonControlFailure::ServiceCommandFailed
                        })
                    }),
                })
            },
        )
    }

    fn finish_daemon_control(
        &mut self,
        operation_id: u64,
        action: DaemonControlAction,
        result: Result<DaemonControlOutcome, DaemonControlFailure>,
    ) -> Task<Message> {
        if self.active_daemon_control_id != Some(operation_id) {
            return Task::none();
        }
        self.active_daemon_control_id = None;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error.message(self.locale, action));
                return Task::none();
            }
        };
        match outcome.observation {
            DaemonControlObservation::Running(snapshot) => {
                self.daemon = DaemonLoadState::Ready(snapshot);
            }
            DaemonControlObservation::Stopped => {
                self.daemon = daemon_state_from_poll(Ok(None));
            }
            DaemonControlObservation::Unavailable => {}
        }
        self.operation = OperationState::Succeeded(match outcome.confirmation {
            DaemonControlConfirmation::Confirmed => self
                .locale
                .daemon_state_confirmed(action.expected_running()),
            DaemonControlConfirmation::NotConfirmed => {
                self.locale.daemon_action_unconfirmed(action.name(), false)
            }
            DaemonControlConfirmation::Unavailable => {
                self.locale.daemon_action_unconfirmed(action.name(), true)
            }
        });
        Task::none()
    }
}

fn run_daemon_control(
    action: DaemonControlAction,
) -> Result<DaemonControlOutcome, DaemonControlFailure> {
    if action == DaemonControlAction::Start {
        return query_daemon_snapshot()
            .map(|snapshot| DaemonControlOutcome {
                observation: DaemonControlObservation::Running(snapshot),
                confirmation: DaemonControlConfirmation::Confirmed,
            })
            .map_err(|_| DaemonControlFailure::ActivationFailed);
    }
    let service_action = match action {
        DaemonControlAction::Stop => UserServiceAction::Stop,
        DaemonControlAction::Restart => UserServiceAction::Restart,
        DaemonControlAction::Start => unreachable!("start handled through D-Bus activation"),
    };
    let command = user_service_command(service_action);
    let command_outcome = run_user_service_command(&command);
    if !command_outcome.ok {
        return Err(DaemonControlFailure::ServiceCommandFailed);
    }
    Ok(classify_owner_observation(
        action,
        query_daemon_snapshot_if_owned(),
    ))
}

fn classify_owner_observation(
    action: DaemonControlAction,
    observation: Result<Option<DaemonSnapshot>, String>,
) -> DaemonControlOutcome {
    match observation {
        Ok(Some(snapshot)) => DaemonControlOutcome {
            observation: DaemonControlObservation::Running(snapshot),
            confirmation: if action.expected_running() {
                DaemonControlConfirmation::Confirmed
            } else {
                DaemonControlConfirmation::NotConfirmed
            },
        },
        Ok(None) => DaemonControlOutcome {
            observation: DaemonControlObservation::Stopped,
            confirmation: if action.expected_running() {
                DaemonControlConfirmation::NotConfirmed
            } else {
                DaemonControlConfirmation::Confirmed
            },
        },
        Err(_) => DaemonControlOutcome {
            observation: DaemonControlObservation::Unavailable,
            confirmation: DaemonControlConfirmation::Unavailable,
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn snapshot() -> DaemonSnapshot {
        DaemonSnapshot {
            status: "idle".to_owned(),
            runtime: json!({"active_session": false}),
            text_adapters: vinpst_protocol::TextAdapterState::default(),
        }
    }

    #[test]
    fn owner_observation_confirms_expected_stop_and_restart_states() {
        let stopped = classify_owner_observation(DaemonControlAction::Stop, Ok(None));
        assert_eq!(stopped.confirmation, DaemonControlConfirmation::Confirmed);
        assert!(matches!(
            stopped.observation,
            DaemonControlObservation::Stopped
        ));

        let restarted =
            classify_owner_observation(DaemonControlAction::Restart, Ok(Some(snapshot())));
        assert_eq!(restarted.confirmation, DaemonControlConfirmation::Confirmed);
        assert!(matches!(
            restarted.observation,
            DaemonControlObservation::Running(_)
        ));
    }

    #[test]
    fn owner_observation_reports_contradictions_and_query_failure() {
        let stop_still_running =
            classify_owner_observation(DaemonControlAction::Stop, Ok(Some(snapshot())));
        assert_eq!(
            stop_still_running.confirmation,
            DaemonControlConfirmation::NotConfirmed
        );

        let restart_stopped = classify_owner_observation(DaemonControlAction::Restart, Ok(None));
        assert_eq!(
            restart_stopped.confirmation,
            DaemonControlConfirmation::NotConfirmed
        );

        let unavailable = classify_owner_observation(
            DaemonControlAction::Restart,
            Err("query-secret".to_owned()),
        );
        assert_eq!(
            unavailable.confirmation,
            DaemonControlConfirmation::Unavailable
        );
        assert!(!format!("{unavailable:?}").contains("query-secret"));
    }

    #[test]
    fn stale_completion_does_not_replace_current_daemon_state() {
        let mut app = crate::test_support::GuiHarness::new();
        app.active_daemon_control_id = Some(9);
        app.daemon = DaemonLoadState::Ready(snapshot());
        let _ = app.finish_daemon_control(
            8,
            DaemonControlAction::Stop,
            Ok(DaemonControlOutcome {
                observation: DaemonControlObservation::Stopped,
                confirmation: DaemonControlConfirmation::Confirmed,
            }),
        );
        assert_eq!(app.active_daemon_control_id, Some(9));
        assert!(matches!(app.daemon, DaemonLoadState::Ready(_)));
    }
}
