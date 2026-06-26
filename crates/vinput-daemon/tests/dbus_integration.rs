//! Integration tests for the legacy D-Bus ABI exposed by `vinput-daemon`.
#![cfg(feature = "dbus-integration")]

use std::time::Duration;

use futures_util::StreamExt;
use tokio::time::timeout;
use vinput_config::VinputConfig;
use vinput_daemon::{RuntimeState, VinputDbusService};
use vinput_protocol::{RecognitionPayload, dbus};
use zbus::{Message, Proxy};

async fn spawn_service() -> anyhow::Result<zbus::Connection> {
    let config = VinputConfig::bundled_default()?;
    let runtime = RuntimeState::new(config)?;
    let connection = VinputDbusService::new(runtime)
        .serve_on_session_bus()
        .await?;
    Ok(connection)
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

    let mut status_signals = proxy.receive_signal(dbus::signal::STATUS_CHANGED).await?;
    let mut partial_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_PARTIAL)
        .await?;
    let mut result_signals = proxy
        .receive_signal(dbus::signal::RECOGNITION_RESULT)
        .await?;

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    let status: String = proxy.call(dbus::method::START_RECORDING, &()).await?;
    assert_eq!(status, "recording");
    assert_eq!(next_string_signal(&mut status_signals).await?, "recording");
    assert_eq!(
        next_string_signal(&mut partial_signals).await?,
        "mock partial"
    );

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "mock recognition result");
    assert_eq!(next_string_signal(&mut status_signals).await?, "inferring");
    let result_payload_json = next_string_signal(&mut result_signals).await?;
    let signal_payload = RecognitionPayload::from_json_str(&result_payload_json)?;
    assert_eq!(signal_payload.commit_text, "mock recognition result");
    assert_eq!(next_string_signal(&mut status_signals).await?, "idle");

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    Ok(())
}
