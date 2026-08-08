//! Text-adapter runtime status and D-Bus controls for the LLM page.

use std::fmt;

use iced::Task;
use vinpst_protocol::dbus;

use crate::{
    App, DaemonLoadState, DaemonSnapshot, GuiLocale, GuiText, Message, OperationState,
    daemon_client::query_daemon_snapshot_on, daemon_proxy,
};

/// One text-adapter runtime action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterRuntimeAction {
    /// Start the configured adapter process.
    Start,
    /// Stop the configured adapter process.
    Stop,
}

impl AdapterRuntimeAction {
    const fn verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
        }
    }

    const fn expected_running(self) -> bool {
        matches!(self, Self::Start)
    }
}

/// Typed text-adapter runtime interaction.
#[derive(Clone)]
pub enum AdapterRuntimeMessage {
    /// Start one configured adapter.
    Start(String),
    /// Stop one configured adapter.
    Stop(String),
    /// Result of a submitted runtime action.
    Finished(Result<AdapterRuntimeOutcome, AdapterRuntimeError>),
}

impl fmt::Debug for AdapterRuntimeMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(id) => formatter.debug_tuple("Start").field(id).finish(),
            Self::Stop(id) => formatter.debug_tuple("Stop").field(id).finish(),
            Self::Finished(Ok(outcome)) => formatter
                .debug_struct("Finished")
                .field("adapter_id", &outcome.adapter_id)
                .field("action", &outcome.action)
                .field("confirmation", &outcome.confirmation)
                .finish(),
            Self::Finished(Err(error)) => formatter
                .debug_struct("Finished")
                .field("adapter_id", &error.adapter_id)
                .field("action", &error.action)
                .field("category", &error.category)
                .finish(),
        }
    }
}

/// Result after the daemon accepted one adapter runtime action.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterRuntimeOutcome {
    /// Stable configured adapter id.
    pub adapter_id: String,
    /// Submitted runtime action.
    pub action: AdapterRuntimeAction,
    /// Daemon-owner generation captured before the lifecycle request.
    pub owner_generation: u64,
    /// Typed confirmation derived from the post-action daemon snapshot.
    pub confirmation: AdapterRuntimeConfirmation,
    /// Fresh daemon snapshot when state could be read after the action.
    pub snapshot: Option<DaemonSnapshot>,
}

/// Post-action confirmation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterRuntimeConfirmation {
    /// The fresh daemon state matches the requested running state.
    Confirmed,
    /// The daemon accepted the action but the fresh state did not match it.
    NotConfirmed,
    /// The daemon accepted the action but the fresh state could not be read.
    Unavailable,
}

/// Secret-free adapter runtime failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRuntimeError {
    /// Stable configured adapter id.
    pub adapter_id: String,
    /// Submitted runtime action.
    pub action: AdapterRuntimeAction,
    /// Fixed error category that excludes daemon text and process details.
    pub category: AdapterRuntimeErrorCategory,
}

/// Fixed adapter runtime error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterRuntimeErrorCategory {
    /// The adapter is no longer present in the loaded config.
    NotConfigured,
    /// The session bus could not be reached.
    SessionBusUnavailable,
    /// The daemon proxy could not be constructed.
    DaemonUnavailable,
    /// The daemon rejected or failed the requested lifecycle action.
    ActionRejected,
}

impl AdapterRuntimeError {
    fn message(&self, locale: GuiLocale) -> String {
        match (locale, self.action, self.category) {
            (GuiLocale::EnUs, action, category) => {
                let detail = match category {
                    AdapterRuntimeErrorCategory::NotConfigured => "is no longer configured",
                    AdapterRuntimeErrorCategory::SessionBusUnavailable => {
                        "cannot reach the session bus"
                    }
                    AdapterRuntimeErrorCategory::DaemonUnavailable => "cannot reach the daemon",
                    AdapterRuntimeErrorCategory::ActionRejected => "daemon rejected the request",
                };
                format!(
                    "Cannot {} text adapter `{}`: {detail}.",
                    action.verb(),
                    self.adapter_id
                )
            }
            (GuiLocale::ZhCn, action, category) => {
                let action = if action == AdapterRuntimeAction::Start {
                    "启动"
                } else {
                    "停止"
                };
                let detail = match category {
                    AdapterRuntimeErrorCategory::NotConfigured => "已不在配置中",
                    AdapterRuntimeErrorCategory::SessionBusUnavailable => "无法访问会话总线",
                    AdapterRuntimeErrorCategory::DaemonUnavailable => "无法访问守护进程",
                    AdapterRuntimeErrorCategory::ActionRejected => "守护进程拒绝了请求",
                };
                format!("无法{action}文本适配器“{}”：{detail}。", self.adapter_id)
            }
        }
    }
}

