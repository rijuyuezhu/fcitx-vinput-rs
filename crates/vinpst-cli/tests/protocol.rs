//! Integration tests for protocol inspection CLI output.

mod common;

use std::fs;

use common::{assert_json_success, assert_stdout_success, vinpst_command};
use tempfile::NamedTempFile;
use vinpst_protocol::{RecognitionPayload, ServiceStatus, dbus};

const RAW_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/raw.json"
));
const MENU_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/menu.json"
));
const SENTINEL_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/sentinel.json"
));

fn fixture_json(input: &str) -> &str {
    input.trim_end()
}
fn flatpak_info_fixture() -> NamedTempFile {
    let file = NamedTempFile::new().expect("create flatpak info fixture");
    fs::write(
        file.path(),
        "[Context]\nshared=network;ipc;\nsockets=wayland;pipewire;\nfilesystems=xdg-config/systemd;xdg-cache;\n",
    )
    .expect("write flatpak info fixture");
    file
}

fn assert_daemon_owner_probe_plan(value: &serde_json::Value) {
    assert_eq!(value["owner_probe"]["target_name"], dbus::SERVICE_BUS_NAME);
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

fn assert_human_output_hides_transport_details(stdout: &str) {
    assert!(!stdout.trim().is_empty());
    for internal in [
        "org.fcitx.Vinpst",
        "/org/fcitx/Vinpst",
        "GetNameOwner",
        "owner_probe:",
        "will_call_dbus:",
        "tool_program:",
        "tool_env_override:",
        "dry_run:",
        "called:",
        "method:",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal detail: {internal}"
        );
    }
}

#[test]
fn shared_recognition_fixtures_roundtrip_through_protocol_crate() {
    for fixture in [RAW_PAYLOAD_JSON, MENU_PAYLOAD_JSON, SENTINEL_PAYLOAD_JSON] {
        let fixture = fixture_json(fixture);
        let payload = RecognitionPayload::from_json_str(fixture).unwrap();

        assert_eq!(payload.to_json_string().unwrap(), fixture);
    }
}

#[test]
fn protocol_prints_service_dbus_contract() {
    let output = vinpst_command()
        .args(["protocol"])
        .output()
        .expect("run vinpst protocol");

    let value = assert_json_success(output, "protocol output");
    assert_eq!(value["service_bus_name"], "org.fcitx.Vinpst");
    assert_eq!(value["service_object_path"], "/org/fcitx/Vinpst");
    assert_eq!(value["service_interface"], "org.fcitx.Vinpst.Service");
    assert_eq!(value["frontend_notifier_method"], "Notify");
    assert_eq!(
        value["operation_failed_error"],
        "org.fcitx.Vinpst.Error.OperationFailed"
    );
    assert_eq!(value["error_info_signature"], "ssss");
    assert_eq!(
        value["methods"],
        serde_json::to_value(dbus::SERVICE_METHODS).unwrap()
    );
    assert_eq!(
        value["legacy_methods"],
        serde_json::to_value(dbus::LEGACY_SERVICE_METHODS).unwrap()
    );
    assert_eq!(
        value["diagnostic_extension_methods"],
        serde_json::to_value(dbus::DIAGNOSTIC_EXTENSION_METHODS).unwrap()
    );
    assert!(
        !value["legacy_methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetTextAdapterState"))
    );
    assert!(
        !value["legacy_methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetRuntimeStatus"))
    );
    assert!(
        value["diagnostic_extension_methods"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("GetRuntimeStatus"))
    );
    assert_eq!(
        value["signals"],
        serde_json::to_value(dbus::SERVICE_SIGNALS).unwrap()
    );
    assert_eq!(
        value["statuses"],
        serde_json::to_value(ServiceStatus::WIRE_VALUES).unwrap()
    );
}

