//! Integration tests for the legacy D-Bus ABI exposed by `vinpst-daemon`.
#![cfg(feature = "dbus-integration")]

use std::{
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};

use futures_util::StreamExt;
use tokio::time::{sleep, timeout};
use vinpst_asr::{AsrBackendFactory, MockAsrBackend};
use vinpst_audio::{
    AudioChunkCallback, AudioError, AudioRecorder, CaptureTarget, CapturedAudio, MockAudioSource,
    PcmBuffer,
};
use vinpst_config::{AsrProviderConfig, AsrProviderKind, VinpstConfig};
use vinpst_daemon::{RuntimeState, VinpstDbusService};
use vinpst_protocol::{RecognitionPayload, TextAdapterState, dbus};
use vinpst_text::AdapterRuntimePaths;
use zbus::{Message, Proxy};

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

type AsrMenuStateTuple = (
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String)>,
);

type AsrTargetMenuStateTuple = (
    String,
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String, String)>,
);

type AsrDisplayMenuStateTuple = (
    String,
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String, String, String)>,
);

const RAW_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/raw.json"
));
static WELL_KNOWN_NAME_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn fixture_json(input: &str) -> &str {
    input.trim_end()
}

async fn spawn_service() -> anyhow::Result<zbus::Connection> {
    let config = VinpstConfig::bundled_default()?;
    let runtime = RuntimeState::new(config)?;
    let connection = VinpstDbusService::new(runtime)
        .serve_on_session_bus()
        .await?;
    Ok(connection)
}

#[tokio::test]
async fn second_service_cannot_replace_or_queue_behind_current_owner() -> anyhow::Result<()> {
    let _well_known_name_guard = WELL_KNOWN_NAME_TEST_LOCK.lock().await;
    let first = spawn_service().await?;
    let first_unique_name = first
        .unique_name()
        .ok_or_else(|| anyhow::anyhow!("first service should have a unique bus name"))?
        .to_owned();

    let second_runtime = RuntimeState::new(VinpstConfig::bundled_default()?)?;
    let error = VinpstDbusService::new(second_runtime)
        .serve_on_session_bus()
        .await
        .expect_err("a second daemon must not replace or queue behind the current owner");
    assert!(matches!(error, zbus::Error::NameTaken));

    let bus = zbus::fdo::DBusProxy::new(&first).await?;
    let owner = bus
        .get_name_owner(dbus::SERVICE_BUS_NAME.try_into()?)
        .await?;
    assert_eq!(owner, first_unique_name);
    assert!(first.release_name(dbus::SERVICE_BUS_NAME).await?);
    Ok(())
}

async fn spawn_runtime_on_unique_name(
    runtime: RuntimeState,
) -> anyhow::Result<(zbus::Connection, String)> {
    let connection = zbus::Connection::session().await?;
    let unique_name = connection
        .unique_name()
        .ok_or_else(|| anyhow::anyhow!("session connection should have a unique name"))?
        .to_string();
    let service = VinpstDbusService::new(runtime);
    service.bind_signal_connection(&connection).await?;
    connection
        .object_server()
        .at(dbus::SERVICE_OBJECT_PATH, service)
        .await?;
    Ok((connection, unique_name))
}

async fn get_asr_backend_state(proxy: &Proxy<'_>) -> zbus::Result<AsrBackendStateTuple> {
    proxy.call(dbus::method::GET_ASR_BACKEND_STATE, &()).await
}

