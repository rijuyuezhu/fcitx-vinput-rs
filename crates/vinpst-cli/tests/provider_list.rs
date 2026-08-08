//! Integration tests for ASR provider listing and selection CLI paths.

mod common;

use std::{fs, path::Path};

use common::{
    assert_json_success, assert_stdout_success, isolated_vinpst_command, vinpst_command,
    write_temp_json,
};

#[test]
fn provider_list_json_reports_bundled_default_active_provider() {
    let (_home, mut command) = isolated_vinpst_command("vinpst-provider-list-default");
    let output = command
        .args(["provider", "list", "--json"])
        .output()
        .expect("run vinpst provider list --json");

    let value = assert_json_success(output, "provider list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["active_provider"], "sherpa-onnx");
    assert_eq!(value["provider_count"], 1);

    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["id"], "sherpa-onnx");
    assert_eq!(providers[0]["type"], "local");
    assert_eq!(providers[0]["active"], true);
    assert_eq!(providers[0]["timeout_ms"], 15000);
    assert_eq!(providers[0]["hotwords_file_configured"], false);
    assert_eq!(providers[0]["command_configured"], false);
    assert_eq!(providers[0]["endpoint_configured"], false);
}

#[test]
fn provider_list_json_reports_multiple_provider_kinds_and_active_marker() {
    let path = write_provider_fixture("vinpst-provider-list");

    let output = vinpst_command()
        .args(["provider", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinpst provider ls --json");
    fs::remove_file(&path).expect("remove temporary provider config");

    let value = assert_json_success(output, "provider list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["provider_count"], 3);

    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers[0]["id"], "local");
    assert_eq!(providers[0]["type"], "local");
    assert_eq!(providers[0]["active"], false);
    assert_eq!(providers[0]["model"], "/tmp/model");
    assert_eq!(providers[0]["hotwords_file_configured"], true);

    assert_eq!(providers[1]["id"], "cmd");
    assert_eq!(providers[1]["type"], "command");
    assert_eq!(providers[1]["active"], true);
    assert_eq!(providers[1]["command_configured"], true);
    assert_eq!(providers[1]["args_count"], 1);
    assert_eq!(providers[1]["env_count"], 1);
    assert_eq!(providers[1]["timeout_ms"], 20000);

    assert_eq!(providers[2]["id"], "remote");
    assert_eq!(providers[2]["type"], "remote");
    assert_eq!(providers[2]["endpoint_configured"], true);
}

#[test]
fn provider_list_text_prints_table_and_active_marker() {
    let path = write_provider_fixture("vinpst-provider-list-text");

    let output = vinpst_command()
        .args(["provider", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst provider list text");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider list text");
    assert!(stdout.contains("ID\tTYPE\tMODEL\tSTATUS"));
    assert!(stdout.contains("local\tlocal\t/tmp/model\t"));
    assert!(stdout.contains("cmd\tcommand\t-\tactive"));
    assert!(stdout.contains("remote\tremote\tcloud\t"));
    for internal in [
        "source:",
        "config_path:",
        "active_provider:",
        "provider_count:",
        "hotwords",
        "endpoint",
        "timeout_ms",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
}
#[test]
fn provider_use_dry_run_json_validates_existing_provider_without_writing() {
    let path = write_provider_fixture("vinpst-provider-use-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "use", "remote", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst provider use dry-run");

    let value = assert_json_success(output, "provider use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["before"], "cmd");
    assert_eq!(value["after"], "remote");
    assert_eq!(value["provider_type"], "remote");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_use_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinpst-provider-use-text-dry-run");

    let output = vinpst_command()
        .args(["provider", "use", "remote", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider use text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider use text dry-run");
    assert!(stdout.contains("Would select ASR provider `remote`."));
    for internal in [
        "dry_run:",
        "source:",
        "before:",
        "after:",
        "provider_type:",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
}

#[test]
fn provider_use_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-provider-use-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "use", "local", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst provider use --output");

    let value = assert_json_success(output, "provider use output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert_eq!(read_json(&output_path)["asr"]["active_provider"], "local");
    fs::remove_dir_all(root).expect("remove provider output fixture dir");
}

#[test]
fn provider_use_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-provider-use-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "use", "local", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst provider use --in-place");

    let value = assert_json_success(output, "provider use in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider backup config"),
        before
    );
    assert_eq!(read_json(&config_path)["asr"]["active_provider"], "local");
    fs::remove_dir_all(root).expect("remove provider in-place fixture dir");
}

#[test]
fn provider_use_rejects_empty_missing_and_missing_write_target() {
    let path = write_provider_fixture("vinpst-provider-use-errors");

    let empty = vinpst_command()
        .args(["provider", "use", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider use empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinpst_command()
        .args(["provider", "use", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider use missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    let missing_target = vinpst_command()
        .args(["provider", "use", "local", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst provider use without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_create_dry_run_json_validates_local_provider_without_writing() {
    let path = write_provider_fixture("vinpst-provider-add-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "create", "extra", "--config"])
        .arg(&path)
        .args([
            "--model",
            "extra-model",
            "--hotwords-file",
            "/tmp/extra-hotwords.txt",
        ])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst provider create dry-run");

    let value = assert_json_success(output, "provider add dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "extra");
    assert_eq!(value["provider_type"], "local");
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["before_provider_count"], 3);
    assert_eq!(value["after_provider_count"], 4);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_create_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinpst-provider-add-text-dry-run");

    let output = vinpst_command()
        .args(["provider", "create", "extra", "--config"])
        .arg(&path)
        .args(["--model", "extra-model", "--dry-run"])
        .output()
        .expect("run vinpst provider create text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider add text dry-run");
    assert!(stdout.contains("Would add ASR provider `extra` (local)."));
    for internal in [
        "dry_run:",
        "source:",
        "active_provider:",
        "before_provider_count",
        "after_provider_count",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
}

#[test]
fn provider_create_output_writes_command_provider_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-provider-add-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args([
            "provider",
            "create",
            "cmd2",
            "--type",
            "command",
            "--command",
            "helper",
            "--config",
        ])
        .arg(&config_path)
        .args([
            "--arg=--json",
            "--env",
            "TOKEN=redacted",
            "--timeout-ms",
            "30000",
        ])
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst provider create command --output");

    let value = assert_json_success(output, "provider add output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["provider_id"], "cmd2");
    assert_eq!(value["provider_type"], "command");
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    let provider = json["asr"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["id"] == "cmd2")
        .expect("added command provider");
    assert_eq!(provider["type"], "command");
    assert_eq!(provider["command"], "helper");
    assert_eq!(provider["args"][0], "--json");
    assert_eq!(provider["env"]["TOKEN"], "redacted");
    assert_eq!(provider["timeout_ms"], 30000);
    fs::remove_dir_all(root).expect("remove provider add output fixture dir");
}

#[test]
fn provider_create_in_place_writes_remote_provider_and_backup() {
    let root = unique_temp_dir("vinpst-provider-add-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args([
            "provider",
            "create",
            "cloud",
            "--type",
            "remote",
            "--endpoint",
            "https://asr.example.test",
            "--config",
        ])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst provider create remote --in-place");

    let value = assert_json_success(output, "provider add in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["provider_id"], "cloud");
    assert_eq!(value["provider_type"], "remote");
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider backup config"),
        before
    );
    let json = read_json(&config_path);
    let provider = json["asr"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["id"] == "cloud")
        .expect("added remote provider");
    assert_eq!(provider["type"], "remote");
    assert_eq!(provider["endpoint"], "https://asr.example.test");
    fs::remove_dir_all(root).expect("remove provider add in-place fixture dir");
}

#[test]
fn provider_create_rejects_invalid_duplicate_and_missing_write_target() {
    let path = write_provider_fixture("vinpst-provider-add-errors");

    let empty = vinpst_command()
        .args(["provider", "create", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider create empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let duplicate = vinpst_command()
        .args(["provider", "create", "cmd", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider create duplicate id");
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `cmd` already exists"));

    let invalid_type = vinpst_command()
        .args(["provider", "create", "new", "--type", "bad", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider create invalid type");
    assert!(!invalid_type.status.success());
    let stderr = String::from_utf8(invalid_type.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("unsupported ASR provider type `bad`"));

    let missing_command = vinpst_command()
        .args([
            "provider", "create", "cmd2", "--type", "command", "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider create command without command");
    assert!(!missing_command.status.success());
    let stderr = String::from_utf8(missing_command.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("command ASR provider `cmd2` must configure a command"));

    let missing_endpoint = vinpst_command()
        .args([
            "provider", "create", "cloud", "--type", "remote", "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider create remote without endpoint");
    assert!(!missing_endpoint.status.success());
    let stderr = String::from_utf8(missing_endpoint.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("remote ASR provider `cloud` must configure an endpoint"));

    let bad_env = vinpst_command()
        .args([
            "provider",
            "create",
            "cmd3",
            "--type",
            "command",
            "--command",
            "helper",
            "--env",
            "TOKEN",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider create invalid env");
    assert!(!bad_env.status.success());
    let stderr = String::from_utf8(bad_env.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("provider env `TOKEN` is not KEY=VALUE"));

    let missing_target = vinpst_command()
        .args(["provider", "create", "new", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst provider create without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_configure_dry_run_json_updates_command_provider_without_writing() {
    let path = write_provider_fixture("vinpst-provider-edit-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "configure", "cmd", "--config"])
        .arg(&path)
        .args([
            "--command",
            "helper2",
            "--arg=--stream",
            "--env",
            "TOKEN=new",
            "--timeout-ms",
            "31000",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst provider configure dry-run");

    let value = assert_json_success(output, "provider edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "cmd");
    assert_eq!(value["before_provider_type"], "command");
    assert_eq!(value["after_provider_type"], "command");
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["wrote_config"], false);
    let changed = value["changed_fields"].as_array().unwrap();
    assert!(changed.iter().any(|field| field == "command"));
    assert!(changed.iter().any(|field| field == "args"));
    assert!(changed.iter().any(|field| field == "env"));
    assert!(changed.iter().any(|field| field == "timeout_ms"));
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_configure_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinpst-provider-edit-text-dry-run");

    let output = vinpst_command()
        .args(["provider", "configure", "local", "--config"])
        .arg(&path)
        .args(["--model", "/tmp/new-model", "--dry-run"])
        .output()
        .expect("run vinpst provider configure text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider edit text dry-run");
    assert!(stdout.contains("Would update ASR provider `local`."));
    for internal in [
        "dry_run:",
        "source:",
        "before_provider_type",
        "after_provider_type",
        "active_provider:",
        "changed_fields",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
}

#[test]
fn provider_configure_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-provider-edit-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "configure", "local", "--config"])
        .arg(&config_path)
        .args(["--model", "/tmp/new-model", "--clear-hotwords-file"])
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst provider configure --output");

    let value = assert_json_success(output, "provider edit output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["provider_id"], "local");
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(json["asr"]["providers"][0]["model"], "/tmp/new-model");
    assert!(
        json["asr"]["providers"][0]
            .as_object()
            .unwrap()
            .get("hotwords_file")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove provider edit output fixture dir");
}

#[test]
fn provider_configure_in_place_writes_remote_provider_and_backup() {
    let root = unique_temp_dir("vinpst-provider-edit-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "configure", "remote", "--config"])
        .arg(&config_path)
        .args([
            "--endpoint",
            "https://new-asr.example.test",
            "--model",
            "cloud-v2",
            "--in-place",
            "--json",
        ])
        .output()
        .expect("run vinpst provider configure --in-place");

    let value = assert_json_success(output, "provider edit in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["provider_id"], "remote");
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider edit backup config"),
        before
    );
    let json = read_json(&config_path);
    assert_eq!(
        json["asr"]["providers"][2]["endpoint"],
        "https://new-asr.example.test"
    );
    assert_eq!(json["asr"]["providers"][2]["model"], "cloud-v2");
    fs::remove_dir_all(root).expect("remove provider edit in-place fixture dir");
}

#[test]
fn provider_configure_rejects_invalid_missing_noop_conflicts_and_invalid_config() {
    let path = write_provider_fixture("vinpst-provider-edit-errors");

    let empty = vinpst_command()
        .args(["provider", "configure", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider configure empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinpst_command()
        .args(["provider", "configure", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider configure missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    let noop = vinpst_command()
        .args(["provider", "configure", "local", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider configure without field changes");
    assert!(!noop.status.success());
    let stderr = String::from_utf8(noop.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("provider edit requires at least one field change"));

    let conflict = vinpst_command()
        .args([
            "provider",
            "configure",
            "local",
            "--model",
            "new",
            "--clear-model",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider configure conflicting model flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("provider edit cannot combine --model and --clear-model"));

    let invalid_command = vinpst_command()
        .args([
            "provider",
            "configure",
            "cmd",
            "--clear-command",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider configure invalid command provider");
    assert!(!invalid_command.status.success());
    let stderr = String::from_utf8(invalid_command.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("command ASR provider `cmd` must configure a command"));

    let invalid_remote = vinpst_command()
        .args([
            "provider",
            "configure",
            "remote",
            "--clear-endpoint",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider configure invalid remote provider");
    assert!(!invalid_remote.status.success());
    let stderr = String::from_utf8(invalid_remote.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("remote ASR provider `remote` must configure an endpoint"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_edit_script_dry_run_resolves_short_id_without_launching_editor() {
    let root = unique_temp_dir("vinpst-provider-edit-script-dry-run");
    let script_path = root.join("provider.py");
    let config_path = root.join("config.json");
    let registry_path = root.join("providers.json");
    fs::write(&script_path, "print('provider')\n").expect("write provider script");
    fs::write(&config_path, editable_provider_config_json(&script_path))
        .expect("write editable provider config");
    fs::write(
        &registry_path,
        live_provider_registry_fixture_json("https://provider.example.test/entry.py"),
    )
    .expect("write provider registry");
    let before_config = fs::read_to_string(&config_path).expect("read provider config");
    let before_script = fs::read_to_string(&script_path).expect("read provider script");

    let output = vinpst_command()
        .args(["provider", "edit-script", "oai-stream", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .args(["--editor", "false", "--dry-run", "--json"])
        .output()
        .expect("run provider edit-script dry-run");

    let value = assert_json_success(output, "provider edit-script dry-run json");
    assert_eq!(value["selector"], "oai-stream");
    assert_eq!(value["provider_id"], "provider.openai-compatible.streaming");
    assert_eq!(value["registry_source"]["kind"], "file");
    assert_eq!(value["script_path"], script_path.to_string_lossy().as_ref());
    assert_eq!(value["editor"], "false");
    assert_eq!(value["edited"], false);
    assert_eq!(value["exit_status"], serde_json::Value::Null);
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged config"),
        before_config
    );
    assert_eq!(
        fs::read_to_string(&script_path).expect("read unchanged script"),
        before_script
    );
    fs::remove_dir_all(root).expect("remove provider edit-script dry-run fixture");
}

#[test]
fn provider_edit_script_runs_editor_without_mutating_config() {
    let root = unique_temp_dir("vinpst-provider-edit-script-run");
    let script_path = root.join("provider.py");
    let editor_path = root.join("editor.sh");
    let config_path = root.join("config.json");
    fs::write(&script_path, "print('provider')\n").expect("write provider script");
    fs::write(
        &editor_path,
        "#!/bin/sh\nprintf '\n# edited by test\n' >> \"$1\"\n",
    )
    .expect("write provider editor");
    fs::write(&config_path, editable_provider_config_json(&script_path))
        .expect("write editable provider config");
    let before_config = fs::read_to_string(&config_path).expect("read provider config");
    let editor = format!("sh {}", editor_path.display());

    let output = vinpst_command()
        .args([
            "provider",
            "edit-script",
            "provider.openai-compatible.streaming",
            "--config",
        ])
        .arg(&config_path)
        .args(["--editor", &editor, "--json"])
        .output()
        .expect("run provider edit-script editor");

    let value = assert_json_success(output, "provider edit-script editor json");
    assert_eq!(value["edited"], true);
    assert_eq!(value["exit_status"], 0);
    assert_eq!(value["registry_source"], serde_json::Value::Null);
    assert!(
        fs::read_to_string(&script_path)
            .expect("read edited provider script")
            .contains("# edited by test")
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged provider config"),
        before_config
    );
    fs::remove_dir_all(root).expect("remove provider edit-script run fixture");
}

#[test]
fn provider_edit_script_rejects_invalid_targets_and_editor_failure() {
    let provider_fixture = write_provider_fixture("vinpst-provider-edit-script-errors");

    let local = vinpst_command()
        .args(["provider", "edit-script", "local", "--config"])
        .arg(&provider_fixture)
        .args(["--editor", "true", "--dry-run"])
        .output()
        .expect("run provider edit-script local provider");
    assert!(!local.status.success());
    assert!(
        String::from_utf8(local.stderr)
            .expect("stderr should be utf8")
            .contains("is not a command provider")
    );

    let missing_script = vinpst_command()
        .args(["provider", "edit-script", "cmd", "--config"])
        .arg(&provider_fixture)
        .args(["--editor", "true", "--dry-run"])
        .output()
        .expect("run provider edit-script missing script");
    assert!(!missing_script.status.success());
    assert!(
        String::from_utf8(missing_script.stderr)
            .expect("stderr should be utf8")
            .contains("does not reference an existing editable script file")
    );
    fs::remove_file(provider_fixture).expect("remove provider error fixture");

    let root = unique_temp_dir("vinpst-provider-edit-script-editor-error");
    let script_path = root.join("provider.py");
    let config_path = root.join("config.json");
    fs::write(&script_path, "print('provider')\n").expect("write provider script");
    fs::write(&config_path, editable_provider_config_json(&script_path))
        .expect("write editable provider config");

    let short_without_registry = vinpst_command()
        .args(["provider", "edit-script", "oai-stream", "--config"])
        .arg(&config_path)
        .args(["--editor", "true", "--dry-run"])
        .output()
        .expect("run provider edit-script short id without registry");
    assert!(!short_without_registry.status.success());
    assert!(
        String::from_utf8(short_without_registry.stderr)
            .expect("stderr should be utf8")
            .contains("pass --registry <providers.json> to resolve a short id")
    );

    let editor_failure = vinpst_command()
        .args([
            "provider",
            "edit-script",
            "provider.openai-compatible.streaming",
            "--config",
        ])
        .arg(&config_path)
        .args(["--editor", "false"])
        .output()
        .expect("run provider edit-script failing editor");
    assert!(!editor_failure.status.success());
    assert!(
        String::from_utf8(editor_failure.stderr)
            .expect("stderr should be utf8")
            .contains("provider editor `false` exited with status 1")
    );
    fs::remove_dir_all(root).expect("remove provider edit-script error fixture");
}

#[test]
fn provider_remove_dry_run_json_validates_inactive_provider_without_writing() {
    let path = write_provider_fixture("vinpst-provider-remove-dry-run");
    let before = fs::read_to_string(&path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst provider remove dry-run");

    let value = assert_json_success(output, "provider remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["removed_provider_id"], "remote");
    assert_eq!(value["removed_provider_type"], "remote");
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["before_provider_count"], 3);
    assert_eq!(value["after_provider_count"], 2);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_remove_resolves_installed_short_id_from_registry() {
    let config_path = write_live_provider_config_fixture("vinpst-provider-remove-short-id");
    let registry_path = write_live_provider_registry_fixture(
        "vinpst-provider-remove-short-id-registry",
        "https://provider.example.test/entry.py",
    );
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "remove", "oai-stream", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst provider remove short id");

    let value = assert_json_success(output, "provider remove short id json");
    assert_eq!(
        value["removed_provider_id"],
        "provider.openai-compatible.streaming"
    );
    assert_eq!(value["removed_provider_type"], "command");
    assert_eq!(value["active_provider"], "base");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged provider config"),
        before
    );
    fs::remove_file(config_path).expect("remove temporary provider config");
    fs::remove_file(registry_path).expect("remove temporary provider registry");
}

#[test]
fn provider_remove_text_dry_run_outputs_expected_fields() {
    let path = write_provider_fixture("vinpst-provider-remove-text-dry-run");

    let output = vinpst_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider remove text dry-run");
    fs::remove_file(&path).expect("remove temporary provider config");

    let stdout = assert_stdout_success(output, "provider remove text dry-run");
    assert!(stdout.contains("Would remove ASR provider `remote`."));
    for internal in [
        "dry_run:",
        "source:",
        "removed_provider_type",
        "active_provider:",
        "before_provider_count",
        "after_provider_count",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
}

#[test]
fn provider_remove_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-provider-remove-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let output_path = root.join("out/provider.json");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "remove", "cmd", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst provider remove --output");

    let value = assert_json_success(output, "provider remove output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["removed_provider_id"], "cmd");
    assert_eq!(value["active_provider"], "");
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let output_json = read_json(&output_path);
    assert_eq!(output_json["asr"]["active_provider"], "");
    let providers = output_json["asr"]["providers"].as_array().unwrap().clone();
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().all(|provider| provider["id"] != "cmd"));
    fs::remove_dir_all(root).expect("remove provider remove output fixture dir");
}

#[test]
fn provider_remove_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-provider-remove-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, provider_fixture_json()).expect("write provider config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original provider config");

    let output = vinpst_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst provider remove --in-place");

    let value = assert_json_success(output, "provider remove in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read provider backup config"),
        before
    );
    let providers = read_json(&config_path)["asr"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().all(|provider| provider["id"] != "remote"));
    fs::remove_dir_all(root).expect("remove provider remove in-place fixture dir");
}

#[test]
fn provider_remove_rejects_empty_missing_local_and_missing_write_target() {
    let path = write_provider_fixture("vinpst-provider-remove-errors");

    let empty = vinpst_command()
        .args(["provider", "remove", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider remove empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinpst_command()
        .args(["provider", "remove", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider remove missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));
    assert!(stderr.contains("pass --registry"));

    let local = vinpst_command()
        .args(["provider", "remove", "local", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst provider remove local id");
    assert!(!local.status.success());
    let stderr = String::from_utf8(local.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("local ASR provider `local` cannot be removed"));

    let missing_target = vinpst_command()
        .args(["provider", "remove", "remote", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst provider remove without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary provider config");
}

#[test]
fn provider_list_available_json_reports_live_registry_and_install_status() {
    let config_path = write_live_provider_config_fixture("vinpst-provider-available-config-json");
    let registry_path = write_live_provider_registry_fixture(
        "vinpst-provider-available-registry-json",
        "https://provider.example.test/entry.py",
    );
    let i18n_path = write_temp_json(
        "vinpst-provider-available-i18n-json",
        r#"{
          "provider.openai-compatible.streaming.title":"OpenAI 兼容流式",
          "provider.openai-compatible.streaming.description":"OpenAI 兼容流式识别",
          "provider.doubao.batch.title":"豆包批量"
        }"#,
    );

    let output = vinpst_command()
        .args(["provider", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&i18n_path)
        .args(["--locale", "zh_CN"])
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run vinpst provider list --available --json");
    fs::remove_file(&config_path).expect("remove temporary config");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_file(&i18n_path).expect("remove temporary i18n");

    let value = assert_json_success(output, "provider list available json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["registry_source"]["kind"], "file");
    assert_eq!(value["i18n"]["kind"], "file");
    assert_eq!(value["i18n"]["loaded"], true);
    assert_eq!(value["provider_count"], 2);
    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers[0]["id"], "oai-stream");
    assert_eq!(
        providers[0]["machine_id"],
        "provider.openai-compatible.streaming"
    );
    assert_eq!(providers[0]["title"], "OpenAI 兼容流式");
    assert_eq!(providers[0]["description"], "OpenAI 兼容流式识别");
    assert_eq!(providers[0]["protocol"], "streaming");
    assert_eq!(providers[0]["status"], "installed");
    assert_eq!(providers[0]["envs"].as_array().unwrap().len(), 2);
    assert_eq!(providers[1]["id"], "doubao-batch");
    assert_eq!(providers[1]["title"], "豆包批量");
    assert_eq!(providers[1]["description"], serde_json::Value::Null);
    assert_eq!(providers[1]["protocol"], "batch");
    assert_eq!(providers[1]["status"], "available");
}

#[test]
fn provider_list_available_applies_automatic_local_i18n_override() {
    let config_path = write_live_provider_config_fixture("vinpst-provider-local-i18n-config");
    let registry_path = write_live_provider_registry_fixture(
        "vinpst-provider-local-i18n-registry",
        "https://provider.example.test/entry.py",
    );
    let preferred_i18n_path = write_temp_json(
        "vinpst-provider-local-i18n-preferred",
        r#"{
          "provider.openai-compatible.streaming.title":"Preferred title",
          "provider.openai-compatible.streaming.description":"Preferred description"
        }"#,
    );
    let config_home = tempfile::tempdir().expect("create XDG config home");
    let local_i18n_dir = config_home.path().join("vinpst");
    fs::create_dir_all(&local_i18n_dir).expect("create local i18n directory");
    fs::write(
        local_i18n_dir.join("i18n.local.json"),
        r#"{
          "provider.openai-compatible.streaming.title":"Local override title"
        }"#,
    )
    .expect("write local i18n override");

    let output = vinpst_command()
        .env("XDG_CONFIG_HOME", config_home.path())
        .args(["provider", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&preferred_i18n_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run provider list with automatic local i18n override");
    fs::remove_file(&config_path).expect("remove temporary config");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_file(&preferred_i18n_path).expect("remove preferred i18n");

    let value = assert_json_success(output, "provider list local i18n override");
    let providers = value["providers"].as_array().unwrap();
    assert_eq!(
        providers[0]["machine_id"],
        "provider.openai-compatible.streaming"
    );
    assert_eq!(providers[0]["title"], "Local override title");
    assert_eq!(providers[0]["description"], "Preferred description");
    assert_eq!(value["i18n"]["kind"], "file");
    assert_eq!(value["i18n"]["priority"][0], "local");
    assert_eq!(value["i18n"]["layers"]["local"]["loaded"], true);
    assert_eq!(value["i18n"]["layers"]["preferred"]["loaded"], true);
}

#[test]
fn provider_list_available_detects_and_normalizes_default_locale() {
    let config_path = write_live_provider_config_fixture("vinpst-provider-detected-locale-config");
    let registry_path = write_live_provider_registry_fixture(
        "vinpst-provider-detected-locale-registry",
        "https://provider.example.test/entry.py",
    );
    let i18n_path = write_temp_json(
        "vinpst-provider-detected-locale-i18n",
        r#"{"provider.openai-compatible.streaming.title":"Localized title"}"#,
    );
    let config_home = tempfile::tempdir().expect("create isolated XDG config home");

    let detected = vinpst_command()
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("LANGUAGE", "en-US.UTF-8")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG")
        .args(["provider", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&i18n_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run provider list with detected locale");
    let detected = assert_json_success(detected, "provider list detected locale");
    assert_eq!(detected["i18n"]["preferred_locale"], "en_US");

    let explicit = vinpst_command()
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("LANGUAGE", "en-US.UTF-8")
        .args(["provider", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&i18n_path)
        .args(["--locale", "zh"])
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run provider list with explicit locale");
    let explicit = assert_json_success(explicit, "provider list explicit locale");
    assert_eq!(explicit["i18n"]["preferred_locale"], "zh_CN");

    fs::remove_file(&config_path).expect("remove temporary config");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_file(&i18n_path).expect("remove temporary i18n");
}

#[test]
fn provider_list_available_text_prints_live_registry_table() {
    let config_path = write_live_provider_config_fixture("vinpst-provider-available-config-text");
    let registry_path = write_live_provider_registry_fixture(
        "vinpst-provider-available-registry-text",
        "https://provider.example.test/entry.py",
    );
    let i18n_path = write_temp_json(
        "vinpst-provider-available-i18n-text",
        r#"{
          "provider.openai-compatible.streaming.title":"OpenAI 兼容流式",
          "provider.openai-compatible.streaming.description":"OpenAI 兼容流式识别"
        }"#,
    );

    let output = vinpst_command()
        .args(["provider", "ls", "-a", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&i18n_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("run vinpst provider ls --available text");
    fs::remove_file(&config_path).expect("remove temporary config");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_file(&i18n_path).expect("remove temporary i18n");

    let stdout = assert_stdout_success(output, "provider list available text");
    assert!(stdout.contains("ID\tTITLE\tMODE\tSTATUS"));
    assert!(
        stdout.contains(
            "provider.openai-compatible.streaming\tOpenAI 兼容流式\tstreaming\tinstalled"
        )
    );
    assert!(stdout.contains("provider.doubao.batch\tdoubao-batch\tbatch\tavailable"));
    for internal in [
        "registry_source:",
        "config_source:",
        "provider_count:",
        "command",
        "envs",
        "readme",
        "description",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
}

#[test]
fn provider_list_available_rejects_adapter_registry() {
    let registry_path = write_temp_json(
        "vinpst-provider-wrong-registry-kind",
        r#"{"version":1,"items":[{"id":"adapter.test.proxy","command":"python3","script_urls":["https://example.test/entry.py"]}]}"#,
    );
    let output = vinpst_command()
        .args(["provider", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst provider list with adapter registry");
    fs::remove_file(&registry_path).expect("remove temporary registry");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("does not belong to `provider` registry"));
}

#[test]
fn provider_install_dry_run_resolves_short_id_without_writing() {
    let root = unique_temp_dir("vinpst-provider-install-dry-run");
    let config_path = root.join("config.json");
    fs::write(&config_path, empty_live_provider_config_fixture_json()).expect("write config");
    let registry_path = root.join("providers.json");
    fs::write(
        &registry_path,
        live_provider_registry_fixture_json("https://provider.example.test/entry.py"),
    )
    .expect("write provider registry");
    let provider_root = root.join("managed-providers");
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinpst_command()
        .args(["provider", "install", "oai-stream", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--provider-root")
        .arg(&provider_root)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run provider install dry-run");

    let value = assert_json_success(output, "provider install dry-run");
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["provider_id"], "provider.openai-compatible.streaming");
    assert_eq!(value["short_id"], "oai-stream");
    assert_eq!(value["protocol"], "streaming");
    assert_eq!(value["replacing_managed"], false);
    assert_eq!(value["wrote_script"], false);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        value["required_env"],
        serde_json::json!(["VINPST_ASR_API_KEY"])
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged config"),
        before
    );
    assert!(!provider_root.join("openai-compatible/streaming").exists());
    fs::remove_dir_all(root).expect("remove provider dry-run root");
}

#[test]
fn provider_install_downloads_script_and_updates_config_in_place() {
    let root = unique_temp_dir("vinpst-provider-install-live");
    let config_path = root.join("config.json");
    fs::write(&config_path, empty_live_provider_config_fixture_json()).expect("write config");
    let provider_root = root.join("managed-providers");
    let script = b"#!/usr/bin/env python3\nprint('provider installed')\n".to_vec();
    let (script_url, server) = serve_single_binary_response(script.clone());
    let registry_path = root.join("providers.json");
    fs::write(
        &registry_path,
        live_provider_registry_fixture_json(&script_url),
    )
    .expect("write provider registry");

    let output = vinpst_command()
        .args(["provider", "install", "oai-stream", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--provider-root")
        .arg(&provider_root)
        .args(["--in-place", "--json"])
        .output()
        .expect("run provider install");
    let requested_path = server.join().expect("join provider HTTP server");
    assert_eq!(requested_path, "/entry.py");

    let value = assert_json_success(output, "provider install json");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["protocol"], "streaming");
    assert_eq!(value["wrote_script"], true);
    assert_eq!(value["wrote_config"], true);
    let script_path = provider_root.join("openai-compatible/streaming");
    assert_eq!(
        fs::read(&script_path).expect("read installed script"),
        script
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&script_path)
            .expect("installed script metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0);
    }
    let document = read_json(&config_path);
    let provider = document["asr"]["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|provider| provider["id"] == "provider.openai-compatible.streaming")
        .expect("installed provider config");
    assert_eq!(provider["type"], "command");
    assert_eq!(provider["command"], "python3");
    assert_eq!(provider["args"], serde_json::json!([script_path]));
    assert_eq!(provider["timeout_ms"], 60000);
    assert_eq!(provider["env"]["VINPST_ASR_API_KEY"], "");
    assert_eq!(provider["env"]["VINPST_ASR_MODEL"], "");
    assert!(config_path.with_extension("json.bak").exists());
    fs::remove_dir_all(root).expect("remove provider install root");
}

#[test]
fn provider_install_refuses_user_defined_provider_before_download() {
    let root = unique_temp_dir("vinpst-provider-install-refuse-custom");
    let config_path = root.join("config.json");
    fs::write(
        &config_path,
        user_defined_live_provider_config_fixture_json(),
    )
    .expect("write config");
    let registry_path = root.join("providers.json");
    fs::write(
        &registry_path,
        live_provider_registry_fixture_json("http://127.0.0.1:9/should-not-fetch.py"),
    )
    .expect("write provider registry");

    let output = vinpst_command()
        .args(["provider", "install", "oai-stream", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--provider-root")
        .arg(root.join("managed-providers"))
        .args(["--dry-run", "--json"])
        .output()
        .expect("run provider install against custom provider");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("refusing to overwrite user-defined ASR provider"));
    fs::remove_dir_all(root).expect("remove custom provider root");
}

fn write_live_provider_config_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, live_provider_config_fixture_json())
}

fn write_live_provider_registry_fixture(prefix: &str, script_url: &str) -> std::path::PathBuf {
    let input = live_provider_registry_fixture_json(script_url);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::write(&path, input).expect("write live provider registry fixture");
    path
}

fn live_provider_registry_fixture_json(script_url: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": 1,
        "items": [
            {
                "id": "provider.openai-compatible.streaming",
                "short_id": "oai-stream",
                "stream": true,
                "command": "python3",
                "script_urls": [script_url],
                "readme_url": "https://example.test/openai/README.md",
                "envs": [
                    {"name": "VINPST_ASR_API_KEY", "required": true},
                    {"name": "VINPST_ASR_MODEL", "required": false}
                ]
            },
            {
                "id": "provider.doubao.batch",
                "short_id": "doubao-batch",
                "stream": false,
                "command": "python3",
                "script_urls": ["https://example.test/doubao.py"],
                "readme_url": "https://example.test/doubao/README.md",
                "envs": [
                    {"name": "VINPST_ASR_APP_ID", "required": true}
                ]
            }
        ]
    }))
    .expect("serialize live provider registry fixture")
}

fn live_provider_config_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "base",
        "providers": [
          {"id":"base","type":"local","model":"base"},
          {
            "id":"provider.openai-compatible.streaming",
            "type":"command",
            "command":"python3",
            "args":["/tmp/managed-providers/openai-compatible/streaming"],
            "env":{"VINPST_ASR_API_KEY":"preserve-me"},
            "timeout_ms":12000
          }
        ]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
}

fn empty_live_provider_config_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "base",
        "providers": [{"id":"base","type":"local","model":"base"}]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
}

fn user_defined_live_provider_config_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "base",
        "providers": [
          {"id":"base","type":"local","model":"base"},
          {"id":"provider.openai-compatible.streaming","type":"local","model":"custom"}
        ]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
}

fn serve_single_binary_response(bytes: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
    let url = format!("http://{}/entry.py", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test HTTP request");
        let mut request = [0_u8; 4096];
        let size = stream.read(&mut request).expect("read HTTP request");
        let first_line = String::from_utf8_lossy(&request[..size])
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        let path = first_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_owned();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write HTTP headers");
        stream.write_all(&bytes).expect("write HTTP body");
        path
    });
    (url, handle)
}

fn write_provider_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, provider_fixture_json())
}

fn editable_provider_config_json(script_path: &Path) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": 1,
        "asr": {
            "active_provider": "provider.openai-compatible.streaming",
            "providers": [{
                "id": "provider.openai-compatible.streaming",
                "type": "command",
                "command": "python3",
                "args": [script_path],
                "timeout_ms": 60000
            }]
        },
        "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
        }
    }))
    .expect("serialize editable provider config")
}

fn provider_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "cmd",
        "providers": [
          {"id":"local","type":"local","model":"/tmp/model","hotwords_file":"/tmp/hotwords.txt"},
          {"id":"cmd","type":"command","command":"helper","args":["--json"],"env":{"TOKEN":"redacted"},"timeout_ms":20000},
          {"id":"remote","type":"remote","endpoint":"https://asr.example.test","model":"cloud"}
        ]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
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

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json file"))
        .expect("parse json file")
}
