//! `zbus` service facade for the legacy daemon D-Bus ABI.
#![allow(missing_docs)]

use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::Duration,
};
use tokio::{sync::Mutex, time::MissedTickBehavior};
use vinpst_protocol::{AsrBackendState, ServiceStatus, dbus};
use vinpst_registry::scan_installed_models;
use zbus::{Connection, DBusError, object_server::SignalEmitter};

use crate::{
    RuntimeError, RuntimeState,
    remote::{RemoteTextLifecycle, RemoteTextLifecycleError, RemoteTextLifecycleStatus},
    runtime::{
        AsrReloadWorkerStep, PendingStopRecording, locale_candidates_from_environment,
        persist_config_atomically, select_asr_provider, select_asr_target,
    },
};

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

type DbusResult<T> = Result<T, VinpstDbusError>;

const MAX_ERROR_DESCRIPTION_LEN: usize = 512;
const LIVE_PARTIAL_POLL_INTERVAL: Duration = Duration::from_millis(40);
const ASR_RELOAD_POLL_INTERVAL: Duration = Duration::from_millis(20);
const ASR_RELOAD_FAILED_CODE: &str = "asr_backend_reload_failed";

#[derive(Debug, Default)]
struct LivePartialEmissionState {
    generation: u64,
    last_emitted: Option<String>,
}

impl LivePartialEmissionState {
    fn begin(&mut self, last_emitted: Option<String>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.last_emitted = last_emitted;
        self.generation
    }