async fn wait_for_asr_reload(proxy: &Proxy<'_>) -> anyhow::Result<AsrBackendStateTuple> {
    timeout(Duration::from_secs(2), async {
        loop {
            let state = get_asr_backend_state(proxy).await?;
            if !state.5 {
                return Ok::<_, zbus::Error>(state);
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("ASR reload did not finish"))?
    .map_err(Into::into)
}

fn configured_command_runtime() -> anyhow::Result<RuntimeState> {
    let config: VinpstConfig = serde_json::from_str(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "normalize_audio": false,
            "input_gain": 1.0,
            "providers": [{"id":"cmd","type":"command","command":"wc","args":["-c"]}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    )?;
    config.validate()?;
    let backend = AsrBackendFactory::build_active(&config.asr)?;
    let audio_source = MockAudioSource::once(CapturedAudio::named(
        PcmBuffer::at_default_rate(vec![1_000, -1_000, 2_000, -2_000]),
        "dbus-e2e",
    ));
    RuntimeState::with_configured_text(config, backend, Box::new(audio_source)).map_err(Into::into)
}

fn configured_streaming_command_runtime() -> anyhow::Result<RuntimeState> {
    let mut config = VinpstConfig::bundled_default()?;
    "cmd.streaming".clone_into(&mut config.asr.active_provider);
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd.streaming".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s
' '{"type":"partial","text":"bus partial"}' '{"type":"final","text":"bus streaming final"}' '{"type":"closed"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    config.validate()?;
    let backend = AsrBackendFactory::build_active(&config.asr)?;
    let audio_source = MockAudioSource::once(CapturedAudio::named(
        PcmBuffer::at_default_rate(vec![1_000, -1_000, 2_000, -2_000]),
        "dbus-streaming-command-e2e",
    ));
    RuntimeState::with_configured_text(config, backend, Box::new(audio_source)).map_err(Into::into)
}

#[derive(Clone, Default)]
struct ManualRecorderHandle {
    callback: Arc<StdMutex<Option<AudioChunkCallback>>>,
}

impl ManualRecorderHandle {
    fn emit(&self, pcm: &PcmBuffer) {
        let mut callback = self.callback.lock().expect("callback lock poisoned");
        callback
            .as_mut()
            .expect("recording callback should be installed")(pcm);
    }
}

struct ManualAudioRecorder {
    handle: ManualRecorderHandle,
    recording: bool,
    captured: CapturedAudio,
}

impl ManualAudioRecorder {
    fn new(captured: CapturedAudio) -> (Self, ManualRecorderHandle) {
        let handle = ManualRecorderHandle::default();
        (
            Self {
                handle: handle.clone(),
                recording: false,
                captured,
            },
            handle,
        )
    }
}

impl AudioRecorder for ManualAudioRecorder {
    fn begin_recording(&mut self, _target: CaptureTarget) -> Result<(), AudioError> {
        self.recording = true;
        Ok(())
    }

    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>) {
        *self.handle.callback.lock().expect("callback lock poisoned") = callback;
    }

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        self.recording = false;
        Ok(self.captured.clone())
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.recording = false;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

fn live_partial_runtime() -> anyhow::Result<(RuntimeState, ManualRecorderHandle)> {
    let config = VinpstConfig::bundled_default()?;
    let backend = MockAsrBackend::streaming("live bus partial", "live bus final");
    let captured = CapturedAudio::named(
        PcmBuffer::at_default_rate(vec![1_000; 800]),
        "manual-live-partial",
    );
    let (recorder, handle) = ManualAudioRecorder::new(captured);
    let runtime = RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder))?;
    Ok((runtime, handle))
}

fn configured_command_text_runtime() -> anyhow::Result<RuntimeState> {
    let config: VinpstConfig = serde_json::from_str(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "normalize_audio": false,
            "input_gain": 1.0,
            "providers": [{"id":"cmd","type":"command","command":"sh","args":["-c","cat >/dev/null; printf raw-bus"]}]
          },
          "llm": {
            "adapters": [{"id":"cmd-adapter","command":"python3","args":["-c", "import sys; sys.stdin.read(); print('{\\\"text\\\":\\\"bus adapter final\\\"}')"]}]
          },
          "scenes": {
            "active_scene": "needs-adapter",
            "definitions": [{"id":"needs-adapter","label":"Needs adapter","prompt":"polish","candidate_count":1}]
          }
        }
        "#,
    )?;
    config.validate()?;
    let backend = AsrBackendFactory::build_active(&config.asr)?;
    let audio_source = MockAudioSource::once(CapturedAudio::named(
        PcmBuffer::at_default_rate(vec![1_000, -1_000, 2_000, -2_000]),
        "dbus-text-e2e",
    ));
    RuntimeState::with_configured_text(config, backend, Box::new(audio_source)).map_err(Into::into)
}

async fn next_string_signal(stream: &mut zbus::proxy::SignalStream<'_>) -> anyhow::Result<String> {
    let message = timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("signal stream ended"))?;
    single_string_body(&message)
}

async fn next_error_info_signal(
    stream: &mut zbus::proxy::SignalStream<'_>,
) -> anyhow::Result<(String, String, String, String)> {
    let message = timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("signal stream ended"))?;
    Ok(message.body().deserialize()?)
}

fn single_string_body(message: &Message) -> anyhow::Result<String> {
    let body: (String,) = message.body().deserialize()?;
    Ok(body.0)
}

fn interface_block<'a>(xml: &'a str, interface: &str) -> anyhow::Result<&'a str> {
    let needle = format!(r#"<interface name="{interface}">"#);
    let start = xml
        .find(&needle)
        .ok_or_else(|| anyhow::anyhow!("interface {interface} missing from introspection XML"))?;
    let body_start = start + needle.len();
    let end = xml[body_start..]
        .find("</interface>")
        .ok_or_else(|| anyhow::anyhow!("interface {interface} is not closed"))?;
    Ok(&xml[body_start..body_start + end])
}

