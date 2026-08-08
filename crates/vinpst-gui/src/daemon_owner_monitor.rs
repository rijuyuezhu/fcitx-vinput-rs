//! Signal-driven monitoring of the daemon's well-known D-Bus owner.

use std::time::Duration;

use iced::{Subscription, Task, futures::SinkExt, futures::StreamExt};
use vinpst_protocol::dbus;

use crate::{
    App, DaemonLoadState, DaemonSnapshot, Message, daemon_state_from_poll, query_daemon_snapshot,
    query_daemon_snapshot_if_owned,
};

/// One lifecycle event emitted by the daemon owner monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonOwnerEvent {
    /// The monitor connected and sampled the current owner without activation.
    Connected {
        /// Whether the daemon service currently has an owner.
        owned: bool,
    },
    /// The daemon service owner changed.
    Changed {
        /// Whether the service has an owner after the change.
        owned: bool,
    },
    /// The signal connection failed and will be retried.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DaemonOwnerMonitorState {
    Connecting,
    Ready,
    Failed(String),
}

pub(crate) fn subscription() -> Subscription<DaemonOwnerEvent> {
    Subscription::run(owner_event_stream)
}

impl App {
    pub(crate) fn begin_daemon_refresh(&mut self, show_loading: bool) -> Task<Message> {
        if self.active_daemon_refresh_id.is_some() {
            return Task::none();
        }
        let operation_id = self.next_daemon_refresh_id();
        self.active_daemon_refresh_id = Some(operation_id);
        if show_loading {
            self.daemon = DaemonLoadState::Loading;
        }
        daemon_refresh_task(operation_id)
    }

    pub(crate) fn restart_daemon_refresh(&mut self, show_loading: bool) -> Task<Message> {
        self.active_daemon_refresh_id = None;
        self.begin_daemon_refresh(show_loading)
    }

    pub(crate) fn begin_daemon_fallback_poll(&mut self) -> Task<Message> {
        if self.active_daemon_refresh_id.is_some() {
            return Task::none();
        }
        let operation_id = self.next_daemon_refresh_id();
        self.active_daemon_refresh_id = Some(operation_id);
        daemon_fallback_poll_task(operation_id)
    }

    pub(crate) fn finish_daemon_refresh(
        &mut self,
        operation_id: u64,
        result: Result<DaemonSnapshot, String>,
    ) {
        if self.active_daemon_refresh_id != Some(operation_id) {
            return;
        }
        self.active_daemon_refresh_id = None;
        self.daemon = match result {
            Ok(snapshot) => DaemonLoadState::Ready(snapshot),
            Err(error) => DaemonLoadState::Failed(error),
        };
    }

    pub(crate) fn finish_daemon_fallback_poll(
        &mut self,
        operation_id: u64,
        result: Result<Option<DaemonSnapshot>, String>,
    ) {
        if self.active_daemon_refresh_id != Some(operation_id) {
            return;
        }
        self.active_daemon_refresh_id = None;
        self.daemon = daemon_state_from_poll(result);
    }

    pub(crate) fn handle_daemon_owner_event(&mut self, event: DaemonOwnerEvent) -> Task<Message> {
        self.daemon_owner_generation = self.daemon_owner_generation.wrapping_add(1).max(1);
        match event {
            DaemonOwnerEvent::Connected { owned } | DaemonOwnerEvent::Changed { owned } => {
                self.daemon_owner_monitor = DaemonOwnerMonitorState::Ready;
                self.active_daemon_refresh_id = None;
                if owned {
                    self.begin_daemon_refresh(false)
                } else {
                    self.invalidate_daemon_owner();
                    Task::none()
                }
            }
            DaemonOwnerEvent::Failed(error) => {
                self.daemon_owner_monitor = DaemonOwnerMonitorState::Failed(error);
                self.begin_daemon_fallback_poll()
            }
        }
    }

    pub(crate) fn daemon_reconciliation_subscriptions(&self) -> Vec<Subscription<Message>> {
        let mut subscriptions = vec![subscription().map(Message::DaemonOwnerEvent)];
        if matches!(
            self.daemon_owner_monitor,
            DaemonOwnerMonitorState::Failed(_)
        ) && self.active_daemon_refresh_id.is_none()
        {
            subscriptions.push(
                iced::time::every(Duration::from_secs(30)).map(|_| Message::DaemonFallbackPollTick),
            );
        }
        subscriptions
    }

    fn invalidate_daemon_owner(&mut self) {
        self.active_daemon_refresh_id = None;
        self.daemon = DaemonLoadState::Stopped;
    }