    fn cancel(&mut self) -> Option<String> {
        self.generation = self.generation.wrapping_add(1);
        self.last_emitted.take()
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

#[derive(Debug, DBusError)]
#[zbus(prefix = "org.fcitx.Vinpst.Error")]
enum VinpstDbusError {
    OperationFailed(String),
}

fn sanitize_dbus_error_message(message: &str) -> String {
    let sanitized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = sanitized.to_ascii_lowercase();
    if lower.contains("authorization:") || lower.contains("bearer ") || lower.contains("api_key") {
        return "operation failed".to_owned();
    }
    if sanitized.chars().count() <= MAX_ERROR_DESCRIPTION_LEN {
        return sanitized;
    }
    let mut truncated = sanitized
        .chars()
        .take(MAX_ERROR_DESCRIPTION_LEN.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

/// Thread-safe D-Bus facade over the daemon runtime.
#[derive(Clone)]
pub struct VinpstDbusService {
    runtime: Arc<Mutex<RuntimeState>>,
    remote_text: Arc<Mutex<RemoteTextLifecycle>>,
    recording_operation: Arc<Mutex<()>>,
    live_partials: Arc<Mutex<LivePartialEmissionState>>,
    signal_emitter: Arc<Mutex<Option<SignalEmitter<'static>>>>,
}

impl VinpstDbusService {
    /// Creates a service facade over an existing runtime.
    #[must_use]
    pub fn new(runtime: RuntimeState) -> Self {
        Self::new_with_remote_bind(runtime, IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }

    /// Creates a service facade with an explicit remote-text bind address.
    #[must_use]
    pub fn new_with_remote_bind(runtime: RuntimeState, bind_ip: IpAddr) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
            remote_text: Arc::new(Mutex::new(RemoteTextLifecycle::new(bind_ip))),
            recording_operation: Arc::new(Mutex::new(())),
            live_partials: Arc::new(Mutex::new(LivePartialEmissionState::default())),
            signal_emitter: Arc::new(Mutex::new(None)),
        }
    }

    /// Registers the service object and requests the legacy bus name.
    pub async fn serve_on_session_bus(&self) -> zbus::Result<Connection> {
        let connection = Connection::session().await?;
        self.bind_signal_connection(&connection).await?;
        connection
            .object_server()
            .at(dbus::SERVICE_OBJECT_PATH, self.clone())
            .await?;
        connection
            .request_name_with_flags(
                dbus::SERVICE_BUS_NAME,
                zbus::fdo::RequestNameFlags::DoNotQueue.into(),
            )
            .await?;
        Ok(connection)
    }

    /// Reconciles the daemon-owned remote service with the current runtime config.
    pub async fn start_remote_text_service(&self) -> Result<bool, RemoteTextLifecycleError> {
        let config = self.runtime.lock().await.config_snapshot();
        self.reconcile_remote_text_config(&config).await
    }

    /// Stops the daemon-owned remote service during process shutdown.
    pub async fn shutdown_remote_text_service(&self) -> Result<bool, RemoteTextLifecycleError> {
        self.remote_text.lock().await.stop().await
    }

    /// Returns redacted remote service listener state.
    pub async fn remote_text_status(&self) -> RemoteTextLifecycleStatus {
        self.remote_text.lock().await.status()
    }

    /// Binds background signal emission to the connection hosting this service.
    pub async fn bind_signal_connection(&self, connection: &Connection) -> zbus::Result<()> {
        let emitter = SignalEmitter::new(connection, dbus::SERVICE_OBJECT_PATH)?.to_owned();
        *self.signal_emitter.lock().await = Some(emitter);
        Ok(())
    }

    fn operation_failed(message: impl AsRef<str>) -> VinpstDbusError {
        VinpstDbusError::OperationFailed(sanitize_dbus_error_message(message.as_ref()))
    }

    fn map_runtime_error(error: &RuntimeError) -> VinpstDbusError {
        Self::operation_failed(error.to_string())
    }

    fn map_json_error(error: impl std::error::Error) -> VinpstDbusError {
        Self::operation_failed(format!("failed to serialize response: {error}"))
    }

    fn map_signal_error(error: &zbus::Error) -> VinpstDbusError {
        Self::operation_failed(format!("failed to emit signal: {error}"))
    }

    async fn emit_asr_reload_failure(&self, message: &str) {
        let emitter = self.signal_emitter.lock().await.clone();
        let Some(emitter) = emitter else {
            return;
        };
        if let Err(error) =
            Self::daemon_notification(&emitter, ASR_RELOAD_FAILED_CODE, "", "", message).await
        {
            tracing::warn!(%error, "failed to emit ASR reload notification");
        }
    }

    async fn run_asr_reload_worker(self) {
        loop {
            let step = {
                let mut runtime = self.runtime.lock().await;
                runtime.next_asr_reload_worker_step()
            };
            match step {
                AsrReloadWorkerStep::Wait => {
                    tokio::time::sleep(ASR_RELOAD_POLL_INTERVAL).await;
                }
                AsrReloadWorkerStep::Stop => return,
                AsrReloadWorkerStep::Failed { generation, error } => {
                    let notification = self
                        .runtime
                        .lock()
                        .await
                        .fail_prepared_asr_reload(generation, &error);
                    if let Some(message) = notification {
                        self.emit_asr_reload_failure(&message).await;
                    }
                    return;
                }
                AsrReloadWorkerStep::Prepare(request) => {
                    let generation = request.generation();
                    let result = tokio::task::spawn_blocking(move || request.prepare()).await;
                    match result {
                        Ok(Ok(prepared)) => {
                            let mut prepared = Some(prepared);
                            loop {
                                let applied = {
                                    let mut runtime = self.runtime.lock().await;
                                    if runtime.can_apply_prepared_asr_reload() {
                                        let Some(prepared) = prepared.take() else {
                                            return;
                                        };
                                        runtime.complete_prepared_asr_reload(prepared);
                                        true
                                    } else {
                                        false
                                    }
                                };
                                if applied {
                                    break;
                                }
                                tokio::time::sleep(ASR_RELOAD_POLL_INTERVAL).await;
                            }
                        }
                        Ok(Err(error)) => {
                            let notification = self
                                .runtime
                                .lock()
                                .await
                                .fail_prepared_asr_reload(generation, &error);
                            if let Some(message) = notification {
                                self.emit_asr_reload_failure(&message).await;
                            }
                        }
                        Err(error) => {
                            let error = RuntimeError::BackgroundTask(error.to_string());
                            let notification = self
                                .runtime
                                .lock()
                                .await
                                .fail_prepared_asr_reload(generation, &error);
                            if let Some(message) = notification {
                                self.emit_asr_reload_failure(&message).await;
                            }
                        }
                    }
                }
            }
        }
    }

    async fn reconcile_remote_text_config(
        &self,
        config: &vinpst_config::VinpstConfig,
    ) -> Result<bool, RemoteTextLifecycleError> {
        self.remote_text.lock().await.reconcile_config(config).await
    }

    async fn queue_asr_reload_config(&self, config: vinpst_config::VinpstConfig) -> DbusResult<()> {
        let remote_config = config.clone();
        let should_spawn_worker = self
            .runtime
            .lock()
            .await
            .queue_configured_asr_reload(config)
            .map_err(|error| Self::map_runtime_error(&error))?;
        if should_spawn_worker {
            let service = self.clone();
            tokio::spawn(async move {
                service.run_asr_reload_worker().await;
            });
        }
        self.reconcile_remote_text_config(&remote_config)
            .await
            .map_err(|error| {
                Self::operation_failed(format!("failed to reconcile remote text service: {error}"))
            })?;
        Ok(())
    }

    async fn start_recording_state(&self) -> DbusResult<(String, Option<String>)> {
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
    ) -> DbusResult<(String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_command_recording(selected_text)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok((
            runtime.status().to_string(),
            runtime.partial_text().map(ToOwned::to_owned),
        ))
    }

    async fn ensure_recording_for_stop(&self) -> DbusResult<()> {
        let runtime = self.runtime.lock().await;
        if runtime.status() == ServiceStatus::Recording {
            Ok(())
        } else {
            Err(Self::map_runtime_error(&RuntimeError::NotRecording(
                runtime.status(),
            )))
        }
    }

    async fn begin_stop_recording_payload(
        &self,
        scene_id: &str,
    ) -> DbusResult<PendingStopRecording> {
        let scene = (!scene_id.is_empty()).then_some(scene_id);
        let mut runtime = self.runtime.lock().await;
        runtime
            .begin_stop_recording(scene)
            .map_err(|error| Self::map_runtime_error(&error))
    }

    async fn finish_stop_recording_payload(
        &self,
        pending: PendingStopRecording,
    ) -> DbusResult<(String, String, Option<String>)> {
        let mut runtime = self.runtime.lock().await;
        let report = runtime
            .finish_stop_recording(pending)
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

    async fn abort_stop_recording_payload(&self, pending: PendingStopRecording) -> String {
        let mut runtime = self.runtime.lock().await;
        runtime.abort_stop_recording(&pending);
        runtime.status().to_string()
    }

    #[cfg(test)]
    async fn stop_recording_payload(
        &self,
        scene_id: &str,
    ) -> DbusResult<(String, String, Option<String>)> {
        let pending = self.begin_stop_recording_payload(scene_id).await?;
        self.finish_stop_recording_payload(pending).await
    }

    async fn begin_live_partial_emission(&self, last_emitted: Option<String>) -> u64 {
        self.live_partials.lock().await.begin(last_emitted)
    }

    async fn cancel_live_partial_emission(&self) -> Option<String> {
        self.live_partials.lock().await.cancel()
    }

    async fn lock_recording_operation(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.recording_operation).lock_owned().await
    }

    fn spawn_live_partial_emitter(&self, emitter: SignalEmitter<'static>, generation: u64) {
        let runtime = Arc::clone(&self.runtime);
        let live_partials = Arc::clone(&self.live_partials);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(LIVE_PARTIAL_POLL_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if !live_partials.lock().await.is_current(generation) {
                    break;
                }

                let partials = {
                    let mut runtime = runtime.lock().await;
                    if runtime.status() != ServiceStatus::Recording {
                        break;
                    }
                    match runtime.take_live_partial_texts() {
                        Ok(partials) => partials,
                        Err(_) => break,
                    }
                };

                for partial in partials {
                    let should_emit = {
                        let state = live_partials.lock().await;
                        state.is_current(generation)
                            && state.last_emitted.as_deref() != Some(partial.as_str())
                    };
                    if !should_emit {
                        continue;
                    }
                    if Self::recognition_partial(&emitter, &partial).await.is_err() {
                        return;
                    }
                    let mut state = live_partials.lock().await;
                    if state.is_current(generation) {
                        state.last_emitted = Some(partial);
                    }
                }
            }
        });
    }
}

#[allow(missing_docs)]
#[zbus::interface(name = "org.fcitx.Vinpst.Service")]
impl VinpstDbusService {
    /// Start normal speech recognition.
    #[zbus(name = "StartRecording")]
    async fn start_recording(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), VinpstDbusError> {
        let _operation = self.lock_recording_operation().await;
        let (status, partial_text) = self.start_recording_state().await?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        if let Some(partial_text) = &partial_text {
            Self::recognition_partial(&emitter, partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        let generation = self.begin_live_partial_emission(partial_text).await;
        self.spawn_live_partial_emitter(emitter.to_owned(), generation);
        Ok(())
    }

    /// Start command-mode speech recognition with selected text context.
    #[zbus(name = "StartCommandRecording")]
    async fn start_command_recording(
        &self,
        selected_text: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), VinpstDbusError> {
        let _operation = self.lock_recording_operation().await;
        let (status, partial_text) = self.start_command_recording_state(selected_text).await?;
        Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        if let Some(partial_text) = &partial_text {
            Self::recognition_partial(&emitter, partial_text)
                .await
                .map_err(|error| Self::map_signal_error(&error))?;
        }
        let generation = self.begin_live_partial_emission(partial_text).await;
        self.spawn_live_partial_emitter(emitter.to_owned(), generation);
        Ok(())
    }

    /// Stop current recording and return the legacy recognition JSON payload.
    #[zbus(name = "StopRecording")]
    async fn stop_recording(
        &self,
        scene_id: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<String, VinpstDbusError> {
        let _operation = self.lock_recording_operation().await;
        self.ensure_recording_for_stop().await?;
        let last_emitted_partial = self.cancel_live_partial_emission().await;
        Self::status_changed(&emitter, "inferring")
            .await
            .map_err(|error| Self::map_signal_error(&error))?;
        let pending = match self.begin_stop_recording_payload(scene_id).await {
            Ok(pending) => pending,
            Err(error) => {
                let _ = Self::status_changed(&emitter, "idle").await;
                return Err(error);
            }
        };
        if let Err(error) = Self::status_changed(&emitter, "postprocessing").await {
            let status = self.abort_stop_recording_payload(pending).await;
            let _ = Self::status_changed(&emitter, &status).await;
            return Err(Self::map_signal_error(&error));
        }
        let (payload_json, status, partial_text) =
            match self.finish_stop_recording_payload(pending).await {
                Ok(result) => result,
                Err(error) => {
                    let _ = Self::status_changed(&emitter, "idle").await;
                    return Err(error);
                }
            };
        let result_emission = async {
            if let Some(partial_text) = partial_text
                && last_emitted_partial.as_deref() != Some(partial_text.as_str())
            {
                Self::recognition_partial(&emitter, &partial_text)
                    .await
                    .map_err(|error| Self::map_signal_error(&error))?;
            }
            Self::recognition_result(&emitter, &payload_json)
                .await
                .map_err(|error| Self::map_signal_error(&error))
        }
        .await;
        let status_emission = Self::status_changed(&emitter, &status)
            .await
            .map_err(|error| Self::map_signal_error(&error));
        result_emission?;
        status_emission?;
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
        asr_backend_state_tuple(self.runtime.lock().await.asr_backend_state())
    }

    /// Return text adapter diagnostic state JSON.
    #[zbus(name = "GetTextAdapterState")]
    async fn get_text_adapter_state(&self) -> Result<String, VinpstDbusError> {
        let mut runtime = self.runtime.lock().await;
        runtime.refresh_text_adapters();
        serde_json::to_string(&runtime.configured_text_adapter_state_for_runtime())
            .map_err(Self::map_json_error)
    }

    /// Return sanitized runtime status JSON.
    #[zbus(name = "GetRuntimeStatus")]
    async fn get_runtime_status(&self) -> Result<String, VinpstDbusError> {
        let mut status = {
            let mut runtime = self.runtime.lock().await;
            runtime.refresh_text_adapters();
            runtime.runtime_status_json()
        };
        let remote = self.remote_text.lock().await;
        let remote_status = remote.status();
        let endpoints = remote.endpoints();
        status["remote_text"] = serde_json::json!({
            "running": remote_status.running,
            "listen_addr": remote_status.local_addr.map(|address| address.to_string()),
            "endpoints": endpoints,
        });
        serde_json::to_string(&status).map_err(Self::map_json_error)
    }

    /// Return active scene and configured scene id/label pairs.
    #[zbus(name = "GetSceneState", out_args("active_scene", "scenes"))]
    async fn get_scene_state(&self) -> (String, Vec<(String, String)>) {
        self.runtime.lock().await.scene_state()
    }

    /// Select the active scene and persist it when an explicit config file is available.
    #[zbus(name = "SetActiveScene")]
    async fn set_active_scene(&self, scene_id: &str) -> Result<bool, VinpstDbusError> {
        self.runtime
            .lock()
            .await
            .set_active_scene(scene_id)
            .map_err(|error| Self::map_runtime_error(&error))
    }

    /// Return the capture-device config value used by the next recording.
    #[zbus(name = "GetCaptureDevice")]
    async fn get_capture_device(&self) -> String {
        self.runtime.lock().await.capture_device()
    }

    /// Select and persist the capture device used by the next recording.
    #[zbus(name = "SetCaptureDevice")]
    async fn set_capture_device(&self, target: &str) -> Result<bool, VinpstDbusError> {
        self.runtime
            .lock()
            .await
            .set_capture_device(target)
            .map_err(|error| Self::map_runtime_error(&error))
    }

    /// Return target/effective ASR state and configured provider rows.
    #[zbus(
        name = "GetAsrMenuState",
        out_args(
            "target_provider_id",
            "effective_provider_id",
            "effective_model_id",
            "reload_in_progress",
            "last_error",
            "providers"
        )
    )]
    async fn get_asr_menu_state(
        &self,
    ) -> (
        String,
        String,
        String,
        bool,
        String,
        Vec<(String, String, String)>,
    ) {
        self.runtime.lock().await.asr_menu_state()
    }

    /// Select, persist, and queue reload for a configured ASR provider.
    #[zbus(name = "SetActiveAsrProvider")]
    async fn set_active_asr_provider(&self, provider_id: &str) -> Result<bool, VinpstDbusError> {
        let (config_source, config_path) = {
            let runtime = self.runtime.lock().await;
            (
                runtime.asr_reload_config_source(),
                runtime.config_path_for_persistence(),
            )
        };
        let provider_id = provider_id.to_owned();
        let (config, persisted) = tokio::task::spawn_blocking(move || {
            let config = config_source.load()?;
            let config = select_asr_provider(config, &provider_id).map_err(RuntimeError::Asr)?;
            if let Some(path) = config_path {
                persist_config_atomically(&path, &config, "asr")?;
                Ok((config, true))
            } else {
                Ok((config, false))
            }
        })
        .await
        .map_err(|error| Self::operation_failed(format!("ASR selection task failed: {error}")))?
        .map_err(|error: RuntimeError| Self::map_runtime_error(&error))?;
        self.queue_asr_reload_config(config).await?;
        Ok(persisted)
    }

    /// Return target/effective ASR state and configured provider/model rows.
    #[zbus(
        name = "GetAsrTargetMenuState",
        out_args(
            "target_provider_id",
            "target_model_id",
            "effective_provider_id",
            "effective_model_id",
            "reload_in_progress",
            "last_error",
            "targets"
        )
    )]
    async fn get_asr_target_menu_state(
        &self,
    ) -> Result<
        (
            String,
            String,
            String,
            String,
            bool,
            String,
            Vec<(String, String, String, String)>,
        ),
        VinpstDbusError,
    > {
        let model_root = self.runtime.lock().await.model_root();
        let installed_models = tokio::task::spawn_blocking(move || {
            model_root.map_or_else(
                || Ok(Vec::new()),
                |root| scan_installed_models(&root).map_err(RuntimeError::InstalledModels),
            )
        })
        .await
        .map_err(|error| {
            Self::operation_failed(format!("installed model scan task failed: {error}"))
        })?
        .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(self
            .runtime
            .lock()
            .await
            .asr_target_menu_state(&installed_models))
    }