fn member_block<'a>(interface_xml: &'a str, kind: &str, name: &str) -> anyhow::Result<&'a str> {
    let needle = format!(r#"<{kind} name="{name}">"#);
    let start = interface_xml
        .find(&needle)
        .ok_or_else(|| anyhow::anyhow!("{kind} {name} missing from introspection XML"))?;
    let body_start = start + needle.len();
    let end_tag = format!("</{kind}>");
    let end = interface_xml[body_start..]
        .find(&end_tag)
        .ok_or_else(|| anyhow::anyhow!("{kind} {name} is not closed"))?;
    Ok(&interface_xml[body_start..body_start + end])
}

fn arg_signature(member_xml: &str, direction: Option<&str>) -> String {
    let direction_attr = direction.map(|direction| format!(r#"direction="{direction}""#));
    let mut signature = String::new();
    for line in member_xml.lines() {
        if !line.contains("<arg ") {
            continue;
        }
        if let Some(direction_attr) = &direction_attr
            && !line.contains(direction_attr)
        {
            continue;
        }
        if let Some(type_start) = line.find(r#"type=""#) {
            let value_start = type_start + r#"type=""#.len();
            if let Some(value_end) = line[value_start..].find('"') {
                signature.push_str(&line[value_start..value_start + value_end]);
            }
        }
    }
    signature
}

fn assert_method_signature(
    interface_xml: &str,
    name: &str,
    input_signature: &str,
    output_signature: &str,
) -> anyhow::Result<()> {
    let method_xml = member_block(interface_xml, "method", name)?;
    assert_eq!(
        arg_signature(method_xml, Some("in")),
        input_signature,
        "unexpected input signature for method {name}; XML: {method_xml}"
    );
    assert_eq!(
        arg_signature(method_xml, Some("out")),
        output_signature,
        "unexpected output signature for method {name}; XML: {method_xml}"
    );
    Ok(())
}

fn assert_member_missing(interface_xml: &str, kind: &str, name: &str) {
    let needle = format!(r#"<{kind} name="{name}">"#);
    assert!(
        !interface_xml.contains(&needle),
        "{kind} {name} should not be exported on the daemon service interface"
    );
}

fn assert_signal_signature(interface_xml: &str, name: &str, signature: &str) -> anyhow::Result<()> {
    let signal_xml = member_block(interface_xml, "signal", name)?;
    assert_eq!(
        arg_signature(signal_xml, None),
        signature,
        "unexpected signature for signal {name}; XML: {signal_xml}"
    );
    Ok(())
}

fn assert_legacy_operation_failed(error: &zbus::Error, expected_message: &str) {
    match error {
        zbus::Error::MethodError(name, Some(description), _) => {
            assert_eq!(name.as_str(), dbus::error::OPERATION_FAILED);
            assert!(
                description.contains(expected_message),
                "unexpected operation failure description: {description}"
            );
        }
        other => panic!("expected legacy operation failure, got: {other}"),
    }
}

async fn expect_no_string_signal(stream: &mut zbus::proxy::SignalStream<'_>) -> anyhow::Result<()> {
    match timeout(Duration::from_millis(150), stream.next()).await {
        Err(_) => Ok(()),
        Ok(None) => anyhow::bail!("signal stream ended"),
        Ok(Some(message)) => {
            let value = single_string_body(&message)
                .unwrap_or_else(|error| format!("<unreadable signal body: {error}>"));
            anyhow::bail!("unexpected string signal: {value}");
        }
    }
}

#[tokio::test]
async fn dbus_get_runtime_status_returns_json_snapshot() -> anyhow::Result<()> {
    let runtime = RuntimeState::new(VinpstConfig::bundled_default()?)?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    let status_json: String = proxy.call(dbus::method::GET_RUNTIME_STATUS, &()).await?;
    let status: serde_json::Value = serde_json::from_str(&status_json)?;
    assert_eq!(status["ok"], true);
    assert_eq!(status["status"], "idle");
    assert_eq!(status["active_session"], false);
    assert_eq!(status["selected_text_present"], false);
    assert_eq!(status["current_scene"], serde_json::Value::Null);
    assert_eq!(status["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(status["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(status["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    assert_eq!(status["asr"]["effective_provider_id"], "mock");
    assert_eq!(status["asr"]["target_provider_id"], "sherpa-onnx");
    assert_eq!(status["asr"]["remote_endpoints"], serde_json::json!([]));
    assert_eq!(status["remote_text"]["running"], false);
    assert_eq!(
        status["remote_text"]["listen_addr"],
        serde_json::Value::Null
    );
    assert_eq!(status["remote_text"]["endpoints"], serde_json::json!([]));
    assert_eq!(status["text_adapters"]["adapter_count"], 0);
    assert!(status["uptime_ms"].as_u64().is_some());

    proxy
        .call::<_, _, ()>(dbus::method::START_COMMAND_RECORDING, &"selected text")
        .await?;
    let active_status_json: String = proxy.call(dbus::method::GET_RUNTIME_STATUS, &()).await?;
    let active_status: serde_json::Value = serde_json::from_str(&active_status_json)?;
    assert_eq!(active_status["status"], "recording");
    assert_eq!(active_status["active_session"], true);
    assert_eq!(active_status["selected_text_present"], true);
    assert_eq!(active_status["current_scene"], "__command__");
    assert!(
        !active_status_json.contains("selected text"),
        "runtime status must not expose selected text content"
    );

    Ok(())
}

#[tokio::test]
async fn background_asr_reload_failure_emits_daemon_notification() -> anyhow::Result<()> {
    let config = VinpstConfig::bundled_default()?;
    let runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::buffered("injected final")),
    )?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;
    let mut notifications = proxy
        .receive_signal(dbus::signal::DAEMON_NOTIFICATION)
        .await?;

    proxy
        .call::<_, _, ()>(dbus::method::RELOAD_ASR_BACKEND, &())
        .await?;
    let (code, subject, detail, raw_message) = next_error_info_signal(&mut notifications).await?;
    assert_eq!(code, "asr_backend_reload_failed");
    assert!(subject.is_empty());
    assert!(detail.is_empty());
    assert!(raw_message.contains("Failed to reload ASR backend."));

    let state = wait_for_asr_reload(&proxy).await?;
    assert!(!state.5);
    assert_eq!(state.4, raw_message);
    assert_eq!(state.3, "mock-buffered");
    Ok(())
}

#[tokio::test]
async fn dbus_reload_rereads_config_and_rebuilds_backend() -> anyhow::Result<()> {
    let runtime_config = VinpstConfig::bundled_default()?;
    let mut reload_config = runtime_config.clone();
    reload_config.asr.active_provider = "mock".to_owned();
    reload_config.asr.providers.push(AsrProviderConfig {
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
    let temp = tempfile::tempdir()?;
    let config_path = temp.path().join("config.json");
    std::fs::write(&config_path, serde_json::to_vec_pretty(&reload_config)?)?;
    let mut runtime = RuntimeState::with_asr_backend(
        runtime_config,
        Box::new(MockAsrBackend::buffered("injected final")),
    )?;
    runtime.set_config_path(Some(config_path));
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    let before = get_asr_backend_state(&proxy).await?;
    assert_eq!(before.0, "sherpa-onnx");
    assert_eq!(before.3, "mock-buffered");

    proxy
        .call::<_, _, ()>(dbus::method::RELOAD_ASR_BACKEND, &())
        .await?;
    let after = wait_for_asr_reload(&proxy).await?;
    assert_eq!(after.0, "mock");
    assert_eq!(after.2, "mock");
    assert_eq!(after.3, "mock-streaming");
    assert!(after.4.is_empty());
    assert!(!after.5);

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "mock recognition result");

    Ok(())
}

#[tokio::test]
async fn scene_selection_persists_through_session_bus() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let config_path = temp.path().join("config.json");
    let config = VinpstConfig::bundled_default()?;
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
    let mut runtime = RuntimeState::new(config)?;
    runtime.set_config_path(Some(config_path.clone()));
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    let before: (String, Vec<(String, String)>) =
        proxy.call(dbus::method::GET_SCENE_STATE, &()).await?;
    assert_eq!(before.0, vinpst_config::RAW_SCENE_ID);
    assert_eq!(before.1[0].1, "Raw");

    let persisted: bool = proxy
        .call(
            dbus::method::SET_ACTIVE_SCENE,
            &vinpst_config::COMMAND_SCENE_ID,
        )
        .await?;
    assert!(persisted);
    let after: (String, Vec<(String, String)>) =
        proxy.call(dbus::method::GET_SCENE_STATE, &()).await?;
    assert_eq!(after.0, vinpst_config::COMMAND_SCENE_ID);
    let persisted_config = VinpstConfig::from_json_file(&config_path)?;
    assert_eq!(
        persisted_config.scenes.active_scene,
        vinpst_config::COMMAND_SCENE_ID
    );

    let unknown: zbus::Result<bool> = proxy.call(dbus::method::SET_ACTIVE_SCENE, &"missing").await;
    assert_legacy_operation_failed(&unknown.unwrap_err(), "scene `missing` is not configured");

    Ok(())
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn asr_provider_selection_persists_and_reloads_through_session_bus() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let config_path = temp.path().join("config.json");
    let model_root = temp.path().join("models");
    let model_dir = model_root.join("installed-one");
    std::fs::create_dir_all(&model_dir)?;
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        r#"{
          "backend":"sherpa-offline",
          "family":"moonshine",
          "display":{
            "registry_id":"model.test.installed-one",
            "fallback_title":"Installed Model Title"
          }
        }"#,
    )?;
    let mut config = VinpstConfig::bundled_default()?;
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("mock-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config)?)?;
    let mut runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::buffered("injected final")),
    )?;
    runtime.set_config_path(Some(config_path.clone()));
    runtime.set_model_root(Some(model_root));
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    let before: AsrMenuStateTuple = proxy.call(dbus::method::GET_ASR_MENU_STATE, &()).await?;
    assert_eq!(before.0, "sherpa-onnx");
    assert_eq!(before.1, "mock");
    assert_eq!(before.2, "mock-buffered");
    assert_eq!(before.5[1].0, "mock");

    let persisted: bool = proxy
        .call(dbus::method::SET_ACTIVE_ASR_PROVIDER, &"mock")
        .await?;
    assert!(persisted);
    let after = wait_for_asr_reload(&proxy).await?;
    assert_eq!(after.0, "mock");
    assert_eq!(after.2, "mock");
    assert_eq!(after.3, "mock-streaming");
    assert!(after.4.is_empty());
    let persisted_config = VinpstConfig::from_json_file(&config_path)?;
    assert_eq!(persisted_config.asr.active_provider, "mock");

    let target_state: AsrTargetMenuStateTuple = proxy
        .call(dbus::method::GET_ASR_TARGET_MENU_STATE, &())
        .await?;
    let model_value = model_dir.to_string_lossy().into_owned();
    assert!(
        target_state
            .6
            .iter()
            .any(|item| { item.0 == "mock" && item.2 == "installed-one" && item.3 == model_value })
    );

    let display_state: AsrDisplayMenuStateTuple = proxy
        .call(dbus::method::GET_ASR_DISPLAY_MENU_STATE, &())
        .await?;
    assert!(display_state.6.iter().any(|item| {
        item.0 == "mock"
            && item.2 == "model.test.installed-one"
            && item.3 == "Installed Model Title"
            && item.4 == model_value
    }));
    let target_persisted: bool = proxy
        .call(
            dbus::method::SET_ACTIVE_ASR_TARGET,
            &("mock", model_value.as_str()),
        )
        .await?;
    assert!(target_persisted);
    let target_after = wait_for_asr_reload(&proxy).await?;
    assert_eq!(target_after.0, "mock");
    assert_eq!(target_after.1, model_value);
    assert_eq!(target_after.2, "mock");
    assert_eq!(target_after.3, "mock-streaming");
    let persisted_config = VinpstConfig::from_json_file(&config_path)?;
    assert_eq!(
        persisted_config.asr.providers[1].model.as_deref(),
        Some(model_value.as_str())
    );

    let unknown_target: zbus::Result<bool> = proxy
        .call(
            dbus::method::SET_ACTIVE_ASR_TARGET,
            &("mock", "/not/an/installed/model"),
        )
        .await;
    assert_legacy_operation_failed(
        &unknown_target.unwrap_err(),
        "ASR target `mock` / `/not/an/installed/model` is not configured or installed",
    );

    let unknown: zbus::Result<bool> = proxy
        .call(dbus::method::SET_ACTIVE_ASR_PROVIDER, &"missing")
        .await;
    assert_legacy_operation_failed(
        &unknown.unwrap_err(),
        "ASR provider `missing` is not configured",
    );

    Ok(())
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn legacy_dbus_methods_roundtrip_through_session_bus() -> anyhow::Result<()> {
    let _well_known_name_guard = WELL_KNOWN_NAME_TEST_LOCK.lock().await;
    let service_connection = spawn_service().await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    let introspection_proxy = Proxy::new(
        &client_connection,
        dbus::SERVICE_BUS_NAME,
        dbus::SERVICE_OBJECT_PATH,
        "org.freedesktop.DBus.Introspectable",
    )
    .await?;
    let xml: String = introspection_proxy.call("Introspect", &()).await?;
    let interface_xml = interface_block(&xml, dbus::SERVICE_INTERFACE)?;
    assert_method_signature(interface_xml, dbus::method::START_RECORDING, "", "")?;
    assert_method_signature(
        interface_xml,
        dbus::method::START_COMMAND_RECORDING,
        "s",
        "",
    )?;
    assert_method_signature(interface_xml, dbus::method::STOP_RECORDING, "s", "s")?;
    assert_method_signature(interface_xml, dbus::method::GET_STATUS, "", "s")?;
    assert_method_signature(
        interface_xml,
        dbus::method::GET_ASR_BACKEND_STATE,
        "",
        "sssssbbas",
    )?;
    assert_method_signature(interface_xml, dbus::method::GET_TEXT_ADAPTER_STATE, "", "s")?;
    assert_method_signature(interface_xml, dbus::method::GET_RUNTIME_STATUS, "", "s")?;
    assert_method_signature(interface_xml, dbus::method::GET_SCENE_STATE, "", "sa(ss)")?;
    assert_method_signature(interface_xml, dbus::method::SET_ACTIVE_SCENE, "s", "b")?;
    assert_method_signature(
        interface_xml,
        dbus::method::GET_ASR_MENU_STATE,
        "",
        "sssbsa(sss)",
    )?;
    assert_method_signature(
        interface_xml,
        dbus::method::SET_ACTIVE_ASR_PROVIDER,
        "s",
        "b",
    )?;
    assert_method_signature(
        interface_xml,
        dbus::method::GET_ASR_TARGET_MENU_STATE,
        "",
        "ssssbsa(ssss)",
    )?;
    assert_method_signature(
        interface_xml,
        dbus::method::SET_ACTIVE_ASR_TARGET,
        "ss",
        "b",
    )?;
    assert_method_signature(
        interface_xml,
        dbus::method::GET_ASR_DISPLAY_MENU_STATE,
        "",
        "ssssbsa(sssss)",
    )?;
    assert_method_signature(interface_xml, dbus::method::RELOAD_ASR_BACKEND, "", "")?;
    assert_method_signature(interface_xml, dbus::method::START_ADAPTER, "s", "")?;
    assert_method_signature(interface_xml, dbus::method::STOP_ADAPTER, "s", "")?;
    assert_signal_signature(interface_xml, dbus::signal::RECOGNITION_RESULT, "s")?;
    assert_signal_signature(interface_xml, dbus::signal::RECOGNITION_PARTIAL, "s")?;
    assert_signal_signature(interface_xml, dbus::signal::STATUS_CHANGED, "s")?;
    assert_signal_signature(
        interface_xml,
        dbus::signal::DAEMON_NOTIFICATION,
        dbus::signature::ERROR_INFO,
    )?;
    assert_member_missing(interface_xml, "method", dbus::method::NOTIFY);

    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut partial_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_PARTIAL)
        .await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;
    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    let idle_stop: zbus::Result<String> = proxy.call(dbus::method::STOP_RECORDING, &"").await;
    let idle_stop_error = idle_stop.expect_err("idle stop should fail");
    assert_legacy_operation_failed(&idle_stop_error, "runtime is not recording: idle");
    expect_no_string_signal(&mut status_signals).await?;

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "recording");
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    expect_no_string_signal(&mut partial_signals).await?;

    let duplicate_start: zbus::Result<()> = proxy.call(dbus::method::START_RECORDING, &()).await;
    let duplicate_start_error = duplicate_start.expect_err("duplicate start should fail");
    assert_legacy_operation_failed(&duplicate_start_error, "runtime is busy");

    let command_while_recording: zbus::Result<()> = proxy
        .call(dbus::method::START_COMMAND_RECORDING, &"ignored selection")
        .await;
    let command_while_recording_error =
        command_while_recording.expect_err("command start while recording should fail");
    assert_legacy_operation_failed(&command_while_recording_error, "runtime is busy: recording");

    proxy
        .call::<_, _, ()>(dbus::method::RELOAD_ASR_BACKEND, &())
        .await?;
    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "recording");
    let state: (
        String,
        String,
        String,
        String,
        String,
        bool,
        bool,
        Vec<String>,
    ) = proxy.call(dbus::method::GET_ASR_BACKEND_STATE, &()).await?;
    assert!(
        state.5,
        "reload while recording should be reported as pending"
    );
    expect_no_string_signal(&mut status_signals).await?;

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    assert_eq!(payload_json, fixture_json(RAW_PAYLOAD_JSON));
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.candidates.len(), 1);
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    assert_eq!(
        next_string_signal(&mut status_signals).await?,
        "postprocessing"
    );
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "mock partial"
    );
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    assert_eq!(result_payload_json, fixture_json(RAW_PAYLOAD_JSON));
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload, payload);
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");
    let state = wait_for_asr_reload(&proxy).await?;
    assert!(!state.5, "pending reload should finish once idle");

    proxy
        .call::<_, _, ()>(dbus::method::START_COMMAND_RECORDING, &"selected text")
        .await?;
    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "recording");
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    expect_no_string_signal(&mut partial_signals).await?;

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(
        payload.commit_text,
        "mock command result for: selected text"
    );
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    assert_eq!(
        next_string_signal(&mut status_signals).await?,
        "postprocessing"
    );
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "mock partial"
    );
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(
        signal_payload.commit_text,
        "mock command result for: selected text"
    );
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    let state = wait_for_asr_reload(&proxy).await?;
    assert!(state.6);
    assert_eq!(state.0, "sherpa-onnx");
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
    assert!(
        state.4.contains("Failed to reload ASR backend"),
        "unexpected deferred reload error: {}",
        state.4
    );

    let text_adapter_state_json: String = proxy
        .call(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .await?;
    let text_adapter_state: TextAdapterState = serde_json::from_str(&text_adapter_state_json)?;
    assert_eq!(text_adapter_state.adapter_count, 0);
    assert!(text_adapter_state.adapter_ids.is_empty());
    assert!(text_adapter_state.adapters.is_empty());
    assert!(text_adapter_state.single_adapter_id.is_none());

    proxy
        .call::<_, _, ()>(dbus::method::RELOAD_ASR_BACKEND, &())
        .await?;
    let state = wait_for_asr_reload(&proxy).await?;
    assert_eq!(state.2, "mock");
    assert_eq!(state.3, "mock-streaming");
    assert!(state.6);
    assert!(state.4.contains("Failed to reload ASR backend"));
    let adapter_start: zbus::Result<()> = proxy
        .call(dbus::method::START_ADAPTER, &"mock-adapter")
        .await;
    let adapter_start_error = adapter_start.expect_err("unconfigured adapter start should fail");
    assert_legacy_operation_failed(
        &adapter_start_error,
        "adapter `mock-adapter` is not configured",
    );
    let adapter_stop: zbus::Result<()> = proxy
        .call(dbus::method::STOP_ADAPTER, &"mock-adapter")
        .await;
    let adapter_stop_error = adapter_stop.expect_err("unconfigured adapter stop should fail");
    assert_legacy_operation_failed(
        &adapter_stop_error,
        "adapter `mock-adapter` is not configured",
    );
    let empty_adapter_start: zbus::Result<()> = proxy.call(dbus::method::START_ADAPTER, &"").await;
    let empty_adapter_start_error =
        empty_adapter_start.expect_err("empty adapter start should fail");
    assert_legacy_operation_failed(&empty_adapter_start_error, "adapter `` is not configured");
    let empty_adapter_stop: zbus::Result<()> = proxy.call(dbus::method::STOP_ADAPTER, &"").await;
    let empty_adapter_stop_error = empty_adapter_stop.expect_err("empty adapter stop should fail");
    assert_legacy_operation_failed(&empty_adapter_stop_error, "adapter `` is not configured");

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    assert!(
        service_connection
            .release_name(dbus::SERVICE_BUS_NAME)
            .await?
    );
    Ok(())
}

