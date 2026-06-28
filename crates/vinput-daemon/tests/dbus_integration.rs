//! Integration tests for the legacy D-Bus ABI exposed by `vinput-daemon`.
#![cfg(feature = "dbus-integration")]

use std::time::Duration;

use futures_util::StreamExt;
use tokio::time::timeout;
use vinput_asr::AsrBackendFactory;
use vinput_audio::{CapturedAudio, MockAudioSource, PcmBuffer};
use vinput_config::VinputConfig;
use vinput_daemon::{RuntimeState, VinputDbusService};
use vinput_protocol::{RecognitionPayload, TextAdapterState, dbus};
use vinput_text::AdapterRuntimePaths;
use zbus::{Message, Proxy};

async fn spawn_service() -> anyhow::Result<zbus::Connection> {
    let config = VinputConfig::bundled_default()?;
    let runtime = RuntimeState::new(config)?;
    let connection = VinputDbusService::new(runtime)
        .serve_on_session_bus()
        .await?;
    Ok(connection)
}

async fn spawn_runtime_on_unique_name(
    runtime: RuntimeState,
) -> anyhow::Result<(zbus::Connection, String)> {
    let connection = zbus::Connection::session().await?;
    let unique_name = connection
        .unique_name()
        .ok_or_else(|| anyhow::anyhow!("session connection should have a unique name"))?
        .to_string();
    connection
        .object_server()
        .at(dbus::SERVICE_OBJECT_PATH, VinputDbusService::new(runtime))
        .await?;
    Ok((connection, unique_name))
}

