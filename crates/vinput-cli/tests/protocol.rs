//! Integration tests for protocol inspection CLI output.

mod common;

use common::{assert_json_success, vinput_command};
use vinput_protocol::{RecognitionPayload, dbus};

const RAW_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/raw.json"
));

fn fixture_json(input: &str) -> &str {
    input.trim_end()
}

#[test]
fn shared_recognition_fixture_roundtrips_through_protocol_crate() {
    let payload = RecognitionPayload::from_json_str(fixture_json(RAW_PAYLOAD_JSON)).unwrap();

    assert_eq!(
        payload.to_json_string().unwrap(),
        fixture_json(RAW_PAYLOAD_JSON)
    );
}

#[test]
fn protocol_prints_service_dbus_contract() {
    let output = vinput_command()
        .args(["protocol"])
        .output()
        .expect("run vinput protocol");

    let value = assert_json_success(output, "protocol output");
    assert_eq!(value["service_bus_name"], "org.fcitx.Vinput");
    assert_eq!(value["service_object_path"], "/org/fcitx/Vinput");
    assert_eq!(value["service_interface"], "org.fcitx.Vinput.Service");
    assert_eq!(
        value["methods"],
        serde_json::to_value(dbus::SERVICE_METHODS).unwrap()
    );
    assert_eq!(
        value["signals"],
        serde_json::to_value(dbus::SERVICE_SIGNALS).unwrap()
    );
}