#[tokio::test]
async fn early_final_roundtrips_through_session_bus() -> anyhow::Result<()> {
    let config = VinpstConfig::bundled_default()?;
    let runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::streaming_with_early_final(
            "early partial",
            "early final",
        )),
    )?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;
    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut partial_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_PARTIAL)
        .await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    expect_no_string_signal(&mut partial_signals).await?;

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "early final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    assert_eq!(
        next_string_signal(&mut status_signals).await?,
        "postprocessing"
    );
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "early partial"
    );
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text, "early final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    Ok(())
}

#[tokio::test]
async fn configured_command_backend_roundtrips_through_session_bus() -> anyhow::Result<()> {
    let runtime = configured_command_runtime()?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;
    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text.trim(), "8");
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    assert_eq!(
        next_string_signal(&mut status_signals).await?,
        "postprocessing"
    );
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text.trim(), "8");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    Ok(())
}

fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "vinpst-dbus-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ))
}

#[tokio::test]
async fn chunked_runtime_emits_partial_before_stop_without_duplicate() -> anyhow::Result<()> {
    let (runtime, recorder) = live_partial_runtime()?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;
    let mut partial_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_PARTIAL)
        .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    recorder.emit(&PcmBuffer::at_default_rate(vec![1_000; 800]));
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "live bus partial"
    );

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "live bus final");
    expect_no_string_signal(&mut partial_signals).await?;

    Ok(())
}