    /// Return target/effective ASR state and localized provider/model rows.
    #[zbus(
        name = "GetAsrDisplayMenuState",
        out_args(
            "target_provider_id",
            "target_model_id",
            "effective_provider_id",
            "effective_model_id",
            "reload_in_progress",
            "last_error",
            "targets"
        )
    )]
    async fn get_asr_display_menu_state(
        &self,
    ) -> Result<
        (
            String,
            String,
            String,
            String,
            bool,
            String,
            Vec<(String, String, String, String, String)>,
        ),
        VinpstDbusError,
    > {
        let model_root = self.runtime.lock().await.model_root();
        let installed_models = tokio::task::spawn_blocking(move || {
            model_root.map_or_else(
                || Ok(Vec::new()),
                |root| scan_installed_models(&root).map_err(RuntimeError::InstalledModels),
            )
        })
        .await
        .map_err(|error| {
            Self::operation_failed(format!("installed model scan task failed: {error}"))
        })?
        .map_err(|error| Self::map_runtime_error(&error))?;
        let locale_candidates = locale_candidates_from_environment();
        Ok(self
            .runtime
            .lock()
            .await
            .asr_display_menu_state(&installed_models, &locale_candidates))
    }

    /// Select, persist, and queue reload for a configured ASR provider/model target.
    #[zbus(name = "SetActiveAsrTarget")]
    async fn set_active_asr_target(
        &self,
        provider_id: &str,
        model_value: &str,
    ) -> Result<bool, VinpstDbusError> {
        let (config_source, config_path, model_root) = {
            let runtime = self.runtime.lock().await;
            (
                runtime.asr_reload_config_source(),
                runtime.config_path_for_persistence(),
                runtime.model_root(),
            )
        };
        let provider_id = provider_id.to_owned();
        let model_value = model_value.to_owned();
        let (config, persisted) = tokio::task::spawn_blocking(move || {
            let installed_models = model_root.map_or_else(
                || Ok(Vec::new()),
                |root| scan_installed_models(&root).map_err(RuntimeError::InstalledModels),
            )?;
            let config = config_source.load()?;
            let Some(provider) = config
                .asr
                .providers
                .iter()
                .find(|provider| provider.id == provider_id)
            else {
                return Err(RuntimeError::Asr(vinpst_asr::AsrError::UnknownProvider(
                    provider_id,
                )));
            };
            let configured_model_matches = provider.model.as_deref() == Some(model_value.as_str());
            let installed_model_matches = provider.kind == vinpst_config::AsrProviderKind::Local
                && installed_models
                    .iter()
                    .any(|model| model.config_model_value() == model_value);
            if !model_value.is_empty() && !configured_model_matches && !installed_model_matches {
                return Err(RuntimeError::UnknownAsrTarget {
                    provider: provider_id,
                    model: model_value,
                });
            }
            let config = select_asr_target(config, &provider_id, Some(&model_value))
                .map_err(RuntimeError::Asr)?;
            if let Some(path) = config_path {
                persist_config_atomically(&path, &config, "asr-target")?;
                Ok((config, true))
            } else {
                Ok((config, false))
            }
        })
        .await
        .map_err(|error| {
            Self::operation_failed(format!("ASR target selection task failed: {error}"))
        })?
        .map_err(|error: RuntimeError| Self::map_runtime_error(&error))?;
        self.queue_asr_reload_config(config).await?;
        Ok(persisted)
    }

    /// Reload ASR backend using the legacy void method signature.
    #[zbus(name = "ReloadAsrBackend")]
    async fn reload_asr_backend(&self) -> Result<(), VinpstDbusError> {
        let config_source = self.runtime.lock().await.asr_reload_config_source();
        let config = tokio::task::spawn_blocking(move || config_source.load())
            .await
            .map_err(|error| Self::operation_failed(format!("ASR reload task failed: {error}")))?
            .map_err(|error| Self::map_runtime_error(&error))?;
        self.queue_asr_reload_config(config).await?;
        Ok(())
    }

    /// Start a configured adapter using the runtime supervisor.
    #[zbus(name = "StartAdapter")]
    async fn start_adapter(&self, adapter_id: &str) -> Result<(), VinpstDbusError> {
        let mut runtime = self.runtime.lock().await;
        runtime
            .start_text_adapter(adapter_id)
            .map_err(|error| Self::map_runtime_error(&error))?;
        Ok(())
    }

    /// Stop a configured adapter using the runtime supervisor.
    #[zbus(name = "StopAdapter")]
    async fn stop_adapter(&self, adapter_id: &str) -> Result<(), VinpstDbusError> {
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
mod tests;
