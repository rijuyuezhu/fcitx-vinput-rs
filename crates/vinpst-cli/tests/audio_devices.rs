//! Integration tests for audio device diagnostics CLI paths.

mod common;

use std::fs;

use common::{assert_json_success, isolated_vinpst_command, vinpst_command, write_temp_json};

fn assert_daemon_owner_probe(value: &serde_json::Value) {
    assert_eq!(
        value["daemon_owner_probe"]["target_name"],
        vinpst_protocol::dbus::SERVICE_BUS_NAME
    );
    let owner_methods = value["daemon_owner_probe"]["methods"]
        .as_array()
        .expect("doctor daemon owner probe methods");
    assert!(owner_methods.contains(&serde_json::json!("GetNameOwner")));
    assert!(owner_methods.contains(&serde_json::json!("GetConnectionUnixProcessID")));
    let process_fields = value["daemon_owner_probe"]["process_fields"]
        .as_array()
        .expect("doctor daemon owner probe fields");
    for field in ["unix_process_id", "exe", "cmdline"] {
        assert!(
            process_fields.contains(&serde_json::json!(field)),
            "missing doctor daemon owner probe process field {field}"
        );
    }
}

fn doctor_next_steps_text(value: &serde_json::Value) -> String {
    value["next_steps"]
        .as_array()
        .expect("doctor next steps")
        .iter()
        .map(|step| step.as_str().expect("doctor next step string"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn audio_devices_reports_default_capture_target_and_backend() {
    let output = vinpst_command()
        .arg("audio-devices")
        .output()
        .expect("run vinpst audio-devices");

    let value = assert_json_success(output, "audio devices summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["capture_device"], "default");
    assert_eq!(value["capture_target"]["kind"], "default");
    assert_eq!(
        value["capture_target"]["target_object"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["backend"],
        if cfg!(feature = "pipewire-backend") {
            "pipewire"
        } else {
            "unavailable"
        }
    );
    assert!(value["live"].is_boolean());
    let devices = value["devices"].as_array().unwrap();
    if value["live"] == true {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    } else {
        assert_eq!(devices.len(), 0);
    }
    if cfg!(feature = "pipewire-backend") {
        assert!(value["enumeration_error"].is_null() || value["enumeration_error"].is_string());
    } else {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    }
}

#[test]
fn audio_devices_preserves_configured_capture_target_object() {
    let path = write_temp_json(
        "vinpst-audio-devices",
        r#"
        {
          "version": 1,
          "global": {"capture_device": "  alsa_input.usb-mic  "},
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = vinpst_command()
        .args(["audio-devices", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst audio-devices with config");
    fs::remove_file(&path).expect("remove temporary config fixture");

    let value = assert_json_success(output, "audio devices summary");
    assert_eq!(value["capture_device"], "  alsa_input.usb-mic  ");
    assert_eq!(value["capture_target"]["kind"], "object");
    assert_eq!(
        value["capture_target"]["target_object"],
        "alsa_input.usb-mic"
    );
}

#[cfg(feature = "pipewire-backend")]
#[test]
fn audio_devices_reports_pipewire_enumeration_error_without_failing() {
    let config_dir = std::env::temp_dir().join(format!(
        "vinpst-missing-pipewire-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir(&config_dir).expect("create empty PipeWire config dir");

    let output = vinpst_command()
        .env("PIPEWIRE_CONFIG_DIR", &config_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_DIRS", &config_dir)
        .arg("audio-devices")
        .output()
        .expect("run vinpst audio-devices without PipeWire client config");
    fs::remove_dir(&config_dir).expect("remove empty PipeWire config dir");

    let value = assert_json_success(output, "audio devices summary without PipeWire config");
    assert_eq!(value["ok"], true);
    assert_eq!(value["backend"], "pipewire");
    assert_eq!(value["live"], false);
    assert_eq!(value["devices"].as_array().unwrap().len(), 0);
    assert!(
        value["enumeration_error"]
            .as_str()
            .is_some_and(|message| message.contains("enumerate PipeWire audio sources"))
    );
}

#[test]
fn diagnostics_discover_user_config_before_bundled_default() {
    let (root, mut doctor) = isolated_vinpst_command("vinpst-diagnostics-user-config");
    let config_path = root.path().join("config/fcitx-vinpst/config.json");
    fs::create_dir_all(config_path.parent().expect("config parent"))
        .expect("create user config directory");
    fs::write(
        &config_path,
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{
              "id": "mock",
              "type": "local",
              "model": "fixture"
            }]
          }
        }"#,
    )
    .expect("write user diagnostic config");

    let doctor_output = doctor
        .arg("doctor")
        .output()
        .expect("run doctor with discovered user config");
    let doctor_value = assert_json_success(doctor_output, "discovered doctor config");
    assert_eq!(doctor_value["ok"], true);
    assert_eq!(doctor_value["status"], "ready");
    assert_eq!(
        doctor_value["config_path"],
        config_path.to_string_lossy().as_ref()
    );
    assert_eq!(doctor_value["asr"]["target_provider_id"], "mock");

    let mut asr_state = vinpst_command();
    asr_state
        .env("HOME", root.path().join("home"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_CACHE_HOME", root.path().join("cache"))
        .arg("asr-state");
    let asr_value = assert_json_success(
        asr_state
            .output()
            .expect("run ASR state with discovered config"),
        "discovered ASR config",
    );
    assert_eq!(asr_value["target_provider_id"], "mock");
    assert_eq!(asr_value["target_model_id"], "fixture");
    assert_eq!(asr_value["has_effective_backend"], true);
}

#[test]
fn doctor_reports_combined_local_diagnostics() {
    let data_home = std::env::temp_dir().join(format!(
        "vinpst-doctor-data-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let addon_lib_dir = data_home.join("lib/fcitx5");

    let output = vinpst_command()
        .env("XDG_CONFIG_HOME", data_home.join("config-home"))
        .env("XDG_DATA_HOME", &data_home)
        .env(
            "VINPST_SHERPA_VAD_MODEL",
            data_home.join("missing-silero-vad.onnx"),
        )
        .env("VINPST_USER_FCITX_LIB_DIR", &addon_lib_dir)
        .arg("doctor")
        .output()
        .expect("run vinpst doctor");

    let value = assert_json_success(output, "doctor summary");
    assert_eq!(value["ok"], false);
    assert_eq!(value["status"], "setup-required");
    assert_eq!(value["config"]["ok"], true);
    assert_eq!(value["asr"]["target_provider_id"], "sherpa-onnx");
    assert_eq!(value["asr_timeout"]["provider_id"], "sherpa-onnx");
    assert_eq!(value["asr_timeout"]["provider_kind"], "local");
    assert_eq!(value["asr_timeout"]["timeout_ms"], 15_000);
    assert_eq!(value["asr_timeout"]["enforcement"], "unsupported");
    assert!(
        value["asr_timeout"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("diagnostic-only"))
    );
    assert_eq!(value["vad"]["status"], "missing");
    assert_eq!(value["vad"]["enabled"], true);
    assert_eq!(value["vad"]["available"], false);
    assert_eq!(value["vad"]["source"], "explicit_env");
    assert_eq!(value["vad"]["scope"], "offline-sherpa-only");
    assert!(
        value["vad"]["threshold"]
            .as_f64()
            .is_some_and(|threshold| (threshold - 0.45).abs() < 1e-6)
    );
    assert_eq!(value["audio"]["ok"], true);
    assert_eq!(value["audio"]["capture_target"]["kind"], "default");
    assert_eq!(
        value["activation_service"]["user_service_path"],
        data_home
            .join("dbus-1")
            .join("services")
            .join("org.fcitx.Vinpst.service")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["activation_service"]["user_service_exists"], false);
    assert!(
        value["activation_service"]["next_steps"]
            .as_array()
            .expect("doctor activation service next steps")
            .iter()
            .any(|step| step
                .as_str()
                .is_some_and(|step| step.contains("daemon owner/procfs probes")))
    );
    assert_eq!(
        value["fcitx_addon"]["user_module_path"],
        addon_lib_dir
            .join("fcitx5-vinpst.so")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["fcitx_addon"]["user_module_exists"], false);
    assert_daemon_owner_probe(&value);
    assert_eq!(
        value["fcitx_addon"]["user_addon_metadata_path"],
        data_home
            .join("fcitx5")
            .join("addon")
            .join("vinpst.conf")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(value["fcitx_addon"]["user_addon_metadata_exists"], false);
    let next_steps_text = doctor_next_steps_text(&value);
    assert!(next_steps_text.contains("vinpst asr-state --json"));
    assert!(next_steps_text.contains("vinpst model list --available"));
    assert!(next_steps_text.contains("vinpst model install <id-or-short-id>"));
    assert!(next_steps_text.contains("vinpst model use <id-or-short-id>"));
    assert!(next_steps_text.contains("vinpst provider list"));
    assert!(next_steps_text.contains("vinpst provider use sherpa-onnx"));
    assert!(next_steps_text.contains("vinpst hotword get"));
    assert!(next_steps_text.contains("vinpst device list"));
    assert!(next_steps_text.contains("vinpst device use <target>"));
    assert!(next_steps_text.contains("daemon D-Bus owner/procfs probes"));
    assert!(next_steps_text.contains("VINPST_SHERPA_VAD_MODEL"));
    assert!(next_steps_text.contains("cancellable command ASR provider"));
}

#[test]
fn doctor_reports_ready_when_the_active_backend_is_usable() {
    let config = write_temp_json(
        "vinpst-doctor-ready",
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{
              "id": "mock",
              "type": "local",
              "model": "fixture"
            }]
          }
        }"#,
    );
    let output = vinpst_command()
        .args(["doctor", "--config"])
        .arg(&config)
        .output()
        .expect("run vinpst doctor with a usable mock backend");
    fs::remove_file(&config).expect("remove doctor ready fixture");

    let value = assert_json_success(output, "doctor ready summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["status"], "ready");
    assert_eq!(value["asr"]["has_effective_backend"], true);
    let next_steps = doctor_next_steps_text(&value);
    assert!(!next_steps.contains("vinpst model list --available"));
}

#[test]
fn doctor_reports_explicit_vad_model_readiness() {
    let data_home = std::env::temp_dir().join(format!(
        "vinpst-doctor-vad-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&data_home).expect("create doctor VAD fixture dir");
    let model = data_home.join("silero_vad.onnx");
    fs::write(&model, b"diagnostic fixture").expect("write doctor VAD fixture");

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .env("VINPST_SHERPA_VAD_MODEL", &model)
        .arg("doctor")
        .output()
        .expect("run vinpst doctor with VAD model");
    fs::remove_dir_all(&data_home).expect("remove doctor VAD fixture dir");

    let value = assert_json_success(output, "doctor VAD readiness");
    assert_eq!(value["vad"]["status"], "ready");
    assert_eq!(value["vad"]["available"], true);
    assert_eq!(value["vad"]["source"], "explicit_env");
    assert_eq!(value["vad"]["model"], model.to_string_lossy().as_ref());
    assert_eq!(
        value["vad"]["requested_model"],
        model.to_string_lossy().as_ref()
    );
    assert!(
        value["next_steps"]
            .as_array()
            .expect("doctor next steps")
            .iter()
            .all(|step| !step
                .as_str()
                .unwrap_or_default()
                .contains("VINPST_SHERPA_VAD_MODEL"))
    );
}

#[test]
fn doctor_distinguishes_command_and_native_timeout_enforcement() {
    let command_config = write_temp_json(
        "vinpst-doctor-command-timeout",
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "providers": [{
              "id": "cmd",
              "type": "command",
              "command": "helper",
              "timeout_ms": 250
            }]
          }
        }"#,
    );
    let command_output = vinpst_command()
        .args(["doctor", "--config"])
        .arg(&command_config)
        .output()
        .expect("run vinpst doctor for command timeout");
    fs::remove_file(&command_config).expect("remove command timeout fixture");
    let command = assert_json_success(command_output, "doctor command timeout");
    assert_eq!(command["asr_timeout"]["provider_kind"], "command");
    assert_eq!(command["asr_timeout"]["timeout_ms"], 250);
    assert_eq!(command["asr_timeout"]["enforcement"], "enforced");
    assert!(
        command["asr_timeout"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("terminated"))
    );

    let native_config = write_temp_json(
        "vinpst-doctor-native-timeout",
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "native",
            "providers": [{
              "id": "native",
              "type": "local",
              "model": "/tmp/missing-native-model",
              "timeout_ms": 250
            }]
          }
        }"#,
    );
    let native_output = vinpst_command()
        .args(["doctor", "--config"])
        .arg(&native_config)
        .output()
        .expect("run vinpst doctor for native timeout");
    fs::remove_file(&native_config).expect("remove native timeout fixture");
    let native = assert_json_success(native_output, "doctor native timeout");
    assert_eq!(native["asr_timeout"]["provider_kind"], "local");
    assert_eq!(native["asr_timeout"]["timeout_ms"], 250);
    assert_eq!(native["asr_timeout"]["enforcement"], "unsupported");
    assert!(
        native["asr_timeout"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("diagnostic-only"))
    );
    assert!(doctor_next_steps_text(&native).contains("cancellable command ASR provider"));
}

#[test]
fn doctor_reports_existing_user_activation_exec_line() {
    let data_home = std::env::temp_dir().join(format!(
        "vinpst-doctor-service-home-{}-{}",
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
        "[D-BUS Service]\nName=org.fcitx.Vinpst\nExec=/tmp/vinpst-daemon --dbus --audio-backend pipewire --exit-when-executable-replaced\n",
    )
    .expect("write user activation service");

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .arg("doctor")
        .output()
        .expect("run vinpst doctor");

    let value = assert_json_success(output, "doctor summary with user service");
    assert_eq!(value["activation_service"]["user_service_exists"], true);
    assert_eq!(
        value["activation_service"]["user_service_name"],
        "org.fcitx.Vinpst"
    );
    assert_eq!(
        value["activation_service"]["user_service_name_matches"],
        true
    );
    assert_eq!(
        value["activation_service"]["user_service_exec"],
        "/tmp/vinpst-daemon --dbus --audio-backend pipewire --exit-when-executable-replaced"
    );
    assert_eq!(
        value["activation_service"]["read_error"],
        serde_json::Value::Null
    );
    assert!(
        value["activation_service"]["next_steps"]
            .as_array()
            .expect("activation service next steps")
            .iter()
            .any(|step| step
                .as_str()
                .is_some_and(|step| step.contains("daemon start --dry-run")))
    );
    std::fs::remove_dir_all(data_home).expect("remove service fixture");
}

#[test]
fn doctor_reports_existing_user_fcitx_addon_files() {
    let data_home = std::env::temp_dir().join(format!(
        "vinpst-doctor-addon-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    let addon_lib_dir = data_home.join("lib/fcitx5");
    let module_path = addon_lib_dir.join("fcitx5-vinpst.so");
    let metadata_path = data_home.join("fcitx5").join("addon").join("vinpst.conf");
    std::fs::create_dir_all(&addon_lib_dir).expect("create module dir");
    std::fs::create_dir_all(metadata_path.parent().unwrap()).expect("create metadata dir");
    std::fs::write(&module_path, "fake module").expect("write fake addon module");
    std::fs::write(
        &metadata_path,
        "[Addon]\nLibrary=fcitx5-vinpst\nType=SharedLibrary\n",
    )
    .expect("write addon metadata");

    let output = vinpst_command()
        .env("XDG_DATA_HOME", &data_home)
        .env("VINPST_USER_FCITX_LIB_DIR", &addon_lib_dir)
        .arg("doctor")
        .output()
        .expect("run vinpst doctor");

    let value = assert_json_success(output, "doctor summary with user addon");
    assert_eq!(value["fcitx_addon"]["user_module_exists"], true);
    assert_eq!(value["fcitx_addon"]["user_addon_metadata_exists"], true);
    assert_eq!(value["fcitx_addon"]["user_addon_library"], "fcitx5-vinpst");
    assert_eq!(value["fcitx_addon"]["user_addon_library_matches"], true);
    assert_eq!(value["fcitx_addon"]["user_addon_type"], "SharedLibrary");
    assert_eq!(value["fcitx_addon"]["read_error"], serde_json::Value::Null);
    std::fs::remove_dir_all(data_home).expect("remove addon fixture");
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create unique temp dir");
    path
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json file"))
        .expect("parse json file")
}

fn copy_default_config(root: &std::path::Path) -> std::path::PathBuf {
    let config_path = root.join("config.json");
    fs::copy(
        common::workspace_file("data/default-config.json"),
        &config_path,
    )
    .expect("copy default config");
    config_path
}

#[test]
fn device_list_json_reports_config_source_and_audio_summary() {
    let root = unique_temp_dir("vinpst-device-list-json");
    let config_path = copy_default_config(&root);

    let output = vinpst_command()
        .args(["device", "list", "--config"])
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run vinpst device list --json");

    let value = assert_json_success(output, "device list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], config_path.to_string_lossy().as_ref());
    assert_eq!(value["audio"]["ok"], true);
    assert_eq!(value["audio"]["capture_device"], "default");
    assert_eq!(value["audio"]["capture_target"]["kind"], "default");
}

#[test]
fn device_list_text_includes_default_target() {
    let (_home, mut command) = isolated_vinpst_command("vinpst-device-list-text");
    let output = command
        .args(["device", "list"])
        .output()
        .expect("run vinpst device list text");

    let stdout = common::assert_stdout_success(output, "device list text");
    assert!(stdout.contains("TARGET\tID\tNAME\tDESCRIPTION\tSTATUS"));
    assert!(stdout.contains("default\t-\tdefault\tDefault capture source\tactive"));
    for internal in [
        "source:",
        "config_path:",
        "capture_device:",
        "backend:",
        "live:",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
}

#[test]
fn device_use_dry_run_json_validates_without_writing() {
    let root = unique_temp_dir("vinpst-device-use-dry-run");
    let config_path = copy_default_config(&root);
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinpst_command()
        .args(["device", "use", "alsa_input.usb-mic", "--config"])
        .arg(&config_path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst device use dry-run");

    let value = assert_json_success(output, "device use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["before"], "default");
    assert_eq!(value["after"], "alsa_input.usb-mic");
    assert_eq!(value["capture_target"]["kind"], "object");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged config"),
        before
    );
}

#[test]
fn device_use_materializes_omitted_global_defaults() {
    let root = unique_temp_dir("vinpst-device-use-omitted-global");
    let config_path = copy_default_config(&root);
    let mut document = read_json(&config_path);
    document
        .as_object_mut()
        .expect("default config root")
        .remove("global");
    let before = serde_json::to_string_pretty(&document).expect("serialize compact config");
    fs::write(&config_path, &before).expect("write compact config");

    let output = vinpst_command()
        .args(["device", "use", "alsa_input.virtual-source", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst device use with omitted global defaults");

    let value = assert_json_success(output, "device use omitted global json");
    assert_eq!(value["before"], "default");
    assert_eq!(value["after"], "alsa_input.virtual-source");
    assert_eq!(
        read_json(&config_path)["global"]["capture_device"],
        "alsa_input.virtual-source"
    );
    assert_eq!(
        fs::read_to_string(root.join("config.json.bak")).expect("read compact backup"),
        before
    );
}

#[test]
fn device_use_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-device-use-output");
    let config_path = copy_default_config(&root);
    let output_path = root.join("out/device.json");
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinpst_command()
        .args(["device", "use", "alsa_input.output-mic", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst device use --output");

    let value = assert_json_success(output, "device use output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert_eq!(
        read_json(&output_path)["global"]["capture_device"],
        "alsa_input.output-mic"
    );
}

#[test]
fn device_use_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-device-use-in-place");
    let config_path = copy_default_config(&root);
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinpst_command()
        .args(["device", "use", "default", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst device use --in-place");

    let value = assert_json_success(output, "device use in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read backup config"),
        before
    );
    assert_eq!(
        read_json(&config_path)["global"]["capture_device"],
        "default"
    );
}

#[test]
fn device_use_rejects_empty_target_and_missing_write_target() {
    let root = unique_temp_dir("vinpst-device-use-errors");
    let config_path = copy_default_config(&root);

    let empty = vinpst_command()
        .args(["device", "use", "   ", "--config"])
        .arg(&config_path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst device use empty target");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("capture device cannot be empty"));

    let missing_target = vinpst_command()
        .args(["device", "use", "alsa_input.usb-mic", "--config"])
        .arg(&config_path)
        .output()
        .expect("run vinpst device use without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));
}
