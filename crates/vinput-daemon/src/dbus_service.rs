//! `zbus` service facade for the legacy daemon D-Bus ABI.
#![allow(missing_docs)]

use std::sync::Arc;
use tokio::sync::Mutex;
use vinput_protocol::{AsrBackendState, ServiceStatus, dbus};
use zbus::{Connection, fdo, object_server::SignalEmitter};

use crate::{RuntimeError, RuntimeState};

/// Legacy `GetAsrBackendState` D-Bus output tuple.
type AsrBackendStateTuple = (
    String,
    String,
    String,
    String,
    String,
    bool,
    bool,
    Vec<String>,
);

fn asr_backend_state_tuple(state: AsrBackendState) -> AsrBackendStateTuple {
    (
        state.target_provider_id,
        state.target_model_id,
        state.effective_provider_id,
        state.effective_model_id,
        state.last_error,
        state.reload_in_progress,
        state.has_effective_backend,
        state.remote_endpoints,
    )
}

/// Thread-safe D-Bus facade over the daemon runtime.
#[derive(Clone)]
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

    fn map_signal_error(error: &zbus::Error) -> fdo::Error {
        fdo::Error::Failed(format!("failed to emit signal: {error}"))
    }

    async fn start_recording_state(&self) -> fdo::Result<(String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_recording()
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok((
            runtime.status().to_string(),
            runtime.partial_text().map(ToOwned::to_owned),
        ))
    }

    async fn start_command_recording_state(
        &self,
        selected_text: &str,
    ) -> fdo::Result<(String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_command_recording(selected_text)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok((
            runtime.status().to_string(),
            runtime.partial_text().map(ToOwned::to_owned),
        ))
    }

    async fn ensure_recording_for_stop(&self) -> fdo::Result<()> {
        let runtime = self.runtime.lock().await;
        if runtime.status() == ServiceStatus::Recording {
            Ok(())
        } else {
            Err(Self::map_runtime_error(&RuntimeError::NotRecording(
                runtime.status(),
            )))
        }
    }

    async fn stop_recording_payload(
        &self,
        scene_id: &str,
    ) -> fdo::Result<(String, String, Option<String>)> {
        let scene = (!scene_id.is_empty()).then_some(scene_id);
        let mut runtime = self.runtime.lock().await;
        let report = runtime
            .stop_recording_report(scene)
            .map_err(|error| Self::map_runtime_error(&error))?;
        let payload_json = report
            .payload
            .to_json_string()
            .map_err(Self::map_json_error)?;
        Ok((
            payload_json,
            runtime.status().to_string(),
            report.partial_text,
        ))
    }
}