#[test]
fn activation_service_prints_configured_exec_line() {
    let output = vinpst_command()
        .args([
            "activation-service",
            "--daemon",
            "/opt/vinpst daemon/bin/vinpst-daemon",
            "--configured-backends",
            "--config",
            "/tmp/vinpst config.json",
            "--audio-backend",
            "pipewire",
            "--daemon-arg=--log-level",
            "--daemon-arg=debug",
        ])
        .output()
        .expect("run vinpst activation-service");

    let stdout = assert_stdout_success(output, "activation service output");
    assert_eq!(
        stdout,
        "[D-BUS Service]\nName=org.fcitx.Vinpst\nExec='/opt/vinpst daemon/bin/vinpst-daemon' --dbus --configured-backends --config '/tmp/vinpst config.json' --audio-backend pipewire --log-level debug --exit-when-executable-replaced\n"
    );
}

#[test]
fn activation_service_user_writes_xdg_data_home_service() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinpst-cli-user-service-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .args([
            "activation-service",
            "--daemon",
            "/usr/bin/vinpst-daemon",
            "--user",
        ])
        .output()
        .expect("run vinpst activation-service --user");

    let stdout = assert_stdout_success(output, "activation service user output");
    assert!(stdout.is_empty());
    let service_path = data_home
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinpst.service");
    let service = std::fs::read_to_string(&service_path).expect("read generated user service");
    assert_eq!(
        service,
        "[D-BUS Service]\nName=org.fcitx.Vinpst\nExec=/usr/bin/vinpst-daemon --dbus --exit-when-executable-replaced\n"
    );
    std::fs::remove_dir_all(data_home).expect("remove generated user service fixture");
}

#[test]
fn activation_service_remove_user_deletes_xdg_data_home_service() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinpst-cli-remove-user-service-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));

    let service_path = data_home
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinpst.service");
    std::fs::create_dir_all(service_path.parent().unwrap()).expect("create service dir");
    std::fs::write(&service_path, "stale service").expect("write stale service");

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .args(["activation-service", "--remove-user"])
        .output()
        .expect("run vinpst activation-service --remove-user");

    let value = assert_json_success(output, "remove user activation service output");
    assert_eq!(value["ok"], true);
    assert_eq!(value["removed"], true);
    assert_eq!(
        value["user_service_path"],
        service_path.to_string_lossy().as_ref()
    );
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon start --dry-run"))
    }));
    assert!(!service_path.exists());
    std::fs::remove_dir_all(data_home).expect("remove service fixture");
}

#[test]
fn activation_service_user_status_reports_existing_service() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinpst-cli-user-status-service-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let service_path = data_home
        .join("dbus-1")
        .join("services")
        .join("org.fcitx.Vinpst.service");
    std::fs::create_dir_all(service_path.parent().unwrap()).expect("create service dir");
    std::fs::write(
        &service_path,
        "[D-BUS Service]\nName=org.fcitx.Vinpst\nExec=/usr/bin/vinpst-daemon --dbus --exit-when-executable-replaced\n",
    )
    .expect("write service file");

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .args(["activation-service", "--user-status"])
        .output()
        .expect("run vinpst activation-service --user-status");

    let value = assert_json_success(output, "user activation status output");
    assert_eq!(value["user_service_exists"], true);
    assert_eq!(value["user_service_name"], "org.fcitx.Vinpst");
    assert_eq!(value["user_service_name_matches"], true);
    assert_eq!(
        value["user_service_exec"],
        "/usr/bin/vinpst-daemon --dbus --exit-when-executable-replaced"
    );
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon owner/procfs probes"))
    }));
    std::fs::remove_dir_all(data_home).expect("remove service fixture");
}

#[test]
fn activation_service_user_status_reports_missing_service_next_steps() {
    let mut data_home = std::env::temp_dir();
    data_home.push(format!(
        "vinpst-cli-missing-user-status-service-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .args(["activation-service", "--user-status"])
        .output()
        .expect("run vinpst activation-service --user-status without service");

    let value = assert_json_success(output, "missing user activation status output");
    assert_eq!(value["user_service_exists"], false);
    assert_eq!(value["user_service_name_matches"], false);
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon start --dry-run"))
    }));
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon owner/procfs probes"))
    }));
    if data_home.exists() {
        std::fs::remove_dir_all(data_home).expect("remove missing service fixture");
    }
}

