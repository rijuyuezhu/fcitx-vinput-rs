//! Regression tests for the retained C++ Fcitx5 bridge D-Bus contract.

mod common;

use common::workspace_file;
use vinput_protocol::{ServiceStatus, dbus};

fn cpp_constant(header: &str, name: &str) -> String {
    let needle = format!("{name} =");
    let start = header
        .find(&needle)
        .unwrap_or_else(|| panic!("C++ bridge contract should define {name}"));
    let suffix = &header[start + needle.len()..];
    let first_quote = suffix
        .find('"')
        .unwrap_or_else(|| panic!("C++ bridge contract constant {name} should be a string"));
    let value = &suffix[first_quote + 1..];
    let second_quote = value
        .find('"')
        .unwrap_or_else(|| panic!("C++ bridge contract constant {name} should terminate"));
    value[..second_quote].to_owned()
}

#[test]
fn cpp_bridge_dbus_contract_matches_rust_protocol() {
    let header = std::fs::read_to_string(workspace_file(
        "cpp/fcitx5-addon/include/vinput_fcitx_bridge/dbus_contract.h",
    ))
    .expect("read C++ bridge D-Bus contract header");

    for (name, expected) in [
        ("kFcitxBusName", dbus::FCITX_BUS_NAME),
        ("kServiceBusName", dbus::SERVICE_BUS_NAME),
        ("kServiceObjectPath", dbus::SERVICE_OBJECT_PATH),
        ("kServiceInterface", dbus::SERVICE_INTERFACE),
        (
            "kFrontendNotifierObjectPath",
            dbus::FRONTEND_NOTIFIER_OBJECT_PATH,
        ),
        (
            "kFrontendNotifierInterface",
            dbus::FRONTEND_NOTIFIER_INTERFACE,
        ),
        ("kMethodStartRecording", dbus::method::START_RECORDING),
        (
            "kMethodStartCommandRecording",
            dbus::method::START_COMMAND_RECORDING,
        ),
        ("kMethodStopRecording", dbus::method::STOP_RECORDING),
        ("kMethodGetStatus", dbus::method::GET_STATUS),
        (
            "kMethodGetAsrBackendState",
            dbus::method::GET_ASR_BACKEND_STATE,
        ),
        ("kMethodReloadAsrBackend", dbus::method::RELOAD_ASR_BACKEND),
        ("kMethodStartAdapter", dbus::method::START_ADAPTER),
        ("kMethodStopAdapter", dbus::method::STOP_ADAPTER),
        ("kMethodNotify", dbus::method::NOTIFY),
        ("kSignalRecognitionResult", dbus::signal::RECOGNITION_RESULT),
        (
            "kSignalRecognitionPartial",
            dbus::signal::RECOGNITION_PARTIAL,
        ),
        ("kSignalStatusChanged", dbus::signal::STATUS_CHANGED),
        (
            "kSignalDaemonNotification",
            dbus::signal::DAEMON_NOTIFICATION,
        ),
        ("kErrorOperationFailed", dbus::error::OPERATION_FAILED),
        ("kStatusIdle", ServiceStatus::Idle.as_wire_str()),
        ("kStatusRecording", ServiceStatus::Recording.as_wire_str()),
        ("kStatusInferring", ServiceStatus::Inferring.as_wire_str()),
        (
            "kStatusPostprocessing",
            ServiceStatus::Postprocessing.as_wire_str(),
        ),
        ("kStatusError", ServiceStatus::Error.as_wire_str()),
    ] {
        assert_eq!(cpp_constant(&header, name), expected, "{name} should match");
    }
}