#[allow(missing_docs)]
#[zbus::interface(name = "org.fcitx.Vinput.Service")]
impl VinputDbusService {
    /// Start normal speech recognition.
    #[zbus(name = "StartRecording")]
    async fn start_recording(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let (status, partial_text) = self.start_recording_state().await?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        if let Some(partial_text) = partial_text {
            Self::recognition_partial(&emitter, &partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        Ok(())
    }

    /// Start command-mode speech recognition with selected text context.
    #[zbus(name = "StartCommandRecording")]
    async fn start_command_recording(
        &self,
        selected_text: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let (status, partial_text) = self.start_command_recording_state(selected_text).await?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        if let Some(partial_text) = partial_text {
            Self::recognition_partial(&emitter, &partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        Ok(())
    }

    /// Stop current recording and return the legacy recognition JSON payload.
    #[zbus(name = "StopRecording")]
    async fn stop_recording(
        &self,
        scene_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        self.ensure_recording_for_stop().await?;
        Self::status_changed(&emitter, "inferring")
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        let (payload_json, status, partial_text) = self.stop_recording_payload(scene_id).await?;
        if let Some(partial_text) = partial_text {
            Self::recognition_partial(&emitter, &partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        Self::recognition_result(&emitter, &payload_json)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        Ok(payload_json)
    }

    /// Return current daemon status.
    #[zbus(name = "GetStatus")]
    async fn get_status(&self) -> String {
        let runtime = self.runtime.lock().await;
        runtime.status().to_string()
    }

    /// Return ASR backend diagnostic state using the legacy tuple signature.
    #[zbus(
        name = "GetAsrBackendState",
        out_args(
            "target_provider_id",
            "target_model_id",
            "effective_provider_id",
            "effective_model_id",
            "last_error",
            "reload_in_progress",
            "has_effective_backend",
            "remote_endpoints"
        )
    )]
    async fn get_asr_backend_state(
        &self,
    ) -> (
        String,
        String,
        String,
        String,
        String,
        bool,
        bool,
        Vec<String>,
    ) {
        let runtime = self.runtime.lock().await;
        asr_backend_state_tuple(runtime.configured_asr_state_for_runtime())
    }

    /// Return text adapter diagnostic state JSON.
    #[zbus(name = "GetTextAdapterState")]
    async fn get_text_adapter_state(&self) -> fdo::Result<String> {
        let mut runtime = self.runtime.lock().await;
        runtime.refresh_text_adapters();
        serde_json::to_string(&runtime.configured_text_adapter_state_for_runtime())
            .map_err(Self::map_json_error)
    }

    /// Reload ASR backend using the legacy void method signature.
    #[zbus(name = "ReloadAsrBackend")]
    async fn reload_asr_backend(&self) -> fdo::Result<()> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .reload_asr_backend()
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(())
    }

    /// Start a configured adapter using the runtime supervisor.
    #[zbus(name = "StartAdapter")]
    async fn start_adapter(&self, adapter_id: &str) -> fdo::Result<()> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_text_adapter(adapter_id)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(())
    }

    /// Stop a configured adapter using the runtime supervisor.
    #[zbus(name = "StopAdapter")]
    async fn stop_adapter(&self, adapter_id: &str) -> fdo::Result<()> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .stop_text_adapter(adapter_id)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(())
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
        code: &str,
        subject: &str,
        detail: &str,
        raw_message: &str,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::VinputDbusService;
    use crate::RuntimeState;
    use vinput_asr::{
        AsrBackend, AsrError, BackendDescriptor, MockAsrBackend, RecognitionContext,
        RecognitionEvent, RecognitionSession,
    };
    use vinput_config::{AsrProviderConfig, AsrProviderKind, LlmAdapterConfig, VinputConfig};
    use vinput_protocol::{RecognitionPayload, TextAdapterState};

    struct EarlyFinalBackend;

    impl AsrBackend for EarlyFinalBackend {
        fn describe(&self) -> BackendDescriptor {
            MockAsrBackend::streaming("early partial", "late final").describe()
        }

        fn create_session(
            &self,
            _context: RecognitionContext,
        ) -> Result<Box<dyn RecognitionSession>, AsrError> {
            Ok(Box::new(EarlyFinalSession { poll_count: 0 }))
        }
    }

    struct EarlyFinalSession {
        poll_count: usize,
    }

    impl RecognitionSession for EarlyFinalSession {
        fn push_audio(&mut self, _samples: &[i16]) -> Result<(), AsrError> {
            Ok(())
        }

        fn finish(&mut self) -> Result<(), AsrError> {
            Ok(())
        }

        fn cancel(&mut self) -> Result<(), AsrError> {
            Ok(())
        }

        fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
            let poll_index = self.poll_count;
            self.poll_count += 1;
            if poll_index == 0 {
                return Ok(vec![
                    RecognitionEvent::PartialText {
                        text: "early partial".to_owned(),
                    },
                    RecognitionEvent::FinalText {
                        text: "early final".to_owned(),
                    },
                ]);
            }
            Ok(vec![RecognitionEvent::Completed])
        }
    }

    fn service() -> VinputDbusService {
        let config = VinputConfig::bundled_default().unwrap();
        VinputDbusService::new(RuntimeState::new(config).unwrap())
    }

    fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vinput-daemon-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn dbus_facade_exercises_normal_mock_flow() {
        let service = service();
        assert_eq!(service.get_status().await, "idle");
        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(payload.commit_text, "mock recognition result");
        assert_eq!(service.get_status().await, "idle");
    }

    #[tokio::test]
    async fn dbus_facade_preserves_early_final_events() {
        let config = VinputConfig::bundled_default().unwrap();
        let runtime = RuntimeState::with_asr_backend(config, Box::new(EarlyFinalBackend)).unwrap();
        let service = VinputDbusService::new(runtime);

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let (payload_json, status, partial_text) =
            service.stop_recording_payload("").await.unwrap();
        let payload = RecognitionPayload::from_json_str(&payload_json).unwrap();

        assert_eq!(payload.commit_text, "early final");
        assert_eq!(partial_text.as_deref(), Some("early partial"));
        assert_eq!(status, "idle");
    }

    #[tokio::test]
    async fn dbus_facade_exercises_timeout_mock_flow() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.active_scene = "timeout-scene".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "timeout-scene".to_owned(),
                label: "Timeout scene".to_owned(),
                prompt: None,
                provider_id: None,
                model: None,
                candidate_count: 0,
                timeout_ms: Some(2500),
                context_lines: 0,
            });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(
            payload.commit_text,
            "mock postprocess result: mock recognition result"
        );
    }

    #[tokio::test]
    async fn dbus_facade_exercises_command_mock_flow() {
        let service = service();
        assert_eq!(
            service
                .start_command_recording_state("selected text")
                .await
                .unwrap()
                .0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(
            payload.commit_text,
            "mock command result for: selected text"
        );
    }

    #[tokio::test]
    async fn dbus_facade_handles_legacy_command_asr_stdout() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r"cat >/dev/null; printf '%s
' 'dbus final'"
                    .to_owned(),
            ],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let service = VinputDbusService::new(RuntimeState::with_configured_asr(config).unwrap());

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let (payload_json, status, partial_text) =
            service.stop_recording_payload("").await.unwrap();
        let payload = RecognitionPayload::from_json_str(&payload_json).unwrap();

        assert_eq!(payload.commit_text, "dbus final");
        assert_eq!(status, "idle");
        assert!(partial_text.is_none());
    }

    #[tokio::test]
    async fn dbus_facade_uses_configured_text_adapter() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "mock".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        config.scenes.active_scene = "needs-adapter".to_owned();
        config
            .scenes
            .definitions
            .push(vinput_config::SceneDefinition {
                id: "needs-adapter".to_owned(),
                label: "Needs adapter".to_owned(),
                prompt: Some("polish".to_owned()),
                provider_id: None,
                model: None,
                candidate_count: 1,
                timeout_ms: None,
                context_lines: 0,
            });
        config.llm.adapters.push(LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"text":"dbus configured final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let service =
            VinputDbusService::new(RuntimeState::with_configured_backends(config).unwrap());

        assert_eq!(
            service.start_recording_state().await.unwrap().0,
            "recording"
        );
        let payload =
            RecognitionPayload::from_json_str(&service.stop_recording_payload("").await.unwrap().0)
                .unwrap();
        assert_eq!(payload.commit_text, "dbus configured final");
    }

    #[tokio::test]
    async fn dbus_facade_preserves_remote_asr_endpoint() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "remote".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "remote".to_owned(),
            kind: AsrProviderKind::Remote,
            timeout_ms: None,
            model: Some("cloud".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: Some("https://asr.example.test".to_owned()),
        });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

        let state = service.get_asr_backend_state().await;
        assert_eq!(state.0, "remote");
        assert_eq!(state.1, "cloud");
        assert!(!state.6);
        assert_eq!(state.7, ["https://asr.example.test"]);
    }

    #[tokio::test]
    async fn dbus_facade_preserves_command_asr_metadata() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_500),
            model: Some("cmd-model".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("helper".to_owned()),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());

        let state = service.get_asr_backend_state().await;
        assert!(state.6);
        assert_eq!(state.0, "cmd");
        assert_eq!(state.1, "cmd-model");
        assert_eq!(state.2, "cmd");
        assert_eq!(state.3, "cmd-model");
    }

    #[tokio::test]
    async fn dbus_facade_supervises_configured_adapter() {
        let service = service();
        let start_error = service
            .start_adapter("mock-adapter")
            .await
            .expect_err("unconfigured adapter start should fail");
        assert!(
            start_error
                .to_string()
                .contains("text adapter `mock-adapter` is not configured")
        );
        let stop_error = service
            .stop_adapter("mock-adapter")
            .await
            .expect_err("unconfigured adapter stop should fail");
        assert!(
            stop_error
                .to_string()
                .contains("text adapter `mock-adapter` is not configured")
        );

        let runtime_dir = unique_adapter_runtime_dir("dbus-supervisor");
        let pid_path = runtime_dir.join("mock-adapter.pid");
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(LlmAdapterConfig {
            id: "mock-adapter".to_owned(),
            command: "sleep".to_owned(),
            args: vec!["30".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        });
        let runtime = RuntimeState::new(config)
            .unwrap()
            .with_adapter_runtime_paths(vinput_text::AdapterRuntimePaths::new(runtime_dir.clone()));
        let service = VinputDbusService::new(runtime);

        service.start_adapter("mock-adapter").await.unwrap();
        assert!(pid_path.exists());
        let duplicate_error = service
            .start_adapter("mock-adapter")
            .await
            .expect_err("duplicate adapter start should fail");
        assert!(
            duplicate_error
                .to_string()
                .contains("text adapter `mock-adapter` is already running")
        );
        service.stop_adapter("mock-adapter").await.unwrap();
        assert!(!pid_path.exists());
        service.stop_adapter("mock-adapter").await.unwrap();
        let _ = std::fs::remove_dir_all(runtime_dir);
    }

    #[tokio::test]
    async fn dbus_facade_returns_text_adapter_state_json() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(LlmAdapterConfig {
            id: "mock-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
            working_dir: Some("/tmp/adapter-work".to_owned()),
            extra: std::collections::HashMap::default(),
        });
        let service = VinputDbusService::new(RuntimeState::new(config).unwrap());
        let state_json = service.get_text_adapter_state().await.unwrap();
        let state: TextAdapterState = serde_json::from_str(&state_json).unwrap();
        assert!(!state_json.contains("secret"));
        assert!(!state_json.contains("/tmp/adapter-work"));

        assert_eq!(state.adapter_count, 1);
        assert_eq!(state.adapter_ids, ["mock-adapter"]);
        assert_eq!(state.single_adapter_id.as_deref(), Some("mock-adapter"));
        assert_eq!(state.adapters[0].kind, "command");
        assert_eq!(state.adapters[0].command, "vinput-postprocess");
        assert_eq!(state.adapters[0].args, ["--json"]);
        assert_eq!(state.adapters[0].env_count, 1);
        assert!(state.adapters[0].has_working_dir);
    }

    #[tokio::test]
    async fn dbus_facade_returns_asr_state_tuple() {
        let service = service();
        let state = service.get_asr_backend_state().await;
        assert!(!state.6);
        assert_eq!(state.0, "sherpa-onnx");
        assert!(!state.4.is_empty());
    }
}