#[tokio::test]
async fn configured_streaming_command_backend_emits_stop_partial_signal() -> anyhow::Result<()> {
    let runtime = configured_streaming_command_runtime()?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;
    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut partial_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_PARTIAL)
        .await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    expect_no_string_signal(&mut partial_signals).await?;

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "bus streaming final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    assert_eq!(
        next_string_signal(&mut status_signals).await?,
        "postprocessing"
    );
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "bus partial"
    );
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text, "bus streaming final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    Ok(())
}

#[tokio::test]
async fn configured_adapter_supervision_roundtrips_through_session_bus() -> anyhow::Result<()> {
    let runtime_dir = unique_adapter_runtime_dir("adapter-supervision");
    let pid_path = runtime_dir.join("cmd-adapter.pid");
    let config: VinpstConfig = serde_json::from_str(
        r#"
        {
          "version": 1,
          "llm": {
            "adapters": [{"id":"cmd-adapter","command":"sleep","args":["30"]}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    )?;
    config.validate()?;
    let runtime = RuntimeState::new(config)?
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_ADAPTER, &"cmd-adapter")
        .await?;
    assert!(pid_path.exists(), "adapter start should write pid file");
    let text_adapter_state_json: String = proxy
        .call(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .await?;
    let text_adapter_state: TextAdapterState = serde_json::from_str(&text_adapter_state_json)?;
    assert!(text_adapter_state.adapters[0].is_running);
    assert!(text_adapter_state.adapters[0].pid.is_some());

    let duplicate_start: zbus::Result<()> = proxy
        .call(dbus::method::START_ADAPTER, &"cmd-adapter")
        .await;
    let duplicate_error = duplicate_start.expect_err("duplicate adapter start should fail");
    assert_legacy_operation_failed(&duplicate_error, "already running");

    proxy
        .call::<_, _, ()>(dbus::method::STOP_ADAPTER, &"cmd-adapter")
        .await?;
    assert!(!pid_path.exists(), "adapter stop should remove pid file");
    let text_adapter_state_json: String = proxy
        .call(dbus::method::GET_TEXT_ADAPTER_STATE, &())
        .await?;
    let text_adapter_state: TextAdapterState = serde_json::from_str(&text_adapter_state_json)?;
    assert!(!text_adapter_state.adapters[0].is_running);
    assert_eq!(text_adapter_state.adapters[0].pid, None);
    proxy
        .call::<_, _, ()>(dbus::method::STOP_ADAPTER, &"cmd-adapter")
        .await?;
    let _ = std::fs::remove_dir_all(runtime_dir);

    Ok(())
}

#[tokio::test]
async fn exited_adapter_is_reaped_from_dbus_diagnostics() -> anyhow::Result<()> {
    let runtime_dir = unique_adapter_runtime_dir("adapter-reap");
    let pid_path = runtime_dir.join("cmd-adapter.pid");
    let config: VinpstConfig = serde_json::from_str(
        r#"
        {
          "version": 1,
          "llm": {
            "adapters": [{"id":"cmd-adapter","command":"true"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    )?;
    config.validate()?;
    let runtime = RuntimeState::new(config)?
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_ADAPTER, &"cmd-adapter")
        .await?;
    assert!(pid_path.exists(), "adapter start should write pid file");

    let mut text_adapter_state = None;
    for _ in 0..20 {
        let state_json: String = proxy
            .call(dbus::method::GET_TEXT_ADAPTER_STATE, &())
            .await?;
        let state: TextAdapterState = serde_json::from_str(&state_json)?;
        if !state.adapters[0].is_running {
            text_adapter_state = Some(state);
            break;
        }
        sleep(Duration::from_millis(10)).await;
    }
    let text_adapter_state = text_adapter_state
        .ok_or_else(|| anyhow::anyhow!("adapter should be reaped from D-Bus diagnostics"))?;
    assert_eq!(text_adapter_state.adapters[0].pid, None);
    assert!(!pid_path.exists(), "adapter reap should remove pid file");
    let _ = std::fs::remove_dir_all(runtime_dir);

    Ok(())
}

#[tokio::test]
async fn configured_text_adapter_roundtrips_through_session_bus() -> anyhow::Result<()> {
    let runtime = configured_command_text_runtime()?;
    let (_service_connection, service_name) = spawn_runtime_on_unique_name(runtime).await?;
    let client_connection = zbus::Connection::session().await?;
    let proxy = Proxy::new(
        &client_connection,
        service_name.as_str(),
        dbus::SERVICE_OBJECT_PATH,
        dbus::SERVICE_INTERFACE,
    )
    .await?;
    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "bus adapter final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    assert_eq!(
        next_string_signal(&mut status_signals).await?,
        "postprocessing"
    );
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text, "bus adapter final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    Ok(())
}
