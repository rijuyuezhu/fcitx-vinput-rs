//! Integration tests for the legacy D-Bus ABI exposed by `vinput-daemon`.
#![cfg(feature = "dbus-integration")]

use vinput_config::VinputConfig;
use vinput_daemon::{RuntimeState, VinputDbusService};
use vinput_protocol::{RecognitionPayload, dbus};
use zbus::Proxy;

async fn spawn_service() -> anyhow::Result<zbus::Connection> {
    let config = VinputConfig::bundled_default()?;
    let runtime = RuntimeState::new(config)?;
    let connection = VinputDbusService::new(runtime)
        .serve_on_session_bus()
        .await?;
    Ok(connection)
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

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    let status: String = proxy.call(dbus::method::START_RECORDING, &()).await?;
    assert_eq!(status, "recording");

    let payload_json: String = proxy.call(dbus::method::STOP_RECORDING, &"").await?;
    let payload = RecognitionPayload::from_json_str(&payload_json)?;
    assert_eq!(payload.commit_text, "mock recognition result");

    let status: String = proxy.call(dbus::method::GET_STATUS, &()).await?;
    assert_eq!(status, "idle");

    Ok(())
}