#[test]
fn daemon_reload_asr_dry_run_prints_dbus_plan_json() {
    let output = vinpst_command()
        .args(["daemon", "reload-asr", "--dry-run", "--json"])
        .output()
        .expect("run vinpst daemon reload-asr --dry-run --json");

    let value = assert_json_success(output, "daemon reload-asr dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_call_dbus"], false);
    assert!(value["asr_backend"].is_null());
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(value["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(value["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    assert_eq!(value["dbus"]["method"], dbus::method::RELOAD_ASR_BACKEND);
    assert_daemon_owner_probe_plan(&value);
}

#[test]
fn daemon_dry_run_text_hides_transport_details() {
    for args in [
        vec!["daemon", "reload-asr", "--dry-run"],
        vec!["daemon", "status", "--dry-run"],
        vec!["daemon", "start", "--dry-run"],
        vec!["daemon", "stop", "--dry-run"],
        vec!["daemon", "log", "--lines", "7", "--dry-run"],
    ] {
        let output = vinpst_command()
            .args(args)
            .output()
            .expect("run daemon human-output dry-run");
        let stdout = assert_stdout_success(output, "daemon human-output dry-run");
        assert_human_output_hides_transport_details(&stdout);
    }
}

#[test]
fn daemon_status_dry_run_prints_dbus_plan_json() {
    let output = vinpst_command()
        .args(["daemon", "status", "--dry-run", "--json"])
        .output()
        .expect("run vinpst daemon status --dry-run --json");

    let value = assert_json_success(output, "daemon status dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(value["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(value["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    let methods = value["dbus"]["methods"].as_array().unwrap();
    assert!(methods.contains(&serde_json::json!(dbus::method::GET_STATUS)));
    assert!(methods.contains(&serde_json::json!(dbus::method::GET_ASR_BACKEND_STATE)));
    assert!(methods.contains(&serde_json::json!(dbus::method::GET_RUNTIME_STATUS)));
    let reports = value["reports"].as_array().unwrap();
    assert!(reports.contains(&serde_json::json!("service_status")));
    assert!(reports.contains(&serde_json::json!("bus_owner")));
    assert!(reports.contains(&serde_json::json!("daemon_handoff")));
    assert!(reports.contains(&serde_json::json!("asr_backend")));
    assert!(reports.contains(&serde_json::json!("runtime_status")));
    assert!(reports.contains(&serde_json::json!("text_adapters")));
    assert_daemon_owner_probe_plan(&value);
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("without --dry-run"))
    }));
}

#[test]
fn recording_start_dry_run_prints_dbus_plan_json() {
    let output = vinpst_command()
        .args([
            "recording",
            "start",
            "--selected-text",
            "hello",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst recording start --dry-run --json");

    let value = assert_json_success(output, "recording start dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "start");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(
        value["dbus"]["methods"][0],
        dbus::method::START_COMMAND_RECORDING
    );
    assert_eq!(value["args"]["selected_text_present"], true);
    assert_daemon_owner_probe_plan(&value);
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon owner/procfs probes"))
    }));
}

#[test]
fn recording_dry_run_text_hides_transport_details() {
    for args in [
        vec!["recording", "stop", "--scene", "demo", "--dry-run"],
        vec!["recording", "toggle", "--dry-run"],
    ] {
        let output = vinpst_command()
            .args(args)
            .output()
            .expect("run recording human-output dry-run");
        let stdout = assert_stdout_success(output, "recording human-output dry-run");
        assert_human_output_hides_transport_details(&stdout);
    }
}