/// Display-only status for one configured adapter row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AdapterRuntimeViewState {
    pub(super) label: String,
    pub(super) can_start: bool,
    pub(super) can_stop: bool,
}

impl App {
    pub(super) fn intercept_adapter_runtime_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        let Message::AdapterRuntime(message) = message else {
            return None;
        };
        if self.is_busy() && !matches!(message, AdapterRuntimeMessage::Finished(_)) {
            return Some(Task::none());
        }
        Some(self.handle_adapter_runtime_message(message.clone()))
    }

    fn handle_adapter_runtime_message(&mut self, message: AdapterRuntimeMessage) -> Task<Message> {
        match message {
            AdapterRuntimeMessage::Start(adapter_id) => {
                self.begin_adapter_runtime_action(adapter_id, AdapterRuntimeAction::Start)
            }
            AdapterRuntimeMessage::Stop(adapter_id) => {
                self.begin_adapter_runtime_action(adapter_id, AdapterRuntimeAction::Stop)
            }
            AdapterRuntimeMessage::Finished(result) => self.finish_adapter_runtime_action(result),
        }
    }

    fn begin_adapter_runtime_action(
        &mut self,
        adapter_id: String,
        action: AdapterRuntimeAction,
    ) -> Task<Message> {
        let configured = self.config.as_ref().is_ok_and(|document| {
            document
                .config
                .llm
                .adapters
                .iter()
                .any(|adapter| adapter.id == adapter_id)
        });
        if !configured {
            return self.finish_adapter_runtime_action(Err(AdapterRuntimeError {
                adapter_id,
                action,
                category: AdapterRuntimeErrorCategory::NotConfigured,
            }));
        }
        self.operation = OperationState::Running(
            self.locale
                .adapter_runtime_progress(action == AdapterRuntimeAction::Start),
        );
        let owner_generation = self.daemon_owner_generation;
        let worker_adapter_id = adapter_id.clone();
        crate::blocking_task::perform(
            "vinpst-gui-adapter-runtime",
            move || run_adapter_runtime_action(worker_adapter_id, action, owner_generation),
            move |result| {
                Message::AdapterRuntime(AdapterRuntimeMessage::Finished(result.unwrap_or(Err(
                    AdapterRuntimeError {
                        adapter_id,
                        action,
                        category: AdapterRuntimeErrorCategory::ActionRejected,
                    },
                ))))
            },
        )
    }

    fn finish_adapter_runtime_action(
        &mut self,
        result: Result<AdapterRuntimeOutcome, AdapterRuntimeError>,
    ) -> Task<Message> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.operation = OperationState::Failed(error.message(self.locale));
                return Task::none();
            }
        };
        let start = outcome.action == AdapterRuntimeAction::Start;
        if outcome.owner_generation != self.daemon_owner_generation {
            self.operation = OperationState::Succeeded(
                self.locale
                    .adapter_runtime_previous_owner(&outcome.adapter_id, start),
            );
            return self.restart_daemon_refresh(false);
        }
        self.operation = OperationState::Succeeded(match outcome.confirmation {
            AdapterRuntimeConfirmation::Confirmed => self
                .locale
                .adapter_runtime_confirmed(&outcome.adapter_id, outcome.action.expected_running()),
            AdapterRuntimeConfirmation::NotConfirmed => {
                self.locale
                    .adapter_runtime_unconfirmed(&outcome.adapter_id, start, false)
            }
            AdapterRuntimeConfirmation::Unavailable => {
                self.locale
                    .adapter_runtime_unconfirmed(&outcome.adapter_id, start, true)
            }
        });
        self.restart_daemon_refresh(false)
    }

    pub(super) fn adapter_runtime_view_state(&self, adapter_id: &str) -> AdapterRuntimeViewState {
        let DaemonLoadState::Ready(snapshot) = &self.daemon else {
            return AdapterRuntimeViewState {
                label: self.locale.text(GuiText::RuntimeUnavailable).to_owned(),
                can_start: false,
                can_stop: false,
            };
        };
        let Some(summary) = snapshot
            .text_adapters
            .adapters
            .iter()
            .find(|adapter| adapter.id == adapter_id)
        else {
            return AdapterRuntimeViewState {
                label: self.locale.text(GuiText::NotReportedByDaemon).to_owned(),
                can_start: false,
                can_stop: false,
            };
        };
        if summary.is_running {
            AdapterRuntimeViewState {
                label: summary.pid.map_or_else(
                    || self.locale.text(GuiText::Running).to_owned(),
                    |pid| self.locale.runtime_running_pid(pid),
                ),
                can_start: false,
                can_stop: true,
            }
        } else {
            AdapterRuntimeViewState {
                label: self.locale.text(GuiText::Stopped).to_owned(),
                can_start: true,
                can_stop: false,
            }
        }
    }
}

