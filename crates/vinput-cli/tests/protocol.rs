//! Integration tests for protocol inspection CLI output.

use std::process::Command;

use vinput_protocol::dbus;

#[test]
fn protocol_prints_service_dbus_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["protocol"])
        .output()
        .expect("run vinput protocol");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("protocol output should be JSON");
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