#[test]
fn daemon_start_dry_run_prints_activation_plan_json() {
    let output = vinpst_command()
        .args(["daemon", "start", "--dry-run", "--json"])
        .output()
        .expect("run vinpst daemon start --dry-run --json");

    let value = assert_json_success(output, "daemon start dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "start");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["activation"]["strategy"], "dbus-service-activation");
    assert_eq!(
        value["activation"]["trigger_method"],
        dbus::method::GET_STATUS
    );
    assert_eq!(value["dbus"]["service"], dbus::SERVICE_BUS_NAME);
    assert_eq!(value["dbus"]["object_path"], dbus::SERVICE_OBJECT_PATH);
    assert_eq!(value["dbus"]["interface"], dbus::SERVICE_INTERFACE);
    assert_eq!(value["dbus"]["method"], dbus::method::GET_STATUS);
    assert_daemon_owner_probe_plan(&value);
    assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
        step.as_str()
            .is_some_and(|step| step.contains("daemon status"))
    }));
}

#[test]
fn daemon_user_service_dry_run_commands_print_plans_json() {
    for (command, expected, tool, env_override) in [
        (
            "stop",
            "systemctl --user stop vinpst-daemon.service",
            "systemctl",
            "VINPST_DAEMON_SYSTEMCTL",
        ),
        (
            "restart",
            "systemctl --user restart vinpst-daemon.service",
            "systemctl",
            "VINPST_DAEMON_SYSTEMCTL",
        ),
        (
            "log",
            "journalctl --user -u vinpst-daemon.service",
            "journalctl",
            "VINPST_DAEMON_JOURNALCTL",
        ),
    ] {
        let output = vinpst_command()
            .args(["daemon", command, "--dry-run", "--json"])
            .output()
            .expect("run vinpst daemon user-service command --dry-run --json");

        let value = assert_json_success(output, "daemon user service dry-run json");
        assert_eq!(value["ok"], true);
        assert_eq!(value["dry_run"], true);
        assert_eq!(value["action"], command);
        assert_eq!(value["will_mutate_user_service"], false);
        assert_eq!(value["strategy"], "systemd-user-service");
        assert_eq!(value["tool"]["name"], tool);
        assert_eq!(value["tool"]["program"], tool);
        assert_eq!(value["tool"]["env_override"], env_override);
        assert_eq!(value["tool"]["overridden"], false);
        assert!(
            value["fallback_steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| {
                    step.as_str()
                        .is_some_and(|step| step.contains("activation-service --user-status"))
                })
        );
        assert_eq!(value["command"], expected);
        assert_daemon_owner_probe_plan(&value);
        assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
            step.as_str()
                .is_some_and(|step| step.contains("vinpst daemon"))
        }));
    }
}

#[test]
fn doctor_reports_missing_flatpak_permissions() {
    let flatpak_info = NamedTempFile::new().expect("create partial flatpak info");
    fs::write(
        flatpak_info.path(),
        "[Context]\nsockets=wayland;\nfilesystems=xdg-cache;\n",
    )
    .expect("write partial flatpak info");
    let output = vinpst_command()
        .env("VINPST_FLATPAK_INFO_PATH", flatpak_info.path())
        .args(["doctor"])
        .output()
        .expect("run sandboxed vinpst doctor");

    let value = assert_json_success(output, "sandboxed doctor");
    assert_eq!(value["sandbox"]["detected"], true);
    assert_eq!(
        value["sandbox"]["missing_permissions"],
        serde_json::json!(["socket:pipewire", "filesystem:xdg-config/systemd"])
    );
}