fn configured_command_runtime() -> anyhow::Result<RuntimeState> {
    let config: VinputConfig = serde_json::from_str(
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
    let audio_source = MockAudioSource::from_frames(vec![
        CapturedAudio::named(PcmBuffer::at_default_rate(Vec::<i16>::new()), "warm-up"),
        CapturedAudio::named(
            PcmBuffer::at_default_rate(vec![1_000, -1_000, 2_000, -2_000]),
            "dbus-e2e",
        ),
    ]);
    RuntimeState::with_configured_text(config, backend, Box::new(audio_source)).map_err(Into::into)
}

fn configured_command_text_runtime() -> anyhow::Result<RuntimeState> {
    let config: VinputConfig = serde_json::from_str(
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
            "adapters": [{"id":"cmd-adapter","command":"sh","args":["-c","cat >/dev/null; printf '{\\\"text\\\":\\\"bus adapter final\\\"}'"]}]
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
    let audio_source = MockAudioSource::from_frames(vec![
        CapturedAudio::named(PcmBuffer::at_default_rate(Vec::<i16>::new()), "warm-up"),
        CapturedAudio::named(
            PcmBuffer::at_default_rate(vec![1_000, -1_000, 2_000, -2_000]),
            "dbus-text-e2e",
        ),
    ]);
    RuntimeState::with_configured_text(config, backend, Box::new(audio_source)).map_err(Into::into)
}

async fn next_string_signal(stream: &mut zbus::proxy::SignalStream<'_>) -> anyhow::Result<String> {
    let message = timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("signal stream ended"))?;
    single_string_body(&message)
}

fn single_string_body(message: &Message) -> anyhow::Result<String> {
    let body: (String,) = message.body().deserialize()?;
    Ok(body.0)
}

async fn next_pair_signal(
    stream: &mut zbus::proxy::SignalStream<'_>,
) -> anyhow::Result<(String, String)> {
    let message = timeout(Duration::from_secs(2), stream.next())
        .await?
        .ok_or_else(|| anyhow::anyhow!("signal stream ended"))?;
    let body: (String, String) = message.body().deserialize()?;
    Ok(body)
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

fn assert_signal_signature(interface_xml: &str, name: &str, signature: &str) -> anyhow::Result<()> {
    let signal_xml = member_block(interface_xml, "signal", name)?;
    assert_eq!(
        arg_signature(signal_xml, None),
        signature,
        "unexpected signature for signal {name}; XML: {signal_xml}"
    );
    Ok(())
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

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn legacy_dbus_methods_roundtrip_through_session_bus() -> anyhow::Result<()> {
    let _service_connection = spawn_service().await?;
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
    assert_method_signature(interface_xml, dbus::method::RELOAD_ASR_BACKEND, "", "")?;
    assert_method_signature(interface_xml, dbus::method::START_ADAPTER, "s", "")?;
    assert_method_signature(interface_xml, dbus::method::STOP_ADAPTER, "s", "")?;
    assert_signal_signature(interface_xml, dbus::signal::RECOGNITION_RESULT, "s")?;
    assert_signal_signature(interface_xml, dbus::signal::RECOGNITION_PARTIAL, "s")?;
    assert_signal_signature(interface_xml, dbus::signal::STATUS_CHANGED, "s")?;
    assert_signal_signature(interface_xml, dbus::signal::DAEMON_NOTIFICATION, "ss")?;

    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut partial_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_PARTIAL)
        .await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;
    let mut notification_signals = proxy
        .receive_signal(dbus::signal::DAEMON_NOTIFICATION)
        .await?;

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    let idle_stop: zbus::Result<String> = proxy.call(dbus::method::STOP_RECORDING, &"").await;
    let idle_stop_error = idle_stop.expect_err("idle stop should fail");
    assert!(
        idle_stop_error
            .to_string()
            .contains("runtime is not recording: idle"),
        "unexpected idle stop error: {idle_stop_error}"
    );
    expect_no_string_signal(&mut status_signals).await?;

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    proxy
        .call::<_, _, ()>(dbus::method::START_RECORDING, &())
        .await?;
    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "recording");
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "mock partial"
    );

    let duplicate_start: zbus::Result<()> = proxy.call(dbus::method::START_RECORDING, &()).await;
    let duplicate_start_error = duplicate_start.expect_err("duplicate start should fail");
    assert!(
        duplicate_start_error
            .to_string()
            .contains("runtime is busy"),
        "unexpected duplicate start error: {duplicate_start_error}"
    );

    let command_while_recording: zbus::Result<()> = proxy
        .call(dbus::method::START_COMMAND_RECORDING, &"ignored selection")
        .await;
    let command_while_recording_error =
        command_while_recording.expect_err("command start while recording should fail");
    assert!(
        command_while_recording_error
            .to_string()
            .contains("runtime is busy: recording"),
        "unexpected command start while recording error: {command_while_recording_error}"
    );
    expect_no_string_signal(&mut status_signals).await?;

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "mock recognition result");
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text, "mock recognition result");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    proxy
        .call::<_, _, ()>(dbus::method::START_COMMAND_RECORDING, &"selected text")
        .await?;
    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "recording");
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "mock partial"
    );

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(
        payload.commit_text,
        "mock command result for: selected text"
    );
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(
        signal_payload.commit_text,
        "mock command result for: selected text"
    );
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    let notification: String = proxy
        .call(dbus::method::NOTIFY, &("summary", "body"))
        .await?;
    assert_eq!(notification, "summary: body");
    assert_eq!(
        next_pair_signal(&mut notification_signals).await?,
        ("summary".to_owned(), "body".to_owned())
    );

    let empty_notification: String = proxy.call(dbus::method::NOTIFY, &("", "")).await?;
    assert_eq!(empty_notification, ": ");
    assert_eq!(
        next_pair_signal(&mut notification_signals).await?,
        (String::new(), String::new())
    );

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
    assert!(!state.6);
    assert_eq!(state.0, "sherpa-onnx");
    assert!(!state.4.is_empty());

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
    let adapter_start: zbus::Result<()> = proxy
        .call(dbus::method::START_ADAPTER, &"mock-adapter")
        .await;
    let adapter_start_error = adapter_start.expect_err("unconfigured adapter start should fail");
    assert!(
        adapter_start_error
            .to_string()
            .contains("adapter `mock-adapter` is not configured")
    );
    let adapter_stop: zbus::Result<()> = proxy
        .call(dbus::method::STOP_ADAPTER, &"mock-adapter")
        .await;
    let adapter_stop_error = adapter_stop.expect_err("unconfigured adapter stop should fail");
    assert!(
        adapter_stop_error
            .to_string()
            .contains("adapter `mock-adapter` is not configured")
    );
    let empty_adapter_start: zbus::Result<()> = proxy.call(dbus::method::START_ADAPTER, &"").await;
    let empty_adapter_start_error =
        empty_adapter_start.expect_err("empty adapter start should fail");
    assert!(
        empty_adapter_start_error
            .to_string()
            .contains("adapter `` is not configured")
    );
    let empty_adapter_stop: zbus::Result<()> = proxy.call(dbus::method::STOP_ADAPTER, &"").await;
    let empty_adapter_stop_error = empty_adapter_stop.expect_err("empty adapter stop should fail");
    assert!(
        empty_adapter_stop_error
            .to_string()
            .contains("adapter `` is not configured")
    );

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

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
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text.trim(), "8");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    Ok(())
}

fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "vinput-dbus-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ))
}

#[tokio::test]
async fn configured_adapter_supervision_roundtrips_through_session_bus() -> anyhow::Result<()> {
    let runtime_dir = unique_adapter_runtime_dir("adapter-supervision");
    let pid_path = runtime_dir.join("cmd-adapter.pid");
    let config: VinputConfig = serde_json::from_str(
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
    let duplicate_start: zbus::Result<()> = proxy
        .call(dbus::method::START_ADAPTER, &"cmd-adapter")
        .await;
    let duplicate_error = duplicate_start.expect_err("duplicate adapter start should fail");
    assert!(
        duplicate_error.to_string().contains("already running"),
        "unexpected duplicate adapter start error: {duplicate_error}"
    );

    proxy
        .call::<_, _, ()>(dbus::method::STOP_ADAPTER, &"cmd-adapter")
        .await?;
    assert!(!pid_path.exists(), "adapter stop should remove pid file");
    proxy
        .call::<_, _, ()>(dbus::method::STOP_ADAPTER, &"cmd-adapter")
        .await?;
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
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text, "bus adapter final");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    Ok(())
}