    fn next_daemon_refresh_id(&mut self) -> u64 {
        let operation_id = self.next_daemon_refresh_id;
        self.next_daemon_refresh_id = self.next_daemon_refresh_id.wrapping_add(1).max(1);
        operation_id
    }
}

fn daemon_refresh_task(operation_id: u64) -> Task<Message> {
    crate::blocking_task::perform(
        "vinpst-gui-daemon-refresh",
        query_daemon_snapshot,
        move |result| Message::DaemonLoaded {
            operation_id,
            result: result.unwrap_or_else(|failure| Err(failure.to_string())),
        },
    )
}

fn daemon_fallback_poll_task(operation_id: u64) -> Task<Message> {
    crate::blocking_task::perform(
        "vinpst-gui-daemon-owner-poll",
        query_daemon_snapshot_if_owned,
        move |result| Message::DaemonFallbackPolled {
            operation_id,
            result: result.unwrap_or_else(|failure| Err(failure.to_string())),
        },
    )
}

fn owner_event_stream() -> impl iced::futures::Stream<Item = DaemonOwnerEvent> {
    iced::stream::channel(16, async |mut output| {
        loop {
            if let Err(error) = monitor_once(&mut output).await {
                if output.send(DaemonOwnerEvent::Failed(error)).await.is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    })
}

async fn monitor_once(
    output: &mut iced::futures::channel::mpsc::Sender<DaemonOwnerEvent>,
) -> Result<(), String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("Connect to the session bus for daemon monitoring: {error}"))?;
    let proxy = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|error| format!("Create the D-Bus owner monitor: {error}"))?;
    let mut changes = proxy
        .receive_name_owner_changed_with_args(&[(0, dbus::SERVICE_BUS_NAME)])
        .await
        .map_err(|error| format!("Subscribe to daemon owner changes: {error}"))?;
    let service_name = zbus::names::BusName::try_from(dbus::SERVICE_BUS_NAME)
        .map_err(|error| format!("Validate daemon bus name: {error}"))?;
    let owned = proxy
        .name_has_owner(service_name)
        .await
        .map_err(|error| format!("Query daemon owner without activation: {error}"))?;
    send_event(&mut *output, DaemonOwnerEvent::Connected { owned }).await?;

    while let Some(signal) = changes.next().await {
        let args = signal
            .args()
            .map_err(|error| format!("Decode daemon owner change: {error}"))?;
        send_event(
            &mut *output,
            DaemonOwnerEvent::Changed {
                owned: args.new_owner().is_some(),
            },
        )
        .await?;
    }
    Err("Daemon owner-change stream ended unexpectedly.".to_owned())
}

async fn send_event(
    output: &mut iced::futures::channel::mpsc::Sender<DaemonOwnerEvent>,
    event: DaemonOwnerEvent,
) -> Result<(), String> {
    output
        .send(event)
        .await
        .map_err(|_| "Daemon owner monitor receiver closed.".to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };

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

    #[test]
    fn monitor_events_are_secret_free_and_stable() {
        assert_eq!(
            format!("{:?}", DaemonOwnerEvent::Connected { owned: true }),
            "Connected { owned: true }"
        );
        assert_eq!(
            format!("{:?}", DaemonOwnerEvent::Changed { owned: false }),
            "Changed { owned: false }"
        );
    }

    #[tokio::test]
    async fn filtered_owner_stream_observes_acquire_and_loss() {
        let (address, _bus) = start_private_bus();
        let monitor = zbus::connection::Builder::address(address.as_str())
            .expect("monitor address")
            .build()
            .await
            .expect("monitor connection");
        let owner = zbus::connection::Builder::address(address.as_str())
            .expect("owner address")
            .build()
            .await
            .expect("owner connection");
        let proxy = zbus::fdo::DBusProxy::new(&monitor)
            .await
            .expect("D-Bus proxy");
        let name = "org.fcitx.Vinpst.OwnerMonitorTest";
        let mut changes = proxy
            .receive_name_owner_changed_with_args(&[(0, name)])
            .await
            .expect("filtered owner stream");

        owner.request_name(name).await.expect("request test name");
        let acquired = tokio::time::timeout(Duration::from_secs(2), changes.next())
            .await
            .expect("acquire signal deadline")
            .expect("acquire signal");
        assert!(acquired.args().expect("acquire args").new_owner().is_some());

        assert!(owner.release_name(name).await.expect("release test name"));
        let lost = tokio::time::timeout(Duration::from_secs(2), changes.next())
            .await
            .expect("loss signal deadline")
            .expect("loss signal");
        assert!(lost.args().expect("loss args").new_owner().is_none());
    }
}
