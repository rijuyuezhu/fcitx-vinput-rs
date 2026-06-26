//! Integration tests for protocol inspection CLI output.

use std::process::Command;

#[test]
fn protocol_prints_legacy_dbus_contract() {
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
    assert!(
        value["methods"]
            .as_array()
            .expect("methods should be an array")
            .contains(&serde_json::Value::String("StartRecording".to_owned()))
    );
    assert!(
        value["signals"]
            .as_array()
            .expect("signals should be an array")
            .contains(&serde_json::Value::String("RecognitionResult".to_owned()))
    );
}
