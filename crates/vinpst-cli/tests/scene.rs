//! Integration tests for recognition scene listing and selection CLI paths.

mod common;

use std::{fs, path::Path};

use common::{
    assert_json_success, assert_stdout_success, isolated_vinpst_command, vinpst_command,
    write_temp_json,
};

#[test]
fn scene_list_json_reports_bundled_default_active_scene() {
    let (_home, mut command) = isolated_vinpst_command("vinpst-scene-list-default");
    let output = command
        .args(["scene", "list", "--json"])
        .output()
        .expect("run vinpst scene list --json");

    let value = assert_json_success(output, "scene list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["active_scene"], "__raw__");
    assert_eq!(value["scene_count"], 2);

    let scenes = value["scenes"].as_array().unwrap();
    assert_eq!(scenes[0]["id"], "__raw__");
    assert_eq!(scenes[0]["active"], true);
    assert_eq!(scenes[0]["candidate_count"], 0);
    assert_eq!(scenes[1]["id"], "__command__");
    assert_eq!(scenes[1]["prompt_configured"], true);
}

#[test]
fn scene_list_json_reports_scene_metadata() {
    let path = write_scene_fixture("vinpst-scene-list");

    let output = vinpst_command()
        .args(["scene", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinpst scene ls --json");
    fs::remove_file(&path).expect("remove temporary scene config");

    let value = assert_json_success(output, "scene list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["active_scene"], "rewrite");
    assert_eq!(value["scene_count"], 5);

    let scenes = value["scenes"].as_array().unwrap();
    assert_eq!(scenes[0]["id"], "raw");
    assert_eq!(scenes[0]["active"], false);
    assert_eq!(scenes[0]["prompt_configured"], false);
    assert_eq!(scenes[1]["id"], "rewrite");
    assert_eq!(scenes[1]["active"], true);
    assert_eq!(scenes[1]["provider_id"], "openai");
    assert_eq!(scenes[1]["model"], "gpt-scene");
    assert_eq!(scenes[1]["candidate_count"], 2);
    assert_eq!(scenes[1]["timeout_ms"], 2500);
    assert_eq!(scenes[1]["context_lines"], 4);
}

#[test]
fn scene_list_text_prints_table_and_active_marker() {
    let path = write_scene_fixture("vinpst-scene-list-text");

    let output = vinpst_command()
        .args(["scene", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst scene list text");
    fs::remove_file(&path).expect("remove temporary scene config");

    let stdout = assert_stdout_success(output, "scene list text");
    assert!(stdout.contains("ID\tLABEL\tPROVIDER\tMODEL\tCANDIDATES\tSTATUS"));
    assert!(stdout.contains("raw\tRaw\t-\t-\t0\t"));
    assert!(stdout.contains("rewrite\tRewrite\topenai\tgpt-scene\t2\tactive"));
    for internal in [
        "source:",
        "config_path:",
        "active_scene:",
        "scene_count:",
        "prompt",
        "timeout_ms",
        "context_lines",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
}

#[test]
fn scene_add_dry_run_json_validates_without_writing() {
    let path = write_scene_fixture("vinpst-scene-add-dry-run");
    let before = fs::read_to_string(&path).expect("read original scene config");

    let output = vinpst_command()
        .args([
            "scene",
            "add",
            "summarize",
            "--label",
            "Summarize",
            "--config",
        ])
        .arg(&path)
        .args([
            "--prompt",
            "Summarize selected text",
            "--provider-id",
            "openai",
            "--model",
            "gpt-scene",
            "--candidate-count",
            "2",
            "--timeout-ms",
            "2500",
            "--context-lines",
            "4",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst scene add dry-run");

    let value = assert_json_success(output, "scene add dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["scene_id"], "summarize");
    assert_eq!(value["active_scene"], "rewrite");
    assert_eq!(value["before_scene_count"], 5);
    assert_eq!(value["after_scene_count"], 6);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged scene config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_add_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-scene-add-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let output_path = root.join("out/scene.json");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinpst_command()
        .args([
            "scene",
            "add",
            "summarize",
            "--label",
            "Summarize",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst scene add --output");

    let value = assert_json_success(output, "scene add output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    let scenes = json["scenes"]["definitions"].as_array().unwrap();
    assert!(scenes.iter().any(|scene| scene["id"] == "summarize"));
    fs::remove_dir_all(root).expect("remove scene add output fixture dir");
}

#[test]
fn scene_remove_dry_run_json_validates_inactive_scene_without_writing() {
    let path = write_scene_fixture("vinpst-scene-remove-dry-run");
    let before = fs::read_to_string(&path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "remove", "command", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst scene remove dry-run");

    let value = assert_json_success(output, "scene remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["removed_scene_id"], "command");
    assert_eq!(value["active_scene"], "rewrite");
    assert_eq!(value["before_scene_count"], 5);
    assert_eq!(value["after_scene_count"], 4);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged scene config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_remove_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-scene-remove-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let output_path = root.join("out/scene.json");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "remove", "command", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst scene remove --output");

    let value = assert_json_success(output, "scene remove output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let scenes = read_json(&output_path)["scenes"]["definitions"]
        .as_array()
        .unwrap()
        .clone();
    assert!(scenes.iter().all(|scene| scene["id"] != "command"));
    fs::remove_dir_all(root).expect("remove scene remove output fixture dir");
}

#[test]
fn scene_edit_dry_run_json_validates_without_writing() {
    let path = write_scene_fixture("vinpst-scene-edit-dry-run");
    let before = fs::read_to_string(&path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "edit", "rewrite", "--config"])
        .arg(&path)
        .args([
            "--label",
            "Rewrite Better",
            "--prompt",
            "Rewrite with style",
            "--clear-model",
            "--candidate-count",
            "3",
            "--clear-timeout",
            "--context-lines",
            "2",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst scene edit dry-run");

    let value = assert_json_success(output, "scene edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["scene_id"], "rewrite");
    assert_eq!(value["active_scene"], "rewrite");
    let changed = value["changed_fields"].as_array().unwrap();
    assert!(changed.iter().any(|field| field == "label"));
    assert!(changed.iter().any(|field| field == "prompt"));
    assert!(changed.iter().any(|field| field == "model"));
    assert!(changed.iter().any(|field| field == "candidate_count"));
    assert!(changed.iter().any(|field| field == "timeout_ms"));
    assert!(changed.iter().any(|field| field == "context_lines"));
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged scene config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_edit_text_dry_run_outputs_expected_fields() {
    let path = write_scene_fixture("vinpst-scene-edit-text-dry-run");

    let output = vinpst_command()
        .args([
            "scene",
            "edit",
            "rewrite",
            "--label",
            "Rewrite Better",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene edit text dry-run");
    fs::remove_file(&path).expect("remove temporary scene config");

    let stdout = assert_stdout_success(output, "scene edit text dry-run");
    assert!(stdout.contains("Would update scene `rewrite`."));
    for internal in [
        "dry_run:",
        "source:",
        "active_scene:",
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
fn scene_edit_in_place_writes_backup_and_updates_fields() {
    let root = unique_temp_dir("vinpst-scene-edit-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "edit", "rewrite", "--config"])
        .arg(&config_path)
        .args([
            "--label",
            "Rewrite Better",
            "--clear-model",
            "--in-place",
            "--json",
        ])
        .output()
        .expect("run vinpst scene edit --in-place");

    let value = assert_json_success(output, "scene edit in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read scene edit backup config"),
        before
    );
    let json = read_json(&config_path);
    assert_eq!(json["scenes"]["definitions"][1]["label"], "Rewrite Better");
    assert!(
        json["scenes"]["definitions"][1]
            .as_object()
            .unwrap()
            .get("model")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove scene edit in-place fixture dir");
}

#[test]
fn scene_mutations_reject_invalid_inputs() {
    let path = write_scene_fixture("vinpst-scene-mutation-errors");

    let duplicate = vinpst_command()
        .args(["scene", "add", "rewrite", "--label", "Rewrite", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene add duplicate id");
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene `rewrite` already exists"));

    let missing = vinpst_command()
        .args(["scene", "edit", "missing", "--label", "Missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene edit missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene `missing` not found"));

    let implicit_builtin = vinpst_command()
        .args(["scene", "edit", "__raw__", "--label", "Raw", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene edit implicit builtin id");
    assert!(!implicit_builtin.status.success());
    let stderr = String::from_utf8(implicit_builtin.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene `__raw__` is not explicitly configured"));

    let noop = vinpst_command()
        .args(["scene", "edit", "rewrite", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene edit without field changes");
    assert!(!noop.status.success());
    let stderr = String::from_utf8(noop.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene edit requires at least one field change"));

    let conflict = vinpst_command()
        .args([
            "scene",
            "edit",
            "rewrite",
            "--prompt",
            "Prompt",
            "--clear-prompt",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene edit conflicting prompt flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene edit cannot combine --prompt and --clear-prompt"));

    let active = vinpst_command()
        .args(["scene", "remove", "rewrite", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene remove active id");
    assert!(!active.status.success());
    let stderr = String::from_utf8(active.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("refusing to remove active scene `rewrite`"));

    let missing_target = vinpst_command()
        .args(["scene", "add", "new", "--label", "New", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst scene add without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_remove_rejects_builtin_scene_without_mutating_config() {
    let path = write_temp_json(
        "vinpst-scene-remove-builtin",
        include_str!("../../../data/default-config.json"),
    );
    let before = fs::read_to_string(&path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "remove", "__command__", "--config"])
        .arg(&path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst scene remove builtin id");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("refusing to remove built-in scene `__command__`"));
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged scene config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_use_dry_run_json_validates_existing_scene_without_writing() {
    let path = write_scene_fixture("vinpst-scene-use-dry-run");
    let before = fs::read_to_string(&path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "use", "raw", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst scene use dry-run");

    let value = assert_json_success(output, "scene use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["before"], "rewrite");
    assert_eq!(value["after"], "raw");
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged scene config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary scene config");
}

#[test]
fn scene_use_text_dry_run_outputs_expected_fields() {
    let path = write_scene_fixture("vinpst-scene-use-text-dry-run");

    let output = vinpst_command()
        .args(["scene", "use", "command", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene use text dry-run");
    fs::remove_file(&path).expect("remove temporary scene config");

    let stdout = assert_stdout_success(output, "scene use text dry-run");
    assert!(stdout.contains("Would select scene `command`."));
    for internal in [
        "dry_run:",
        "source:",
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
fn scene_use_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-scene-use-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let output_path = root.join("out/scene.json");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "use", "command", "--config"])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst scene use --output");

    let value = assert_json_success(output, "scene use output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert_eq!(read_json(&output_path)["scenes"]["active_scene"], "command");
    fs::remove_dir_all(root).expect("remove scene use output fixture dir");
}

#[test]
fn scene_use_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-scene-use-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, scene_fixture_json()).expect("write scene config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original scene config");

    let output = vinpst_command()
        .args(["scene", "use", "raw", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst scene use --in-place");

    let value = assert_json_success(output, "scene use in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read scene backup config"),
        before
    );
    assert_eq!(read_json(&config_path)["scenes"]["active_scene"], "raw");
    fs::remove_dir_all(root).expect("remove scene use in-place fixture dir");
}

#[test]
fn scene_use_rejects_empty_missing_and_missing_write_target() {
    let path = write_scene_fixture("vinpst-scene-use-errors");

    let empty = vinpst_command()
        .args(["scene", "use", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene use empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene id cannot be empty"));

    let missing = vinpst_command()
        .args(["scene", "use", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst scene use missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("scene `missing` not found"));

    let missing_target = vinpst_command()
        .args(["scene", "use", "raw", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst scene use without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary scene config");
}

fn write_scene_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, scene_fixture_json())
}

fn scene_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {
        "providers": [{"id":"openai","base_url":"https://llm.example.test/v1"}]
      },
      "scenes": {
        "active_scene": "rewrite",
        "definitions": [
          {"id":"raw","label":"Raw","candidate_count":0},
          {"id":"rewrite","label":"Rewrite","prompt":"Polish text","provider_id":"openai","model":"gpt-scene","candidate_count":2,"timeout_ms":2500,"context_lines":4},
          {"id":"command","label":"Command","prompt":"Apply command","candidate_count":1}
        ]
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
