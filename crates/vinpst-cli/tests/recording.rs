//! Integration tests for recording control CLI dry-run paths.

mod common;

use common::{assert_json_success, assert_stdout_success, vinpst_command};

fn assert_daemon_owner_probe_plan(value: &serde_json::Value) {
    assert_eq!(value["owner_probe"]["target_name"], "org.fcitx.Vinpst");
    let owner_methods = value["owner_probe"]["methods"]
        .as_array()
        .expect("owner probe methods");
    assert!(owner_methods.contains(&serde_json::json!("GetNameOwner")));
    assert!(owner_methods.contains(&serde_json::json!("GetConnectionUnixProcessID")));
    let process_fields = value["owner_probe"]["process_fields"]
        .as_array()
        .expect("owner probe process fields");
    for field in ["unix_process_id", "exe", "cmdline"] {
        assert!(
            process_fields.contains(&serde_json::json!(field)),
            "missing owner probe process field {field}"
        );
    }
}

#[test]
fn recording_status_dry_run_json_reports_get_status_plan() {
    let output = vinpst_command()
        .args(["recording", "status", "--dry-run", "--json"])
        .output()
        .expect("run vinpst recording status dry-run json");

    let value = assert_json_success(output, "recording status dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "status");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["called"], false);
    assert_eq!(value["dbus"]["method"], "GetStatus");
    assert_daemon_owner_probe_plan(&value);
}

#[test]
fn recording_status_text_hides_transport_details() {
    let output = vinpst_command()
        .args(["recording", "status", "--dry-run"])
        .output()
        .expect("run vinpst recording status dry-run text");

    let stdout = assert_stdout_success(output, "recording status dry-run text");
    assert!(!stdout.trim().is_empty());
    for internal in [
        "org.fcitx.Vinpst",
        "GetStatus",
        "owner_probe",
        "will_call_dbus",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal detail: {internal}"
        );
    }
}

#[test]
fn global_json_flag_forces_recording_status_json() {
    let output = vinpst_command()
        .args(["-j", "recording", "status", "--dry-run"])
        .output()
        .expect("run vinpst -j recording status --dry-run");

    let value = assert_json_success(output, "global json recording status");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "status");
    assert_eq!(value["dbus"]["method"], "GetStatus");
    assert_daemon_owner_probe_plan(&value);
}

#[test]
fn global_json_flag_is_accepted_after_subcommands() {
    let output = vinpst_command()
        .args(["recording", "status", "--dry-run", "-j"])
        .output()
        .expect("run vinpst recording status --dry-run -j");

    let value = assert_json_success(output, "post-subcommand global json recording status");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["dbus"]["method"], "GetStatus");
}