#[test]
fn daemon_install_service_flatpak_dry_run_rewrites_template() {
    let flatpak_info = flatpak_info_fixture();
    let root = tempfile::tempdir().expect("create install-service temp dir");
    let template = root.path().join("vinpst-daemon.service");
    let output_path = root.path().join("user/vinpst-daemon.service");
    fs::write(
        &template,
        "[Service]\nExecStart=/usr/bin/vinpst-daemon --dbus --configured-backends\n",
    )
    .expect("write service template");
    let output = vinpst_command()
        .env("VINPST_FLATPAK_INFO_PATH", flatpak_info.path())
        .args([
            "daemon",
            "install-service",
            "--template",
            template.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run sandboxed install-service dry-run");

    let value = assert_json_success(output, "sandboxed install-service dry-run");
    assert_eq!(value["rewritten_for_flatpak"], true);
    assert_eq!(value["wrote_service"], false);
    assert!(!output_path.exists());
    let rendered = value["rendered_service"].as_str().unwrap();
    assert!(rendered.contains(
        "ExecStart=flatpak run --command=/app/addons/Vinpst/bin/vinpst-daemon org.fcitx.Fcitx5 --dbus --configured-backends"
    ));
    assert!(rendered.contains("ExecStop=pkill -INT vinpst-daemon"));
}

#[test]
fn daemon_install_service_writes_and_reloads_user_systemd() {
    let root = tempfile::tempdir().expect("create install-service temp dir");
    let template = root.path().join("vinpst-daemon.service");
    let output_path = root.path().join("user/vinpst-daemon.service");
    let template_contents =
        "[Service]\nExecStart=/usr/bin/vinpst-daemon --dbus --configured-backends\n";
    fs::write(&template, template_contents).expect("write service template");

    let output = vinpst_command()
        .env(
            "VINPST_FLATPAK_INFO_PATH",
            root.path().join("not-a-flatpak-info"),
        )
        .env("VINPST_DAEMON_SYSTEMCTL", "/bin/echo")
        .args([
            "daemon",
            "install-service",
            "--template",
            template.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("install native user service");

    let value = assert_json_success(output, "native install-service");
    assert_eq!(value["rewritten_for_flatpak"], false);
    assert_eq!(value["wrote_service"], true);
    assert_eq!(value["rendered_service"], serde_json::Value::Null);
    assert_eq!(value["daemon_reload"]["ok"], true);
    assert_eq!(value["daemon_reload"]["stdout"], "--user daemon-reload\n");
    assert_eq!(
        fs::read_to_string(&output_path).expect("read installed service"),
        template_contents
    );
}

#[test]
fn daemon_user_service_flatpak_dry_run_wraps_host_commands() {
    let flatpak_info = flatpak_info_fixture();
    let output = vinpst_command()
        .env("VINPST_FLATPAK_INFO_PATH", flatpak_info.path())
        .args(["daemon", "restart", "--dry-run", "--json"])
        .output()
        .expect("run sandboxed vinpst daemon restart --dry-run --json");

    let value = assert_json_success(output, "sandboxed daemon restart dry-run json");
    assert_eq!(value["sandbox"]["kind"], "flatpak");
    assert_eq!(value["sandbox"]["host_command"], true);
    assert_eq!(value["tool"]["name"], "systemctl");
    assert_eq!(value["tool"]["program"], "systemctl");
    assert_eq!(value["host_wrapper"]["program"], "flatpak-spawn");
    assert_eq!(
        value["host_wrapper"]["env_override"],
        "VINPST_FLATPAK_SPAWN"
    );
    assert_eq!(value["host_wrapper"]["overridden"], false);
    assert_eq!(
        value["command_argv"],
        serde_json::json!([
            "flatpak-spawn",
            "--host",
            "systemctl",
            "--user",
            "restart",
            "vinpst-daemon.service"
        ])
    );
}

#[test]
fn daemon_log_flatpak_dry_run_uses_host_flatpak_journal_filter() {
    let flatpak_info = flatpak_info_fixture();
    let output = vinpst_command()
        .env("VINPST_FLATPAK_INFO_PATH", flatpak_info.path())
        .args(["daemon", "log", "--lines", "42", "--dry-run", "--json"])
        .output()
        .expect("run sandboxed vinpst daemon log --dry-run --json");

    let value = assert_json_success(output, "sandboxed daemon log dry-run json");
    assert_eq!(value["sandbox"]["kind"], "flatpak");
    assert_eq!(value["tool"]["name"], "journalctl");
    assert_eq!(
        value["command_argv"],
        serde_json::json!([
            "flatpak-spawn",
            "--host",
            "journalctl",
            "--user",
            "-t",
            "flatpak",
            "--grep",
            "vinpst",
            "-n",
            "42"
        ])
    );
}

#[test]
fn daemon_user_service_flatpak_real_command_preserves_tool_overrides() {
    let flatpak_info = flatpak_info_fixture();
    let output = vinpst_command()
        .env("VINPST_FLATPAK_INFO_PATH", flatpak_info.path())
        .env("VINPST_FLATPAK_SPAWN", "/bin/echo")
        .env("VINPST_DAEMON_SYSTEMCTL", "/custom/systemctl")
        .args(["daemon", "stop", "--json"])
        .output()
        .expect("run sandboxed vinpst daemon stop --json");

    let value = assert_json_success(output, "sandboxed daemon stop real json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["tool"]["program"], "/custom/systemctl");
    assert_eq!(value["tool"]["overridden"], true);
    assert_eq!(value["host_wrapper"]["program"], "/bin/echo");
    assert_eq!(value["host_wrapper"]["overridden"], true);
    assert_eq!(
        value["stdout"],
        "--host /custom/systemctl --user stop vinpst-daemon.service\n"
    );
}

#[test]
fn daemon_handoff_dry_run_pins_conditional_restart_and_verification() {
    let output = vinpst_command()
        .args(["daemon", "handoff", "--dry-run", "--json"])
        .output()
        .expect("run vinpst daemon handoff --dry-run --json");

    let value = assert_json_success(output, "daemon handoff dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "handoff");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["will_mutate_user_service"], false);
    assert_eq!(
        value["strategy"],
        "systemd-reload-or-guarded-direct-owner-termination"
    );
    assert_eq!(
        value["restart_condition"],
        serde_json::json!(["owner-executable-deleted", "owner-executable-path-mismatch"])
    );
    assert_eq!(
        value["service_control"]["command"],
        "systemctl --user restart vinpst-daemon.service"
    );
    assert_eq!(
        value["systemd_control"]["owner_probe"]["command"],
        "systemctl --user show --property MainPID --value vinpst-daemon.service"
    );
    assert_eq!(
        value["systemd_control"]["reload"]["command"],
        "systemctl --user daemon-reload"
    );
    assert_eq!(value["direct_control"]["signal"], "TERM");
    assert_eq!(
        value["direct_control"]["reactivation"],
        "reload D-Bus activation config and verify the new owner"
    );
    assert_eq!(value["service_control"]["will_mutate_user_service"], false);
    assert_eq!(value["verification"]["required_after_restart"], true);
    assert_eq!(value["verification"]["requires_current_owner"], true);
    assert!(value["verification"]["attempts"].as_u64().unwrap() > 1);
}
#[test]
fn daemon_handoff_flatpak_dry_run_wraps_all_systemd_commands() {
    let flatpak_info = flatpak_info_fixture();
    let output = vinpst_command()
        .env("VINPST_FLATPAK_INFO_PATH", flatpak_info.path())
        .args(["daemon", "handoff", "--dry-run", "--json"])
        .output()
        .expect("run sandboxed vinpst daemon handoff --dry-run --json");

    let value = assert_json_success(output, "sandboxed daemon handoff dry-run json");
    for control in [
        &value["service_control"],
        &value["systemd_control"]["owner_probe"],
        &value["systemd_control"]["reload"],
        &value["systemd_control"]["restart"],
    ] {
        assert_eq!(control["sandbox"]["kind"], "flatpak");
        assert_eq!(control["command_argv"][0], "flatpak-spawn");
        assert_eq!(control["command_argv"][1], "--host");
        assert_eq!(control["command_argv"][2], "systemctl");
    }
    assert_eq!(value["direct_control"]["program"], "kill");
    assert_eq!(value["direct_control"]["sandbox"]["kind"], "flatpak");
    assert_eq!(
        value["direct_control"]["command_argv"],
        serde_json::json!(["flatpak-spawn", "--host", "kill", "-TERM", "<owner-pid>"])
    );
}

#[test]
fn daemon_handoff_text_dry_run_is_explicitly_non_mutating() {
    let output = vinpst_command()
        .args(["daemon", "handoff", "--dry-run"])
        .output()
        .expect("run vinpst daemon handoff --dry-run");

    let stdout = assert_stdout_success(output, "daemon handoff dry-run text");
    assert!(stdout.contains("action: handoff"));
    assert!(stdout.contains("will_mutate_user_service: false"));
    assert!(stdout.contains("strategy: systemd-reload-or-guarded-direct-owner-termination"));
    assert!(stdout.contains("systemctl --user restart vinpst-daemon.service"));
    assert!(stdout.contains("without --dry-run to inspect and conditionally restart"));
}

#[test]
fn global_json_flag_forces_daemon_handoff_json() {
    let output = vinpst_command()
        .args(["-j", "daemon", "handoff", "--dry-run"])
        .output()
        .expect("run vinpst -j daemon handoff --dry-run");

    let value = assert_json_success(output, "global JSON daemon handoff dry-run");
    assert_eq!(value["action"], "handoff");
    assert_eq!(value["dry_run"], true);
}

#[test]
fn daemon_log_lines_dry_run_adds_journalctl_limit() {
    let output = vinpst_command()
        .args(["daemon", "log", "--lines", "42", "--dry-run", "--json"])
        .output()
        .expect("run vinpst daemon log --lines --dry-run --json");

    let value = assert_json_success(output, "daemon log lines dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "log");
    assert_eq!(
        value["command"],
        "journalctl --user -u vinpst-daemon.service -n 42"
    );
    assert_eq!(
        value["command_argv"],
        serde_json::json!([
            "journalctl",
            "--user",
            "-u",
            "vinpst-daemon.service",
            "-n",
            "42"
        ])
    );
}

#[test]
fn daemon_log_lines_real_command_reports_limited_argv() {
    let output = vinpst_command()
        .env("VINPST_DAEMON_JOURNALCTL", "/bin/echo")
        .args(["daemon", "log", "--lines", "3", "--json"])
        .output()
        .expect("run vinpst daemon log --lines --json");

    let value = assert_json_success(output, "daemon log lines real json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["action"], "log");
    assert_eq!(value["will_mutate_user_service"], false);
    assert_eq!(value["command_argv"][0], "/bin/echo");
    assert_eq!(value["stdout"], "--user -u vinpst-daemon.service -n 3\n");
}

#[test]
fn global_json_flag_forces_daemon_log_lines_json() {
    let output = vinpst_command()
        .args(["-j", "daemon", "log", "--lines", "9", "--dry-run"])
        .output()
        .expect("run vinpst -j daemon log --lines --dry-run");

    let value = assert_json_success(output, "global json daemon log lines dry-run");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "log");
    assert_eq!(
        value["command"],
        "journalctl --user -u vinpst-daemon.service -n 9"
    );
}

#[test]
fn daemon_log_lines_rejects_zero() {
    let output = vinpst_command()
        .args(["daemon", "log", "--lines", "0", "--dry-run"])
        .output()
        .expect("run vinpst daemon log --lines 0");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("daemon log --lines must be greater than 0"));
}

#[test]
fn daemon_user_service_missing_tool_reports_fallback_steps_json() {
    let output = vinpst_command()
        .env(
            "VINPST_DAEMON_SYSTEMCTL",
            "/definitely/missing/vinpst-systemctl",
        )
        .args(["daemon", "stop", "--json"])
        .output()
        .expect("run vinpst daemon stop with missing systemctl");

    let value = assert_json_success(output, "daemon missing systemctl json");
    assert_eq!(value["ok"], false);
    assert_eq!(value["tool"]["name"], "systemctl");
    assert_eq!(
        value["tool"]["program"],
        "/definitely/missing/vinpst-systemctl"
    );
    assert_eq!(value["tool"]["overridden"], true);
    assert!(
        value["error"]
            .as_str()
            .is_some_and(|error| { error.contains("No such file") || error.contains("not found") })
    );
    assert!(
        value["fallback_steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| {
                step.as_str()
                    .is_some_and(|step| step.contains("activation-service --user-status"))
            })
    );
}

#[test]
fn daemon_log_missing_tool_reports_fallback_steps_json() {
    let output = vinpst_command()
        .env("VINPST_DAEMON_JOURNALCTL", "/tmp/vinpst-no-journalctl")
        .args(["daemon", "log", "--json"])
        .output()
        .expect("run vinpst daemon log with missing journalctl");

    let value = assert_json_success(output, "daemon missing journalctl json");
    assert_eq!(value["ok"], false);
    assert_eq!(value["action"], "log");
    assert_eq!(value["will_mutate_user_service"], false);
    assert_eq!(value["tool"]["name"], "journalctl");
    assert_eq!(value["tool"]["program"], "/tmp/vinpst-no-journalctl");
    assert_eq!(value["tool"]["overridden"], true);
    assert!(
        value["fallback_steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| {
                step.as_str()
                    .is_some_and(|step| step.contains("activation-service --user-status"))
            })
    );
}

#[test]
fn daemon_failure_text_hides_tool_plumbing() {
    let output = vinpst_command()
        .env("VINPST_DAEMON_SYSTEMCTL", "/tmp/vinpst-no-systemctl")
        .args(["daemon", "stop"])
        .output()
        .expect("run daemon stop with missing systemctl");
    let stdout = assert_stdout_success(output, "daemon failure human output");
    assert_human_output_hides_transport_details(&stdout);
}

#[test]
fn daemon_user_service_real_commands_report_external_output_json() {
    for (command, expected_stdout, will_mutate) in [
        ("stop", "--user stop vinpst-daemon.service\n", true),
        ("restart", "--user restart vinpst-daemon.service\n", true),
        ("log", "--user -u vinpst-daemon.service\n", false),
    ] {
        let output = vinpst_command()
            .env("VINPST_DAEMON_SYSTEMCTL", "/bin/echo")
            .env("VINPST_DAEMON_JOURNALCTL", "/bin/echo")
            .args(["daemon", command, "--json"])
            .output()
            .expect("run vinpst daemon user-service command --json");

        let value = assert_json_success(output, "daemon user service real json");
        assert_eq!(value["ok"], true);
        assert_eq!(value["dry_run"], false);
        assert_eq!(value["action"], command);
        assert_eq!(value["will_mutate_user_service"], will_mutate);
        assert_eq!(value["strategy"], "systemd-user-service");
        assert_eq!(value["tool"]["program"], "/bin/echo");
        assert_eq!(value["tool"]["overridden"], true);
        assert!(
            value["fallback_steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| {
                    step.as_str()
                        .is_some_and(|step| step.contains("daemon start --dry-run"))
                })
        );
        assert_eq!(value["command_argv"][0], "/bin/echo");
        assert_daemon_owner_probe_plan(&value);
        assert_eq!(value["exit_status"], 0);
        assert_eq!(value["stdout"], expected_stdout);
        assert_eq!(value["stderr"], "");
        assert!(value["next_steps"].as_array().unwrap().iter().any(|step| {
            step.as_str()
                .is_some_and(|step| step.contains("vinpst daemon"))
        }));
    }
}

#[test]
fn daemon_real_text_hides_tool_plumbing() {
    for (env_name, action) in [
        ("VINPST_DAEMON_SYSTEMCTL", "stop"),
        ("VINPST_DAEMON_JOURNALCTL", "log"),
    ] {
        let output = vinpst_command()
            .env(env_name, "/bin/echo")
            .args(["daemon", action])
            .output()
            .expect("run daemon action with echo tool");
        let stdout = assert_stdout_success(output, "daemon real human output");
        assert_human_output_hides_transport_details(&stdout);
    }
}
