//! `zbus` service facade for the legacy daemon D-Bus ABI.
#![allow(missing_docs)]

use std::sync::Arc;
use tokio::sync::Mutex;
use vinput_protocol::dbus;
use zbus::{Connection, fdo};

use crate::{RuntimeError, RuntimeState};

/// Thread-safe D-Bus facade over the daemon runtime.
#[derive(Debug, Clone)]
pub struct VinputDbusService {
    runtime: Arc<Mutex<RuntimeState>>,
}

impl VinputDbusService {
    /// Creates a service facade over an existing runtime.
    #[must_use]
    pub fn new(runtime: RuntimeState) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
        }
    }

    /// Registers the service object and requests the legacy bus name.
    pub async fn serve_on_session_bus(self) -> zbus::Result<Connection> {
        let connection = Connection::session().await?;
        connection
            .object_server()
            .at(dbus::SERVICE_OBJECT_PATH, self)
            .await?;
        connection.request_name(dbus::SERVICE_BUS_NAME).await?;
        Ok(connection)
    }

    fn map_runtime_error(error: &RuntimeError) -> fdo::Error {
        fdo::Error::Failed(error.to_string())
    }

    fn map_json_error(error: impl std::error::Error) -> fdo::Error {
        fdo::Error::Failed(format!("failed to serialize response: {error}"))
    }
}

#[allow(missing_docs)]
#[zbus::interface(name = "org.fcitx.Vinput.Service")]
impl VinputDbusService {
    /// Start normal speech recognition.
    #[zbus(name = "StartRecording")]
    async fn start_recording(&self) -> fdo::Result<String> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_recording()
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(runtime.status().to_string())
    }

    /// Start command-mode speech recognition with selected text context.
    #[zbus(name = "StartCommandRecording")]
    async fn start_command_recording(&self, selected_text: &str) -> fdo::Result<String> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_command_recording(selected_text)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(runtime.status().to_string())
    }

    /// Stop current recording and return the legacy recognition JSON payload.
    #[zbus(name = "StopRecording")]
    async fn stop_recording(&self, scene_id: &str) -> fdo::Result<String> {
        let scene = (!scene_id.is_empty()).then_some(scene_id);
        let mut runtime = self.runtime.lock().await;
        let payload = runtime
            .stop_recording(scene)
            .map_err(|error| Self::map_runtime_error(&error))?;
        payload.to_json_string().map_err(Self::map_json_error)
    }

    /// Return current daemon status.
    #[zbus(name = "GetStatus")]
    async fn get_status(&self) -> String {
        let runtime = self.runtime.lock().await;
        runtime.status().to_string()
    }

    /// Return mock ASR backend state JSON.
    #[zbus(name = "GetAsrBackendState")]
    async fn get_asr_backend_state(&self) -> fdo::Result<String> {
        let runtime = self.runtime.lock().await;
        serde_json::to_string(&runtime.asr_backend_state()).map_err(Self::map_json_error)
    }

    /// Reload ASR backend and return the resulting state JSON.
    #[zbus(name = "ReloadAsrBackend")]
    async fn reload_asr_backend(&self) -> fdo::Result<String> {
        let mut runtime = self.runtime.lock().await;
        let state = runtime
            .reload_asr_backend()
            .map_err(|error| Self::map_runtime_error(&error))?;
        serde_json::to_string(&state).map_err(Self::map_json_error)
    }

    /// Start a configured adapter. Stubbed until adapter supervision is ported.
    #[zbus(name = "StartAdapter")]
    #[allow(clippy::unused_self)]
    fn start_adapter(&self, adapter_id: &str) -> String {
        format!("adapter `{adapter_id}` start is not implemented yet")
    }

    /// Stop a configured adapter. Stubbed until adapter supervision is ported.
    #[zbus(name = "StopAdapter")]
    #[allow(clippy::unused_self)]
    fn stop_adapter(&self, adapter_id: &str) -> String {
        format!("adapter `{adapter_id}` stop is not implemented yet")
    }

    /// Frontend notification compatibility placeholder.
    #[zbus(name = "Notify")]
    #[allow(clippy::unused_self)]
    fn notify(&self, summary: &str, body: &str) -> String {
        format!("{summary}: {body}")
    }

    /// Signal emitted when a final recognition result is ready.
    #[zbus(signal, name = "RecognitionResult")]
    async fn recognition_result(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        payload_json: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted for streaming partial recognition text.
    #[zbus(signal, name = "RecognitionPartial")]
    async fn recognition_partial(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        text: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted when daemon status changes.
    #[zbus(signal, name = "StatusChanged")]
    async fn status_changed(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        status: &str,
    ) -> zbus::Result<()>;

    /// Signal emitted for daemon-originated notifications.
    #[zbus(signal, name = "DaemonNotification")]
    async fn daemon_notification(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        summary: &str,
        body: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::VinputDbusService;
    use crate::RuntimeState;
    use vinput_config::VinputConfig;
    use vinput_protocol::{AsrBackendState, RecognitionPayload};

    fn service() -> VinputDbusService {
        let config = VinputConfig::bundled_default().unwrap();
        VinputDbusService::new(RuntimeState::new(config).unwrap())
    }

    #[tokio::test]
    async fn dbus_facade_exercises_normal_mock_flow() {
        let service = service();
        assert_eq!(service.get_status().await, "idle");
        assert_eq!(service.start_recording().await.unwrap(), "recording");
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording("").await.unwrap()).unwrap();
        assert_eq!(payload.commit_text, "mock recognition result");
        assert_eq!(service.get_status().await, "idle");
    }

    #[tokio::test]
    async fn dbus_facade_exercises_command_mock_flow() {
        let service = service();
        assert_eq!(
            service
                .start_command_recording("selected text")
                .await
                .unwrap(),
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording("").await.unwrap()).unwrap();
        assert_eq!(
            payload.commit_text,
            "mock command result for: selected text"
        );
    }

    #[tokio::test]
    async fn dbus_facade_returns_asr_state_json() {
        let service = service();
        let state: AsrBackendState =
            serde_json::from_str(&service.get_asr_backend_state().await.unwrap()).unwrap();
        assert!(state.has_effective_backend);
        assert_eq!(state.effective_provider_id, "sherpa-onnx");
    }
}
