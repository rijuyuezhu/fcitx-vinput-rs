//! Integration tests for ASR hotword inspection and mutation CLI paths.

mod common;

use std::{fs, path::Path};

use common::{
    assert_json_success, assert_stdout_success, isolated_vinpst_command, vinpst_command,
    write_temp_json,
};

#[test]
fn hotword_get_json_reports_bundled_default_active_provider() {
    let (_home, mut command) = isolated_vinpst_command("vinpst-hotword-get-default");
    let output = command
        .args(["hotword", "get", "--json"])
        .output()
        .expect("run vinpst hotword get --json");

    let value = assert_json_success(output, "hotword get json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["active_provider"], "sherpa-onnx");
    assert_eq!(value["provider_id"], "sherpa-onnx");
    assert_eq!(value["provider_type"], "local");
    assert_eq!(value["active"], true);
    assert_eq!(value["supported"], true);
    assert_eq!(value["configured"], false);
    assert_eq!(value["hotwords_file"], serde_json::Value::Null);
}

#[test]
fn hotword_get_json_reports_selected_provider_hotwords_file() {
    let path = write_hotword_fixture("vinpst-hotword-get-json");

    let output = vinpst_command()
        .args(["hotword", "get", "--provider", "local", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinpst hotword get selected provider");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let value = assert_json_success(output, "hotword get json selected provider");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["provider_id"], "local");
    assert_eq!(value["provider_type"], "local");
    assert_eq!(value["active"], false);
    assert_eq!(value["supported"], true);
    assert_eq!(value["configured"], true);
    assert_eq!(value["hotwords_file"], "/tmp/hotwords.txt");
}

#[test]
fn hotword_get_json_reports_remote_provider_as_unsupported() {
    let path = write_hotword_fixture("vinpst-hotword-get-remote");

    let output = vinpst_command()
        .args(["hotword", "get", "--provider", "remote", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinpst hotword get remote provider");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let value = assert_json_success(output, "hotword get json remote provider");
    assert_eq!(value["provider_id"], "remote");
    assert_eq!(value["provider_type"], "remote");
    assert_eq!(value["supported"], false);
    assert_eq!(value["configured"], false);
    assert_eq!(value["hotwords_file"], serde_json::Value::Null);
}

#[test]
fn hotword_get_text_reports_active_provider_hotwords_file() {
    let path = write_hotword_fixture("vinpst-hotword-get-text");

    let output = vinpst_command()
        .args(["hotword", "get", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst hotword get text");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let stdout = assert_stdout_success(output, "hotword get text");
    assert_eq!(stdout.trim(), "/tmp/cmd-hotwords.txt");
}

#[test]
fn hotword_get_rejects_empty_and_missing_provider() {
    let path = write_hotword_fixture("vinpst-hotword-get-errors");

    let empty = vinpst_command()
        .args(["hotword", "get", "--provider", "   ", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst hotword get empty provider");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider id cannot be empty"));

    let missing = vinpst_command()
        .args(["hotword", "get", "--provider", "missing", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst hotword get missing provider");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `missing` not found"));

    fs::remove_file(&path).expect("remove temporary hotword config");
}

#[test]
fn hotword_set_dry_run_json_validates_without_writing() {
    let path = write_hotword_fixture("vinpst-hotword-set-dry-run");
    let before = fs::read_to_string(&path).expect("read original hotword config");

    let output = vinpst_command()
        .args(["hotword", "set", "/tmp/new-hotwords.txt", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst hotword set dry-run");

    let value = assert_json_success(output, "hotword set dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["provider_id"], "cmd");
    assert_eq!(value["provider_type"], "command");
    assert_eq!(value["before"], "/tmp/cmd-hotwords.txt");
    assert_eq!(value["after"], "/tmp/new-hotwords.txt");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged hotword config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary hotword config");
}

#[test]
fn hotword_set_text_dry_run_outputs_expected_fields() {
    let path = write_hotword_fixture("vinpst-hotword-set-text-dry-run");

    let output = vinpst_command()
        .args(["hotword", "set", "/tmp/new-hotwords.txt", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst hotword set text dry-run");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let stdout = assert_stdout_success(output, "hotword set text dry-run");
    assert!(stdout.contains("Would set hotwords for `cmd` to `/tmp/new-hotwords.txt`."));
    for internal in [
        "dry_run:",
        "source:",
        "provider_type:",
        "before:",
        "after:",
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
fn hotword_set_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-hotword-set-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, hotword_fixture_json()).expect("write hotword config");
    let output_path = root.join("out/hotword.json");
    let before = fs::read_to_string(&config_path).expect("read original hotword config");

    let output = vinpst_command()
        .args([
            "hotword",
            "set",
            "/tmp/local-hotwords.txt",
            "--provider",
            "local",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst hotword set --output");

    let value = assert_json_success(output, "hotword set output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(
        json["asr"]["providers"][0]["hotwords_file"],
        "/tmp/local-hotwords.txt"
    );
    fs::remove_dir_all(root).expect("remove hotword output fixture dir");
}

#[test]
fn hotword_clear_text_dry_run_outputs_expected_fields() {
    let path = write_hotword_fixture("vinpst-hotword-clear-text-dry-run");

    let output = vinpst_command()
        .args(["hotword", "clear", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst hotword clear text dry-run");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let stdout = assert_stdout_success(output, "hotword clear text dry-run");
    assert!(stdout.contains("Would clear hotwords for `cmd`."));
    for internal in [
        "dry_run:",
        "source:",
        "provider_type:",
        "before:",
        "after:",
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
fn hotword_clear_in_place_writes_backup_and_removes_field() {
    let root = unique_temp_dir("vinpst-hotword-clear-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, hotword_fixture_json()).expect("write hotword config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original hotword config");

    let output = vinpst_command()
        .args(["hotword", "clear", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst hotword clear --in-place");

    let value = assert_json_success(output, "hotword clear in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read hotword backup config"),
        before
    );
    let json = read_json(&config_path);
    assert!(
        json["asr"]["providers"][1]
            .as_object()
            .unwrap()
            .get("hotwords_file")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove hotword in-place fixture dir");
}

#[test]
fn hotword_set_clear_reject_empty_remote_and_missing_write_target() {
    let path = write_hotword_fixture("vinpst-hotword-mutation-errors");

    let empty = vinpst_command()
        .args(["hotword", "set", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst hotword set empty path");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("hotwords file cannot be empty"));

    let remote = vinpst_command()
        .args([
            "hotword",
            "set",
            "/tmp/hotwords.txt",
            "--provider",
            "remote",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst hotword set remote provider");
    assert!(!remote.status.success());
    let stderr = String::from_utf8(remote.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `remote` does not support hotwords"));

    let missing_target = vinpst_command()
        .args(["hotword", "clear", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst hotword clear without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary hotword config");
}

#[test]
fn hotword_edit_dry_run_json_reports_editor_plan_without_launching() {
    let path = write_hotword_fixture("vinpst-hotword-edit-dry-run");

    let output = vinpst_command()
        .args(["hotword", "edit", "--config"])
        .arg(&path)
        .args(["--editor", "true", "--dry-run", "--json"])
        .output()
        .expect("run vinpst hotword edit dry-run");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let value = assert_json_success(output, "hotword edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "cmd");
    assert_eq!(value["provider_type"], "command");
    assert_eq!(value["hotwords_file"], "/tmp/cmd-hotwords.txt");
    assert_eq!(value["editor_argv"][0], "true");
    assert_eq!(value["edited"], false);
    assert_eq!(value["exit_status"], serde_json::Value::Null);
}

#[test]
fn hotword_edit_text_dry_run_outputs_expected_fields() {
    let path = write_hotword_fixture("vinpst-hotword-edit-text-dry-run");

    let output = vinpst_command()
        .args(["hotword", "edit", "--config"])
        .arg(&path)
        .args(["--editor", "true", "--dry-run"])
        .output()
        .expect("run vinpst hotword edit text dry-run");
    fs::remove_file(&path).expect("remove temporary hotword config");

    let stdout = assert_stdout_success(output, "hotword edit text dry-run");
    assert!(stdout.contains("Would open hotwords file: /tmp/cmd-hotwords.txt"));
    for internal in [
        "dry_run:",
        "source:",
        "provider_type:",
        "editor:",
        "edited:",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal editor detail: {internal}"
        );
    }
}

#[test]
fn hotword_edit_runs_editor_for_configured_file() {
    let root = unique_temp_dir("vinpst-hotword-edit-run");
    let config_path = root.join("config.json");
    let hotwords_path = root.join("hotwords.txt");
    fs::write(
        &hotwords_path,
        "before
",
    )
    .expect("write hotwords file");
    fs::write(
        &config_path,
        hotword_fixture_json().replace(
            "/tmp/cmd-hotwords.txt",
            hotwords_path.to_string_lossy().as_ref(),
        ),
    )
    .expect("write hotword config");

    let output = vinpst_command()
        .args(["hotword", "edit", "--config"])
        .arg(&config_path)
        .args(["--editor", "true", "--json"])
        .output()
        .expect("run vinpst hotword edit");

    let value = assert_json_success(output, "hotword edit json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(
        value["hotwords_file"],
        hotwords_path.to_string_lossy().as_ref()
    );
    assert_eq!(value["edited"], true);
    assert_eq!(value["exit_status"], 0);
    fs::remove_dir_all(root).expect("remove hotword edit fixture dir");
}

#[test]
fn hotword_edit_rejects_missing_file_config_and_remote_provider() {
    let path = write_hotword_fixture("vinpst-hotword-edit-errors");

    let missing_config = vinpst_command()
        .args(["hotword", "edit", "--provider", "remote", "--config"])
        .arg(&path)
        .args(["--editor", "true", "--dry-run"])
        .output()
        .expect("run vinpst hotword edit remote provider");
    assert!(!missing_config.status.success());
    let stderr = String::from_utf8(missing_config.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("ASR provider `remote` does not support hotwords"));

    let no_file_config = write_temp_json(
        "vinpst-hotword-edit-no-file",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "local",
            "providers": [{"id":"local","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );
    let no_file = vinpst_command()
        .args(["hotword", "edit", "--config"])
        .arg(&no_file_config)
        .args(["--editor", "true", "--dry-run"])
        .output()
        .expect("run vinpst hotword edit without configured file");
    assert!(!no_file.status.success());
    let stderr = String::from_utf8(no_file.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("No hotwords file configured. Use 'hotword set <path>' first."));

    let failing_editor = vinpst_command()
        .args(["hotword", "edit", "--config"])
        .arg(&path)
        .args(["--editor", "false"])
        .output()
        .expect("run vinpst hotword edit with failing editor");
    assert!(!failing_editor.status.success());
    let stderr = String::from_utf8(failing_editor.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("hotword editor exited with status"));

    fs::remove_file(&path).expect("remove temporary hotword config");
    fs::remove_file(&no_file_config).expect("remove no-file hotword config");
}

fn write_hotword_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, hotword_fixture_json())
}

fn hotword_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "cmd",
        "providers": [
          {"id":"local","type":"local","model":"/tmp/model","hotwords_file":"/tmp/hotwords.txt"},
          {"id":"cmd","type":"command","command":"helper","hotwords_file":"/tmp/cmd-hotwords.txt"},
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