fn run_adapter_runtime_action(
    adapter_id: String,
    action: AdapterRuntimeAction,
    owner_generation: u64,
) -> Result<AdapterRuntimeOutcome, AdapterRuntimeError> {
    let connection = zbus::blocking::Connection::session().map_err(|_| AdapterRuntimeError {
        adapter_id: adapter_id.clone(),
        action,
        category: AdapterRuntimeErrorCategory::SessionBusUnavailable,
    })?;
    run_adapter_runtime_action_on(&connection, adapter_id, action, owner_generation)
}

fn run_adapter_runtime_action_on(
    connection: &zbus::blocking::Connection,
    adapter_id: String,
    action: AdapterRuntimeAction,
    owner_generation: u64,
) -> Result<AdapterRuntimeOutcome, AdapterRuntimeError> {
    let proxy = daemon_proxy(connection).map_err(|_| AdapterRuntimeError {
        adapter_id: adapter_id.clone(),
        action,
        category: AdapterRuntimeErrorCategory::DaemonUnavailable,
    })?;
    let method = match action {
        AdapterRuntimeAction::Start => dbus::method::START_ADAPTER,
        AdapterRuntimeAction::Stop => dbus::method::STOP_ADAPTER,
    };
    proxy
        .call::<_, _, ()>(method, &(adapter_id.as_str(),))
        .map_err(|_| AdapterRuntimeError {
            adapter_id: adapter_id.clone(),
            action,
            category: AdapterRuntimeErrorCategory::ActionRejected,
        })?;

    let snapshot = query_daemon_snapshot_on(connection).ok();
    let confirmation =
        snapshot
            .as_ref()
            .map_or(AdapterRuntimeConfirmation::Unavailable, |snapshot| {
                let matches = snapshot
                    .text_adapters
                    .adapters
                    .iter()
                    .find(|adapter| adapter.id == adapter_id)
                    .is_some_and(|adapter| adapter.is_running == action.expected_running());
                if matches {
                    AdapterRuntimeConfirmation::Confirmed
                } else {
                    AdapterRuntimeConfirmation::NotConfirmed
                }
            });
    Ok(AdapterRuntimeOutcome {
        adapter_id,
        action,
        owner_generation,
        confirmation,
        snapshot,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };

    use serde_json::json;
    use vinpst_protocol::{TextAdapterState, TextAdapterSummary};

    use super::*;

    struct PrivateBus {
        child: Child,
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn start_private_bus() -> (String, PrivateBus) {
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address=1",
                "--print-pid=1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start private dbus-daemon");
        let stdout = child.stdout.take().expect("private bus stdout");
        let mut lines = BufReader::new(stdout).lines();
        let address = lines
            .next()
            .expect("private bus address line")
            .expect("read private bus address");
        let pid = lines
            .next()
            .expect("private bus pid line")
            .expect("read private bus pid")
            .parse::<u32>()
            .expect("private bus pid should be numeric");
        assert_eq!(pid, child.id());
        (address, PrivateBus { child })
    }

    #[derive(Clone)]
    struct AdapterDaemon {
        running: Arc<Mutex<bool>>,
    }

    #[zbus::interface(name = "org.fcitx.Vinpst.Service")]
    impl AdapterDaemon {
        #[zbus(name = "StartAdapter")]
        fn start_adapter(&self, adapter_id: &str) -> zbus::fdo::Result<()> {
            if adapter_id != "adapter-a" {
                return Err(zbus::fdo::Error::Failed("unknown adapter".to_owned()));
            }
            *self.running.lock().expect("running lock") = true;
            Ok(())
        }

        #[zbus(name = "StopAdapter")]
        fn stop_adapter(&self, adapter_id: &str) -> zbus::fdo::Result<()> {
            if adapter_id != "adapter-a" {
                return Err(zbus::fdo::Error::Failed("unknown adapter".to_owned()));
            }
            *self.running.lock().expect("running lock") = false;
            Ok(())
        }

        #[zbus(name = "GetStatus")]
        fn get_status(&self) -> String {
            let _ = &self.running;
            "idle".to_owned()
        }

        #[zbus(name = "GetRuntimeStatus")]
        fn get_runtime_status(&self) -> String {
            let _ = &self.running;
            json!({"active_session": false}).to_string()
        }

        #[zbus(name = "GetTextAdapterState")]
        fn get_text_adapter_state(&self) -> String {
            let running = *self.running.lock().expect("running lock");
            serde_json::to_string(&TextAdapterState::from_adapters(vec![TextAdapterSummary {
                id: "adapter-a".to_owned(),
                kind: "command".to_owned(),
                is_running: running,
                pid: running.then_some(4242),
                ..TextAdapterSummary::default()
            }]))
            .expect("serialize adapter state")
        }
    }

    fn snapshot(adapter_id: &str, running: bool, pid: Option<u32>) -> DaemonSnapshot {
        DaemonSnapshot {
            status: "idle".to_owned(),
            runtime: json!({"active_session": false}),
            text_adapters: TextAdapterState::from_adapters(vec![TextAdapterSummary {
                id: adapter_id.to_owned(),
                kind: "command".to_owned(),
                is_running: running,
                pid,
                ..TextAdapterSummary::default()
            }]),
        }
    }

    #[test]
    fn runtime_view_projects_running_stopped_and_unavailable_states() {
        let mut app = crate::test_support::GuiHarness::new();
        app.daemon = DaemonLoadState::Ready(snapshot("adapter-a", true, Some(42)));
        let running = app.adapter_runtime_view_state("adapter-a");
        assert!(!running.can_start);
        assert!(running.can_stop);
        assert!(running.label.contains("42"));

        app.daemon = DaemonLoadState::Ready(snapshot("adapter-a", false, None));
        let stopped = app.adapter_runtime_view_state("adapter-a");
        assert!(stopped.can_start);
        assert!(!stopped.can_stop);
        assert!(!stopped.label.is_empty());

        app.daemon = DaemonLoadState::Failed("private failure".to_owned());
        let unavailable = app.adapter_runtime_view_state("adapter-a");
        assert!(!unavailable.can_start);
        assert!(!unavailable.can_stop);
        assert!(!unavailable.label.contains("private failure"));

        app.locale = crate::GuiLocale::ZhCn;
        app.daemon = DaemonLoadState::Ready(snapshot("adapter-a", true, Some(42)));
        let localized = app.adapter_runtime_view_state("adapter-a");
        assert!(localized.label.contains("42"));
        assert_ne!(localized.label, running.label);
    }

    #[test]
    fn unconfigured_adapter_is_rejected_before_dbus() {
        let mut app = crate::test_support::GuiHarness::new();
        let _ = app.handle_adapter_runtime_message(AdapterRuntimeMessage::Start(
            "missing-adapter".to_owned(),
        ));
        assert!(matches!(app.operation, OperationState::Failed(_)));
    }

    #[test]
    fn runtime_messages_and_errors_exclude_raw_daemon_details() {
        let error = AdapterRuntimeError {
            adapter_id: "adapter-a".to_owned(),
            action: AdapterRuntimeAction::Start,
            category: AdapterRuntimeErrorCategory::ActionRejected,
        };
        let debug = format!("{:?}", AdapterRuntimeMessage::Finished(Err(error.clone())));
        assert!(!debug.contains("command"));
        assert!(!debug.contains("environment"));
        assert_eq!(error.category, AdapterRuntimeErrorCategory::ActionRejected);
    }

    #[test]
    fn accepted_action_refreshes_current_owner_without_installing_worker_snapshot() {
        let mut app = crate::test_support::GuiHarness::new();
        app.daemon = DaemonLoadState::Failed("refresh pending".to_owned());
        app.operation = OperationState::Running("Starting text adapter…");
        let owner_generation = app.daemon_owner_generation;
        let _ = app.finish_adapter_runtime_action(Ok(AdapterRuntimeOutcome {
            adapter_id: "adapter-a".to_owned(),
            action: AdapterRuntimeAction::Start,
            owner_generation,
            confirmation: AdapterRuntimeConfirmation::Confirmed,
            snapshot: Some(snapshot("adapter-a", true, Some(42))),
        }));
        assert!(matches!(app.operation, OperationState::Succeeded(_)));
        assert!(matches!(app.daemon, DaemonLoadState::Failed(_)));
        assert!(app.active_daemon_refresh_id.is_some());
    }

    #[test]
    fn private_bus_start_and_stop_confirm_typed_runtime_state() {
        let (address, _bus) = start_private_bus();
        let running = Arc::new(Mutex::new(false));
        let _server = zbus::blocking::connection::Builder::address(address.as_str())
            .expect("server address")
            .name(dbus::SERVICE_BUS_NAME)
            .expect("service name")
            .serve_at(
                dbus::SERVICE_OBJECT_PATH,
                AdapterDaemon {
                    running: Arc::clone(&running),
                },
            )
            .expect("serve adapter daemon")
            .build()
            .expect("build adapter daemon");
        let client = zbus::blocking::connection::Builder::address(address.as_str())
            .expect("client address")
            .build()
            .expect("build adapter client");

        let started = run_adapter_runtime_action_on(
            &client,
            "adapter-a".to_owned(),
            AdapterRuntimeAction::Start,
            7,
        )
        .expect("start adapter");
        assert_eq!(started.confirmation, AdapterRuntimeConfirmation::Confirmed);
        assert!(
            started
                .snapshot
                .expect("start snapshot")
                .text_adapters
                .adapters[0]
                .is_running
        );

        let stopped = run_adapter_runtime_action_on(
            &client,
            "adapter-a".to_owned(),
            AdapterRuntimeAction::Stop,
            7,
        )
        .expect("stop adapter");
        assert_eq!(stopped.confirmation, AdapterRuntimeConfirmation::Confirmed);
        assert!(
            !stopped
                .snapshot
                .expect("stop snapshot")
                .text_adapters
                .adapters[0]
                .is_running
        );
    }

    #[test]
    fn stale_owner_completion_restarts_current_owner_refresh() {
        let mut app = crate::test_support::GuiHarness::new();
        let old_generation = app.daemon_owner_generation;
        app.daemon_owner_generation = old_generation.wrapping_add(1);
        app.daemon = DaemonLoadState::Failed("owner changed".to_owned());
        app.active_daemon_refresh_id = Some(77);

        let _ = app.finish_adapter_runtime_action(Ok(AdapterRuntimeOutcome {
            adapter_id: "adapter-a".to_owned(),
            action: AdapterRuntimeAction::Start,
            owner_generation: old_generation,
            confirmation: AdapterRuntimeConfirmation::Confirmed,
            snapshot: Some(snapshot("adapter-a", true, Some(42))),
        }));

        assert!(matches!(app.daemon, DaemonLoadState::Failed(_)));
        assert!(matches!(app.operation, OperationState::Succeeded(_)));
        assert!(app.active_daemon_refresh_id.is_some());
        assert_ne!(app.active_daemon_refresh_id, Some(77));
    }
}
