//! Integration tests for LLM provider and text adapter list CLI paths.

mod common;

use std::fs;

use common::{
    assert_json_success, assert_stdout_success, isolated_vinpst_command, vinpst_command,
    write_temp_json,
};

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
fn llm_list_json_reports_bundled_default_empty_providers() {
    let (_home, mut command) = isolated_vinpst_command("vinpst-llm-list-default");
    let output = command
        .args(["llm", "list", "--json"])
        .output()
        .expect("run vinpst llm list --json");

    let value = assert_json_success(output, "llm list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["provider_count"], 0);
    assert_eq!(value["providers"].as_array().unwrap().len(), 0);
}

#[test]
fn llm_list_json_reports_provider_metadata_without_secrets() {
    let path = write_llm_fixture("vinpst-llm-list-json");

    let output = vinpst_command()
        .args(["llm", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinpst llm ls --json");
    fs::remove_file(&path).expect("remove temporary llm config");

    let value = assert_json_success(output, "llm list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["provider_count"], 2);
    let providers = value["providers"].as_array().unwrap();
    assert_eq!(providers[0]["id"], "openai");
    assert_eq!(providers[0]["base_url_configured"], true);
    assert_eq!(providers[0]["api_key_configured"], true);
    assert_eq!(providers[0]["model"], "gpt-4o-mini");
    assert_eq!(providers[0]["extra_body_configured"], true);
    assert_eq!(providers[0]["extra_field_count"], 0);
    assert_eq!(providers[1]["id"], "local");
    assert_eq!(providers[1]["api_key_configured"], false);
}

#[test]
fn llm_list_text_prints_table_without_secret_values() {
    let path = write_llm_fixture("vinpst-llm-list-text");

    let output = vinpst_command()
        .args(["llm", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst llm list text");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm list text");
    assert!(stdout.contains("ID\tBASE URL\tMODEL\tAPI KEY"));
    assert!(stdout.contains("openai\thttps://llm.example.test/v1\tgpt-4o-mini\tset"));
    assert!(stdout.contains("local\thttp://127.0.0.1:11434/v1\t-\tnot set"));
    for internal in [
        "source:",
        "config_path:",
        "provider_count:",
        "extra_body",
        "extra_fields",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
    assert!(!stdout.contains("secret-token"));
}

#[test]
fn llm_add_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinpst-llm-add-dry-run");
    let before = fs::read_to_string(&path).expect("read original llm config");

    let output = vinpst_command()
        .args([
            "llm",
            "add",
            "anthropic",
            "--base-url",
            "https://llm.example.test/anthropic",
            "--config",
        ])
        .arg(&path)
        .args([
            "--api-key",
            "secret-token",
            "--model",
            "claude-test",
            "--extra-body",
            r#"{"temperature":0.1}"#,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst llm add dry-run");

    let value = assert_json_success(output, "llm add dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "anthropic");
    assert_eq!(value["before_provider_count"], 2);
    assert_eq!(value["after_provider_count"], 3);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged llm config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_add_text_dry_run_outputs_expected_fields() {
    let path = write_llm_fixture("vinpst-llm-add-text-dry-run");

    let output = vinpst_command()
        .args([
            "llm",
            "add",
            "anthropic",
            "--base-url",
            "https://llm.example.test/anthropic",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm add text dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm add text dry-run");
    assert!(stdout.contains("Would add LLM provider `anthropic`."));
    for internal in [
        "dry_run:",
        "source:",
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
    assert!(!stdout.contains("secret-token"));
}

#[test]
fn llm_add_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-llm-add-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let output_path = root.join("out/llm.json");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinpst_command()
        .args([
            "llm",
            "add",
            "anthropic",
            "--base-url",
            "https://llm.example.test/anthropic",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst llm add --output");

    let value = assert_json_success(output, "llm add output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let providers = read_json(&output_path)["llm"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        providers
            .iter()
            .any(|provider| provider["id"] == "anthropic")
    );
    fs::remove_dir_all(root).expect("remove llm add output fixture dir");
}

#[test]
fn llm_edit_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinpst-llm-edit-dry-run");
    let before = fs::read_to_string(&path).expect("read original llm config");

    let output = vinpst_command()
        .args(["llm", "edit", "openai", "--config"])
        .arg(&path)
        .args([
            "--base-url",
            "https://new-llm.example.test/v1",
            "--api-key",
            "new-secret-token",
            "--clear-model",
            "--extra-body",
            r#"{"temperature":0.3}"#,
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst llm edit dry-run");

    let value = assert_json_success(output, "llm edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "openai");
    let changed = value["changed_fields"].as_array().unwrap();
    assert!(changed.iter().any(|field| field == "base_url"));
    assert!(changed.iter().any(|field| field == "api_key"));
    assert!(changed.iter().any(|field| field == "model"));
    assert!(changed.iter().any(|field| field == "extra_body"));
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged llm config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_edit_text_dry_run_outputs_expected_fields_without_secrets() {
    let path = write_llm_fixture("vinpst-llm-edit-text-dry-run");

    let output = vinpst_command()
        .args([
            "llm",
            "edit",
            "openai",
            "--api-key",
            "new-secret-token",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm edit text dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm edit text dry-run");
    assert!(stdout.contains("Would update LLM provider `openai`."));
    for internal in [
        "dry_run:",
        "source:",
        "changed_fields",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
    assert!(!stdout.contains("new-secret-token"));
}

#[test]
fn llm_edit_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-llm-edit-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let output_path = root.join("out/llm.json");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinpst_command()
        .args([
            "llm",
            "edit",
            "openai",
            "--base-url",
            "https://new-llm.example.test/v1",
            "--clear-model",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst llm edit --output");

    let value = assert_json_success(output, "llm edit output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(
        json["llm"]["providers"][0]["base_url"],
        "https://new-llm.example.test/v1"
    );
    assert!(
        json["llm"]["providers"][0]
            .as_object()
            .unwrap()
            .get("model")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove llm edit output fixture dir");
}

#[test]
fn llm_edit_in_place_writes_backup_and_clears_extra_body() {
    let root = unique_temp_dir("vinpst-llm-edit-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinpst_command()
        .args(["llm", "edit", "openai", "--clear-extra-body", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst llm edit --in-place");

    let value = assert_json_success(output, "llm edit in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read llm edit backup config"),
        before
    );
    let json = read_json(&config_path);
    assert!(
        json["llm"]["providers"][0]
            .as_object()
            .unwrap()
            .get("extra_body")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove llm edit in-place fixture dir");
}

#[test]
fn llm_edit_rejects_invalid_inputs() {
    let path = write_llm_fixture("vinpst-llm-edit-errors");

    let missing = vinpst_command()
        .args([
            "llm",
            "edit",
            "missing",
            "--base-url",
            "https://missing.example.test",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm edit missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `missing` not found"));

    let noop = vinpst_command()
        .args(["llm", "edit", "openai", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm edit without field changes");
    assert!(!noop.status.success());
    let stderr = String::from_utf8(noop.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider edit requires at least one field change"));

    let conflict = vinpst_command()
        .args([
            "llm",
            "edit",
            "openai",
            "--api-key",
            "new",
            "--clear-api-key",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm edit conflicting api key flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider edit cannot combine --api-key and --clear-api-key"));

    let invalid_extra = vinpst_command()
        .args(["llm", "edit", "openai", "--extra-body", "[]", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm edit invalid extra body");
    assert!(!invalid_extra.status.success());
    let stderr = String::from_utf8(invalid_extra.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider --extra-body must be a JSON object"));

    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_remove_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinpst-llm-remove-dry-run");
    let before = fs::read_to_string(&path).expect("read original llm config");

    let output = vinpst_command()
        .args(["llm", "remove", "local", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst llm remove dry-run");

    let value = assert_json_success(output, "llm remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["removed_provider_id"], "local");
    assert_eq!(value["cleared_scene_references"], 0);
    assert_eq!(value["before_provider_count"], 2);
    assert_eq!(value["after_provider_count"], 1);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged llm config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn llm_remove_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-llm-remove-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write llm config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original llm config");

    let output = vinpst_command()
        .args(["llm", "remove", "local", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst llm remove --in-place");

    let value = assert_json_success(output, "llm remove in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read llm backup config"),
        before
    );
    let providers = read_json(&config_path)["llm"]["providers"]
        .as_array()
        .unwrap()
        .clone();
    assert!(providers.iter().all(|provider| provider["id"] != "local"));
    fs::remove_dir_all(root).expect("remove llm remove in-place fixture dir");
}

#[test]
fn llm_remove_clears_scene_binding_and_preserves_unknown_fields() {
    let root = unique_temp_dir("vinpst-llm-remove-scene-binding");
    let config_path = root.join("config.json");
    let mut config: serde_json::Value = serde_json::from_str(llm_fixture_json()).unwrap();
    config["scenes"]["definitions"][0]["model"] = serde_json::json!("scene-model");
    config["scenes"]["definitions"][0]["future_field"] = serde_json::json!({"keep": true});
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize scene-bound config"),
    )
    .expect("write scene-bound config");

    let output = vinpst_command()
        .args(["llm", "remove", "openai", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst llm remove referenced provider");

    let value = assert_json_success(output, "llm remove referenced provider json");
    assert_eq!(value["cleared_scene_references"], 1);
    let updated = read_json(&config_path);
    let scene = updated["scenes"]["definitions"][0]
        .as_object()
        .expect("scene object");
    assert!(scene.get("provider_id").is_none());
    assert!(scene.get("model").is_none());
    assert_eq!(scene["future_field"], serde_json::json!({"keep": true}));
    assert!(
        updated["llm"]["providers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|provider| provider["id"] != "openai")
    );
    fs::remove_dir_all(root).expect("remove scene binding removal fixture dir");
}

#[test]
fn llm_mutations_reject_invalid_inputs() {
    let path = write_llm_fixture("vinpst-llm-mutation-errors");

    let duplicate = vinpst_command()
        .args([
            "llm",
            "add",
            "openai",
            "--base-url",
            "https://duplicate.example.test",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm add duplicate id");
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `openai` already exists"));

    let empty_base_url = vinpst_command()
        .args(["llm", "add", "new", "--base-url", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm add empty base url");
    assert!(!empty_base_url.status.success());
    let stderr = String::from_utf8(empty_base_url.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider base URL cannot be empty"));

    let invalid_extra_body = vinpst_command()
        .args([
            "llm",
            "add",
            "new",
            "--base-url",
            "https://new.example.test",
            "--extra-body",
            "[]",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm add invalid extra body");
    assert!(!invalid_extra_body.status.success());
    let stderr = String::from_utf8(invalid_extra_body.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider --extra-body must be a JSON object"));

    let missing = vinpst_command()
        .args(["llm", "remove", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm remove missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `missing` not found"));

    let missing_target = vinpst_command()
        .args([
            "llm",
            "add",
            "new",
            "--base-url",
            "https://new.example.test",
            "--config",
        ])
        .arg(&path)
        .output()
        .expect("run vinpst llm add without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn adapter_create_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinpst-adapter-add-dry-run");
    let before = fs::read_to_string(&path).expect("read original adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "create",
            "extra-adapter",
            "--command",
            "extra-helper",
            "--config",
        ])
        .arg(&path)
        .args([
            "--arg=--json",
            "--env",
            "TOKEN=secret-token",
            "--working-dir",
            "/tmp",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst adapter create dry-run");

    let value = assert_json_success(output, "adapter add dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["adapter_id"], "extra-adapter");
    assert_eq!(value["before_adapter_count"], 2);
    assert_eq!(value["after_adapter_count"], 3);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged adapter config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary adapter config");
}

#[test]
fn adapter_create_text_dry_run_outputs_expected_fields_without_secrets() {
    let path = write_llm_fixture("vinpst-adapter-add-text-dry-run");

    let output = vinpst_command()
        .args([
            "adapter",
            "create",
            "extra-adapter",
            "--command",
            "extra-helper",
            "--config",
        ])
        .arg(&path)
        .args(["--env", "TOKEN=secret-token", "--dry-run"])
        .output()
        .expect("run vinpst adapter create text dry-run");
    fs::remove_file(&path).expect("remove temporary adapter config");

    let stdout = assert_stdout_success(output, "adapter add text dry-run");
    assert!(stdout.contains("Would add text adapter `extra-adapter`."));
    for internal in [
        "dry_run:",
        "source:",
        "before_adapter_count",
        "after_adapter_count",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
    assert!(!stdout.contains("secret-token"));
}

#[test]
fn adapter_create_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-adapter-add-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write adapter config");
    let output_path = root.join("out/adapter.json");
    let before = fs::read_to_string(&config_path).expect("read original adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "create",
            "extra-adapter",
            "--command",
            "extra-helper",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst adapter create --output");

    let value = assert_json_success(output, "adapter add output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let adapters = read_json(&output_path)["llm"]["adapters"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        adapters
            .iter()
            .any(|adapter| adapter["id"] == "extra-adapter")
    );
    fs::remove_dir_all(root).expect("remove adapter add output fixture dir");
}

#[test]
fn adapter_remove_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinpst-adapter-remove-dry-run");
    let before = fs::read_to_string(&path).expect("read original adapter config");

    let output = vinpst_command()
        .args(["adapter", "remove", "simple-adapter", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst adapter remove dry-run");

    let value = assert_json_success(output, "adapter remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["removed_adapter_id"], "simple-adapter");
    assert_eq!(value["before_adapter_count"], 2);
    assert_eq!(value["after_adapter_count"], 1);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged adapter config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary adapter config");
}

#[test]
fn adapter_remove_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-adapter-remove-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write adapter config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original adapter config");

    let output = vinpst_command()
        .args(["adapter", "remove", "simple-adapter", "--config"])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst adapter remove --in-place");

    let value = assert_json_success(output, "adapter remove in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read adapter backup config"),
        before
    );
    let adapters = read_json(&config_path)["llm"]["adapters"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        adapters
            .iter()
            .all(|adapter| adapter["id"] != "simple-adapter")
    );
    fs::remove_dir_all(root).expect("remove adapter remove in-place fixture dir");
}

#[test]
fn adapter_remove_resolves_short_id_and_deletes_managed_script_in_place() {
    let root = unique_temp_dir("vinpst-adapter-remove-managed");
    let config_path = root.join("config.json");
    let registry_path = root.join("adapters.json");
    let adapter_root = root.join("managed-adapters");
    let script_path = adapter_root.join("mtranserver/proxy");
    fs::create_dir_all(script_path.parent().unwrap()).expect("create managed adapter directory");
    fs::write(&script_path, "#!/usr/bin/env python3\n").expect("write managed adapter script");
    fs::write(
        &config_path,
        managed_adapter_config_fixture_json(&script_path),
    )
    .expect("write managed adapter config");
    fs::write(
        &registry_path,
        live_adapter_registry_fixture_json("https://adapter.example.test/entry.py"),
    )
    .expect("write adapter registry");
    let before = fs::read_to_string(&config_path).expect("read managed adapter config");

    let output = vinpst_command()
        .args(["adapter", "remove", "mtran-proxy", "--registry"])
        .arg(&registry_path)
        .arg("--adapter-root")
        .arg(&adapter_root)
        .arg("--config")
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run managed adapter remove");

    let value = assert_json_success(output, "managed adapter remove json");
    assert_eq!(value["registry_source"]["kind"], "file");
    assert_eq!(value["removed_adapter_id"], "adapter.mtranserver.proxy");
    assert_eq!(value["managed_script"], true);
    assert_eq!(value["script_path"], script_path.to_string_lossy().as_ref());
    assert_eq!(value["script_existed"], true);
    assert_eq!(value["will_remove_script"], true);
    assert_eq!(value["removed_script"], true);
    assert_eq!(value["wrote_config"], true);
    assert!(!script_path.exists());
    assert_eq!(
        fs::read_to_string(config_path.with_extension("json.bak"))
            .expect("read managed adapter backup"),
        before
    );
    assert!(
        read_json(&config_path)["llm"]["adapters"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(root).expect("remove managed adapter removal fixture");
}

#[test]
fn adapter_remove_output_preserves_managed_script_and_input_config() {
    let root = unique_temp_dir("vinpst-adapter-remove-output");
    let config_path = root.join("config.json");
    let output_path = root.join("updated.json");
    let adapter_root = root.join("managed-adapters");
    let script_path = adapter_root.join("mtranserver/proxy");
    fs::create_dir_all(script_path.parent().unwrap()).expect("create managed adapter directory");
    fs::write(&script_path, "#!/usr/bin/env python3\n").expect("write managed adapter script");
    fs::write(
        &config_path,
        managed_adapter_config_fixture_json(&script_path),
    )
    .expect("write managed adapter config");
    let before = fs::read_to_string(&config_path).expect("read managed adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "remove",
            "adapter.mtranserver.proxy",
            "--adapter-root",
        ])
        .arg(&adapter_root)
        .arg("--config")
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run managed adapter remove to output");

    let value = assert_json_success(output, "managed adapter output remove json");
    assert_eq!(value["managed_script"], true);
    assert_eq!(value["script_existed"], true);
    assert_eq!(value["will_remove_script"], false);
    assert_eq!(value["removed_script"], false);
    assert!(script_path.exists());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    assert!(
        read_json(&output_path)["llm"]["adapters"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(root).expect("remove adapter output removal fixture");
}

#[test]
fn adapter_remove_never_deletes_user_defined_script() {
    let root = unique_temp_dir("vinpst-adapter-remove-custom");
    let config_path = root.join("config.json");
    let adapter_root = root.join("managed-adapters");
    let custom_script = root.join("custom-adapter.py");
    fs::write(&custom_script, "print('custom')\n").expect("write custom adapter script");
    fs::write(
        &config_path,
        managed_adapter_config_fixture_json(&custom_script),
    )
    .expect("write custom adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "remove",
            "adapter.mtranserver.proxy",
            "--adapter-root",
        ])
        .arg(&adapter_root)
        .arg("--config")
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run custom adapter remove");

    let value = assert_json_success(output, "custom adapter remove json");
    assert_eq!(value["managed_script"], false);
    assert_eq!(value["script_path"], serde_json::Value::Null);
    assert_eq!(value["script_existed"], false);
    assert_eq!(value["removed_script"], false);
    assert!(custom_script.exists());
    fs::remove_dir_all(root).expect("remove custom adapter removal fixture");
}

#[test]
fn adapter_remove_rejects_managed_script_directory_before_writing() {
    let root = unique_temp_dir("vinpst-adapter-remove-directory");
    let config_path = root.join("config.json");
    let adapter_root = root.join("managed-adapters");
    let script_path = adapter_root.join("mtranserver/proxy");
    fs::create_dir_all(&script_path).expect("create directory at managed script path");
    fs::write(
        &config_path,
        managed_adapter_config_fixture_json(&script_path),
    )
    .expect("write managed adapter config");
    let before = fs::read_to_string(&config_path).expect("read managed adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "remove",
            "adapter.mtranserver.proxy",
            "--adapter-root",
        ])
        .arg(&adapter_root)
        .arg("--config")
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run adapter remove with directory target");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("because it is a directory"));
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged adapter config"),
        before
    );
    assert!(!config_path.with_extension("json.bak").exists());
    assert!(script_path.is_dir());
    fs::remove_dir_all(root).expect("remove directory adapter removal fixture");
}

#[test]
fn adapter_mutations_reject_invalid_inputs() {
    let path = write_llm_fixture("vinpst-adapter-mutation-errors");

    let duplicate = vinpst_command()
        .args([
            "adapter",
            "create",
            "simple-adapter",
            "--command",
            "helper",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter create duplicate id");
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter `simple-adapter` already exists"));

    let empty_command = vinpst_command()
        .args(["adapter", "create", "new", "--command", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter create empty command");
    assert!(!empty_command.status.success());
    let stderr = String::from_utf8(empty_command.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter command cannot be empty"));

    let bad_env = vinpst_command()
        .args([
            "adapter",
            "create",
            "new",
            "--command",
            "helper",
            "--env",
            "TOKEN",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter create invalid env");
    assert!(!bad_env.status.success());
    let stderr = String::from_utf8(bad_env.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter env `TOKEN` is not KEY=VALUE"));

    let missing = vinpst_command()
        .args(["adapter", "remove", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter remove missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter `missing` not found"));

    let missing_target = vinpst_command()
        .args([
            "adapter",
            "create",
            "new",
            "--command",
            "helper",
            "--config",
        ])
        .arg(&path)
        .output()
        .expect("run vinpst adapter create without write target");
    assert!(!missing_target.status.success());
    let stderr = String::from_utf8(missing_target.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("config set writes require --output <path> or --in-place"));

    fs::remove_file(&path).expect("remove temporary adapter config");
}

#[test]
fn adapter_edit_dry_run_json_validates_without_writing() {
    let path = write_llm_fixture("vinpst-adapter-edit-dry-run");
    let before = fs::read_to_string(&path).expect("read original adapter config");

    let output = vinpst_command()
        .args(["adapter", "edit", "command-adapter", "--config"])
        .arg(&path)
        .args([
            "--command",
            "new-helper",
            "--arg=--mode=json",
            "--env",
            "TOKEN=new-secret",
            "--clear-working-dir",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst adapter edit dry-run");

    let value = assert_json_success(output, "adapter edit dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["adapter_id"], "command-adapter");
    let changed = value["changed_fields"].as_array().unwrap();
    assert!(changed.iter().any(|field| field == "command"));
    assert!(changed.iter().any(|field| field == "args"));
    assert!(changed.iter().any(|field| field == "env"));
    assert!(changed.iter().any(|field| field == "working_dir"));
    assert_eq!(value["wrote_config"], false);
    assert_eq!(
        fs::read_to_string(&path).expect("read unchanged adapter config"),
        before
    );
    fs::remove_file(&path).expect("remove temporary adapter config");
}

#[test]
fn adapter_edit_text_dry_run_outputs_expected_fields_without_secrets() {
    let path = write_llm_fixture("vinpst-adapter-edit-text-dry-run");

    let output = vinpst_command()
        .args([
            "adapter",
            "edit",
            "command-adapter",
            "--env",
            "TOKEN=new-secret",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter edit text dry-run");
    fs::remove_file(&path).expect("remove temporary adapter config");

    let stdout = assert_stdout_success(output, "adapter edit text dry-run");
    assert!(stdout.contains("Would update text adapter `command-adapter`."));
    for internal in [
        "dry_run:",
        "source:",
        "changed_fields",
        "will_write_config",
        "wrote_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal mutation detail: {internal}"
        );
    }
    assert!(!stdout.contains("new-secret"));
}

#[test]
fn adapter_edit_output_writes_valid_config_without_overwriting_input() {
    let root = unique_temp_dir("vinpst-adapter-edit-output");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write adapter config");
    let output_path = root.join("out/adapter.json");
    let before = fs::read_to_string(&config_path).expect("read original adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "edit",
            "command-adapter",
            "--command",
            "new-helper",
            "--clear-args",
            "--config",
        ])
        .arg(&config_path)
        .arg("--output")
        .arg(&output_path)
        .arg("--json")
        .output()
        .expect("run vinpst adapter edit --output");

    let value = assert_json_success(output, "adapter edit output json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], false);
    assert_eq!(value["output_path"], output_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&config_path).expect("read preserved input config"),
        before
    );
    let json = read_json(&output_path);
    assert_eq!(json["llm"]["adapters"][0]["command"], "new-helper");
    assert!(
        json["llm"]["adapters"][0]
            .as_object()
            .unwrap()
            .get("args")
            .is_none()
    );
    fs::remove_dir_all(root).expect("remove adapter edit output fixture dir");
}

#[test]
fn adapter_edit_in_place_writes_backup() {
    let root = unique_temp_dir("vinpst-adapter-edit-in-place");
    let config_path = root.join("config.json");
    fs::write(&config_path, llm_fixture_json()).expect("write adapter config");
    let backup_path = root.join("config.json.bak");
    let before = fs::read_to_string(&config_path).expect("read original adapter config");

    let output = vinpst_command()
        .args([
            "adapter",
            "edit",
            "command-adapter",
            "--working-dir",
            "/var/tmp",
            "--config",
        ])
        .arg(&config_path)
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst adapter edit --in-place");

    let value = assert_json_success(output, "adapter edit in-place json");
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());
    assert_eq!(
        fs::read_to_string(&backup_path).expect("read adapter edit backup config"),
        before
    );
    assert_eq!(
        read_json(&config_path)["llm"]["adapters"][0]["working_dir"],
        "/var/tmp"
    );
    fs::remove_dir_all(root).expect("remove adapter edit in-place fixture dir");
}

#[test]
fn adapter_edit_rejects_invalid_inputs() {
    let path = write_llm_fixture("vinpst-adapter-edit-errors");

    let missing = vinpst_command()
        .args([
            "adapter",
            "edit",
            "missing",
            "--command",
            "helper",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter edit missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter `missing` not found"));

    let noop = vinpst_command()
        .args(["adapter", "edit", "command-adapter", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter edit without changes");
    assert!(!noop.status.success());
    let stderr = String::from_utf8(noop.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter edit requires at least one field change"));

    let conflict = vinpst_command()
        .args([
            "adapter",
            "edit",
            "command-adapter",
            "--arg=one",
            "--clear-args",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter edit conflicting arg flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter edit cannot combine --arg and --clear-args"));

    let empty_command = vinpst_command()
        .args([
            "adapter",
            "edit",
            "command-adapter",
            "--command",
            "   ",
            "--config",
        ])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst adapter edit empty command");
    assert!(!empty_command.status.success());
    let stderr = String::from_utf8(empty_command.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter command cannot be empty"));

    fs::remove_file(&path).expect("remove temporary adapter config");
}

#[test]
fn adapter_start_stop_dry_run_json_reports_dbus_plan() {
    let path = write_llm_fixture("vinpst-adapter-lifecycle-json");
    let start = vinpst_command()
        .args(["adapter", "start", "command-adapter", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst adapter start dry-run json");
    let value = assert_json_success(start, "adapter start dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "start");
    assert_eq!(value["selector"], "command-adapter");
    assert_eq!(value["adapter_id"], "command-adapter");
    assert_eq!(value["config_source"], "file");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["called"], false);
    assert_eq!(value["dbus"]["method"], "StartAdapter");
    assert_daemon_owner_probe_plan(&value);

    let stop = vinpst_command()
        .args(["adapter", "stop", "command-adapter", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst adapter stop dry-run json");
    let value = assert_json_success(stop, "adapter stop dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "stop");
    assert_eq!(value["adapter_id"], "command-adapter");
    assert_eq!(value["dbus"]["method"], "StopAdapter");
    assert_daemon_owner_probe_plan(&value);
    fs::remove_file(path).expect("remove adapter lifecycle fixture");
}

#[test]
fn adapter_lifecycle_text_hides_transport_details() {
    let path = write_llm_fixture("vinpst-adapter-lifecycle-text");
    for action in ["start", "stop"] {
        let output = vinpst_command()
            .args(["adapter", action, "command-adapter", "--config"])
            .arg(&path)
            .arg("--dry-run")
            .output()
            .expect("run adapter lifecycle text dry-run");
        let stdout = assert_stdout_success(output, "adapter lifecycle text dry-run");
        assert!(stdout.contains("command-adapter"));
        for internal in [
            "org.fcitx.Vinpst",
            "StartAdapter",
            "StopAdapter",
            "owner_probe",
            "will_call_dbus",
        ] {
            assert!(
                !stdout.contains(internal),
                "leaked internal detail: {internal}"
            );
        }
    }
    fs::remove_file(path).expect("remove adapter lifecycle text fixture");
}

#[test]
fn adapter_status_dry_run_json_reports_text_adapter_state_plan() {
    let path = write_llm_fixture("vinpst-adapter-status-json");
    let output = vinpst_command()
        .args(["adapter", "status", "command-adapter", "--config"])
        .arg(&path)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst adapter status dry-run json");

    let value = assert_json_success(output, "adapter status dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["action"], "status");
    assert_eq!(value["selector"], "command-adapter");
    assert_eq!(value["adapter_id"], "command-adapter");
    assert_eq!(value["config_source"], "file");
    assert_eq!(value["will_call_dbus"], false);
    assert_eq!(value["called"], false);
    assert_eq!(value["dbus"]["method"], "GetTextAdapterState");
    assert_daemon_owner_probe_plan(&value);
    fs::remove_file(path).expect("remove adapter status fixture");
}

#[test]
fn adapter_status_text_hides_transport_details() {
    let output = vinpst_command()
        .args(["adapter", "status", "--dry-run"])
        .output()
        .expect("run vinpst adapter status text dry-run");

    let stdout = assert_stdout_success(output, "adapter status text dry-run");
    assert!(!stdout.is_empty());
    for internal in [
        "org.fcitx.Vinpst",
        "GetTextAdapterState",
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
fn global_json_flag_forces_adapter_status_json() {
    let path = write_llm_fixture("vinpst-adapter-status-global-json");
    let output = vinpst_command()
        .args(["-j", "adapter", "status", "command-adapter", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst -j adapter status dry-run");

    let value = assert_json_success(output, "global json adapter status dry-run");
    assert_eq!(value["ok"], true);
    assert_eq!(value["action"], "status");
    assert_eq!(value["adapter_id"], "command-adapter");
    assert_eq!(value["dbus"]["method"], "GetTextAdapterState");
    assert_daemon_owner_probe_plan(&value);
    fs::remove_file(path).expect("remove global adapter status fixture");
}

#[test]
fn adapter_runtime_commands_resolve_registry_short_id_before_dbus() {
    let config_path = write_live_adapter_config_fixture("vinpst-adapter-runtime-short-id");
    let registry_path = write_live_adapter_registry_fixture(
        "vinpst-adapter-runtime-short-id-registry",
        "https://adapter.example.test/entry.py",
    );

    for (action, method) in [
        ("start", "StartAdapter"),
        ("stop", "StopAdapter"),
        ("status", "GetTextAdapterState"),
    ] {
        let output = vinpst_command()
            .args(["adapter", action, "mtran-proxy", "--registry"])
            .arg(&registry_path)
            .arg("--config")
            .arg(&config_path)
            .args(["--dry-run", "--json"])
            .output()
            .expect("run adapter runtime command with short id");

        let value = assert_json_success(output, "adapter runtime short-id json");
        assert_eq!(value["action"], action);
        assert_eq!(value["selector"], "mtran-proxy");
        assert_eq!(value["adapter_id"], "adapter.mtranserver.proxy");
        assert_eq!(value["config_source"], "file");
        assert_eq!(value["registry_source"]["kind"], "file");
        assert_eq!(value["dbus"]["method"], method);
        assert_eq!(value["will_call_dbus"], false);
    }

    fs::remove_file(config_path).expect("remove runtime selector config");
    fs::remove_file(registry_path).expect("remove runtime selector registry");
}

#[test]
fn adapter_runtime_short_id_requires_explicit_registry() {
    let config_path = write_live_adapter_config_fixture("vinpst-adapter-runtime-no-registry");
    let output = vinpst_command()
        .args(["adapter", "start", "mtran-proxy", "--config"])
        .arg(&config_path)
        .arg("--dry-run")
        .output()
        .expect("run adapter start short id without registry");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("pass --registry <adapters.json> to resolve a short id"));
    fs::remove_file(config_path).expect("remove runtime no-registry config");
}

#[test]
fn adapter_start_stop_reject_empty_id_before_dbus() {
    let output = vinpst_command()
        .args(["adapter", "start", "   ", "--dry-run"])
        .output()
        .expect("run vinpst adapter start empty id");
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("text adapter id cannot be empty"));
}

#[test]
fn llm_test_dry_run_json_reports_redacted_request_without_http() {
    let path = write_llm_fixture("vinpst-llm-test-dry-run");

    let output = vinpst_command()
        .args(["llm", "test", "openai", "--config"])
        .arg(&path)
        .args([
            "--text",
            "hello from vinpst",
            "--timeout-ms",
            "1500",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst llm test dry-run json");
    fs::remove_file(&path).expect("remove temporary llm config");

    let value = assert_json_success(output, "llm test dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["source"], "file");
    assert_eq!(value["provider_id"], "openai");
    assert_eq!(value["timeout_ms"], 1500);
    assert_eq!(value["will_call_http"], false);
    assert_eq!(value["called"], false);
    assert_eq!(
        value["request"]["url"],
        "https://llm.example.test/v1/chat/completions"
    );
    let serialized = value.to_string();
    assert!(serialized.contains("<redacted>"));
    assert!(serialized.contains("hello from vinpst"));
    assert!(!serialized.contains("secret-token"));
    assert!(value["result"].is_null());
}

#[test]
fn llm_test_dry_run_reports_legacy_default_timeout() {
    let path = write_llm_fixture("vinpst-llm-test-default-timeout");

    let output = vinpst_command()
        .args(["llm", "test", "openai", "--config"])
        .arg(&path)
        .args(["--text", "default timeout", "--dry-run", "--json"])
        .output()
        .expect("run vinpst llm test default timeout dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let value = assert_json_success(output, "llm test default timeout dry-run");
    assert_eq!(value["timeout_ms"], 4_000);
}

#[test]
fn llm_test_dry_run_redacts_url_credentials_and_query_values() {
    let mut config: serde_json::Value = serde_json::from_str(llm_fixture_json()).unwrap();
    config["llm"]["providers"][0]["base_url"] = serde_json::Value::String(
        "https://url-user:url-password@llm.example.test/v1?api-key=url-secret#fragment".to_owned(),
    );
    let path = write_temp_json("vinpst-llm-test-url-redaction", &config.to_string());

    let output = vinpst_command()
        .args(["llm", "test", "openai", "--config"])
        .arg(&path)
        .args(["--text", "visible dry-run body", "--dry-run", "--json"])
        .output()
        .expect("run vinpst llm test URL redaction dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let value = assert_json_success(output, "llm test URL redaction dry-run");
    assert_eq!(
        value["request"]["url"],
        "https://llm.example.test/v1/chat/completions?api-key=REDACTED"
    );
    let serialized = value.to_string();
    assert!(serialized.contains("visible dry-run body"));
    assert!(!serialized.contains("url-user"));
    assert!(!serialized.contains("url-password"));
    assert!(!serialized.contains("url-secret"));
    assert!(!serialized.contains("fragment"));
    assert!(!serialized.contains("secret-token"));
}

#[test]
fn llm_test_text_dry_run_outputs_expected_fields_without_secrets() {
    let path = write_llm_fixture("vinpst-llm-test-text-dry-run");

    let output = vinpst_command()
        .args(["llm", "test", "openai", "--config"])
        .arg(&path)
        .args(["--timeout-ms", "1500", "--dry-run"])
        .output()
        .expect("run vinpst llm test text dry-run");
    fs::remove_file(&path).expect("remove temporary llm config");

    let stdout = assert_stdout_success(output, "llm test text dry-run");
    assert!(stdout.contains("Would test LLM provider `openai`."));
    assert!(stdout.contains("Request: https://llm.example.test/v1/chat/completions"));
    for internal in [
        "dry_run:",
        "source:",
        "timeout_ms:",
        "will_call_http",
        "called:",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal request detail: {internal}"
        );
    }
    assert!(!stdout.contains("secret-token"));
}

#[test]
fn llm_test_rejects_missing_and_empty_provider_before_http() {
    let path = write_llm_fixture("vinpst-llm-test-errors");

    let empty = vinpst_command()
        .args(["llm", "test", "   ", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm test empty id");
    assert!(!empty.status.success());
    let stderr = String::from_utf8(empty.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider id cannot be empty"));

    let missing = vinpst_command()
        .args(["llm", "test", "missing", "--config"])
        .arg(&path)
        .arg("--dry-run")
        .output()
        .expect("run vinpst llm test missing id");
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("LLM provider `missing` not found"));

    fs::remove_file(&path).expect("remove temporary llm config");
}

#[test]
fn adapter_install_plan_json_reports_registry_asset_plan() {
    let registry_path = write_registry_fixture("vinpst-adapter-install-plan-registry-json");
    let root = unique_temp_dir("vinpst-adapter-install-plan-json");
    let target_root = root.join("adapters");

    let output = vinpst_command()
        .args(["adapter", "install-plan", "command-adapter", "--registry"])
        .arg(&registry_path)
        .arg("--target-root")
        .arg(&target_root)
        .arg("--json")
        .output()
        .expect("run vinpst adapter install-plan --json");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_dir_all(root).expect("remove install plan root");

    let value = assert_json_success(output, "adapter install-plan json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["adapter_id"], "command-adapter");
    assert_eq!(
        value["registry_path"],
        registry_path.to_string_lossy().as_ref()
    );
    assert_eq!(value["target_root"], target_root.to_string_lossy().as_ref());
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["known_size_bytes"], 1234);
    assert_eq!(value["missing_checksum_count"], 1);
    let assets = value["assets"].as_array().unwrap();
    assert_eq!(assets[0]["entry_id"], "command-adapter");
    assert_eq!(assets[0]["source_path"], "adapters/command-adapter.tar.zst");
}

#[test]
fn adapter_install_plan_text_and_summary_only_outputs_expected_fields() {
    let registry_path = write_registry_fixture("vinpst-adapter-install-plan-registry-text");
    let root = unique_temp_dir("vinpst-adapter-install-plan-text");
    let target_root = root.join("adapters");

    let output = vinpst_command()
        .args(["adapter", "install-plan", "command-adapter", "--registry"])
        .arg(&registry_path)
        .arg("--target-root")
        .arg(&target_root)
        .output()
        .expect("run vinpst adapter install-plan text");
    let stdout = assert_stdout_success(output, "adapter install-plan text");
    assert!(stdout.contains("adapter_id: command-adapter"));
    assert!(stdout.contains("asset_count: 1"));
    assert!(stdout.contains("source_path	target_path	urls	checksum_policy	size_bytes"));
    assert!(stdout.contains("adapters/command-adapter.tar.zst"));

    let output = vinpst_command()
        .args([
            "adapter",
            "install-plan",
            "command-adapter",
            "--summary-only",
            "--registry",
        ])
        .arg(&registry_path)
        .arg("--target-root")
        .arg(&target_root)
        .output()
        .expect("run vinpst adapter install-plan summary text");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_dir_all(root).expect("remove install plan root");

    let stdout = assert_stdout_success(output, "adapter install-plan summary text");
    assert!(stdout.contains("asset_count: 1"));
    assert!(!stdout.contains("source_path	target_path"));
}

#[test]
fn adapter_install_plan_rejects_unknown_adapter() {
    let registry_path = write_registry_fixture("vinpst-adapter-install-plan-missing");
    let root = unique_temp_dir("vinpst-adapter-install-plan-missing-root");

    let output = vinpst_command()
        .args(["adapter", "install-plan", "missing", "--registry"])
        .arg(&registry_path)
        .arg("--target-root")
        .arg(&root)
        .arg("--json")
        .output()
        .expect("run vinpst adapter install-plan missing");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_dir_all(root).expect("remove install plan root");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("unknown adapter id `missing`"));
}

#[test]
fn adapter_list_available_json_reports_live_registry_and_install_status() {
    let config_path = write_live_adapter_config_fixture("vinpst-adapter-available-config-json");
    let registry_path = write_live_adapter_registry_fixture(
        "vinpst-adapter-available-registry-json",
        "https://adapter.example.test/entry.py",
    );
    let i18n_path = write_temp_json(
        "vinpst-adapter-available-i18n-json",
        r#"{
          "adapter.mtranserver.proxy.title":"MTranServer 代理",
          "adapter.mtranserver.proxy.description":"本地翻译代理",
          "adapter.other.proxy.title":"其他代理"
        }"#,
    );

    let output = vinpst_command()
        .args(["adapter", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&i18n_path)
        .args(["--locale", "zh_CN"])
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run vinpst adapter list --available --json");
    fs::remove_file(&config_path).expect("remove temporary config");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_file(&i18n_path).expect("remove temporary i18n");

    let value = assert_json_success(output, "adapter list available json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["registry_source"]["kind"], "file");
    assert_eq!(
        value["registry_source"]["path"],
        registry_path.to_string_lossy().as_ref()
    );
    assert_eq!(value["i18n"]["kind"], "file");
    assert_eq!(value["i18n"]["loaded"], true);
    assert_eq!(value["config_source"], "file");
    assert_eq!(value["adapter_count"], 2);
    let adapters = value["adapters"].as_array().unwrap();
    assert_eq!(adapters[0]["id"], "mtran-proxy");
    assert_eq!(adapters[0]["machine_id"], "adapter.mtranserver.proxy");
    assert_eq!(adapters[0]["title"], "MTranServer 代理");
    assert_eq!(adapters[0]["description"], "本地翻译代理");
    assert_eq!(adapters[0]["command"], "python3");
    assert_eq!(adapters[0]["status"], "installed");
    assert_eq!(adapters[0]["envs"].as_array().unwrap().len(), 2);
    assert_eq!(adapters[1]["id"], "other");
    assert_eq!(adapters[1]["title"], "其他代理");
    assert_eq!(adapters[1]["description"], serde_json::Value::Null);
    assert_eq!(adapters[1]["status"], "available");
}

#[test]
fn adapter_list_available_text_prints_live_registry_table() {
    let config_path = write_live_adapter_config_fixture("vinpst-adapter-available-config-text");
    let registry_path = write_live_adapter_registry_fixture(
        "vinpst-adapter-available-registry-text",
        "https://adapter.example.test/entry.py",
    );
    let i18n_path = write_temp_json(
        "vinpst-adapter-available-i18n-text",
        r#"{
          "adapter.mtranserver.proxy.title":"MTranServer 代理",
          "adapter.mtranserver.proxy.description":"本地翻译代理"
        }"#,
    );

    let output = vinpst_command()
        .args(["adapter", "ls", "-a", "--registry"])
        .arg(&registry_path)
        .arg("--i18n")
        .arg(&i18n_path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("run vinpst adapter ls --available text");
    fs::remove_file(&config_path).expect("remove temporary config");
    fs::remove_file(&registry_path).expect("remove temporary registry");
    fs::remove_file(&i18n_path).expect("remove temporary i18n");

    let stdout = assert_stdout_success(output, "adapter list available text");
    assert!(stdout.contains("ID\tTITLE\tSTATUS"));
    assert!(stdout.contains("adapter.mtranserver.proxy\tMTranServer 代理\tinstalled"));
    assert!(stdout.contains("adapter.other.proxy\tother\tavailable"));
    for internal in [
        "registry_source:",
        "config_source:",
        "adapter_count:",
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
fn adapter_list_available_rejects_wrong_registry_kind() {
    let registry_path = write_temp_json(
        "vinpst-adapter-wrong-registry-kind",
        r#"{"version":1,"items":[{"id":"provider.test.batch","command":"python3","script_urls":["https://example.test/entry.py"]}]}"#,
    );
    let output = vinpst_command()
        .args(["adapter", "list", "--available", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst adapter list with provider registry");
    fs::remove_file(&registry_path).expect("remove temporary registry");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("does not belong to `adapter` registry"));
}

#[test]
fn adapter_install_dry_run_resolves_short_id_without_writing() {
    let root = unique_temp_dir("vinpst-adapter-install-dry-run");
    let config_path = root.join("config.json");
    fs::write(&config_path, empty_adapter_config_fixture_json()).expect("write config");
    let registry_path = root.join("adapters.json");
    fs::write(
        &registry_path,
        live_adapter_registry_fixture_json("https://adapter.example.test/entry.py"),
    )
    .expect("write adapter registry");
    let adapter_root = root.join("managed-adapters");
    let before = fs::read_to_string(&config_path).expect("read original config");

    let output = vinpst_command()
        .args(["adapter", "install", "mtran-proxy", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--adapter-root")
        .arg(&adapter_root)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run adapter install dry-run");

    let value = assert_json_success(output, "adapter install dry-run");
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["adapter_id"], "adapter.mtranserver.proxy");
    assert_eq!(value["short_id"], "mtran-proxy");
    assert_eq!(value["replacing_managed"], false);
    assert_eq!(value["wrote_script"], false);
    assert_eq!(value["wrote_config"], false);
    assert_eq!(value["required_env"], serde_json::json!(["MTRAN_TOKEN"]));
    assert_eq!(
        fs::read_to_string(&config_path).expect("read unchanged config"),
        before
    );
    assert!(!adapter_root.join("mtranserver/proxy").exists());
    fs::remove_dir_all(root).expect("remove adapter dry-run root");
}

#[test]
fn adapter_install_downloads_script_and_updates_config_in_place() {
    let root = unique_temp_dir("vinpst-adapter-install-live");
    let config_path = root.join("config.json");
    fs::write(&config_path, empty_adapter_config_fixture_json()).expect("write config");
    let adapter_root = root.join("managed-adapters");
    let script = b"#!/usr/bin/env python3\nprint('adapter installed')\n".to_vec();
    let (script_url, server) = serve_single_binary_response(script.clone());
    let registry_path = root.join("adapters.json");
    fs::write(
        &registry_path,
        live_adapter_registry_fixture_json(&script_url),
    )
    .expect("write adapter registry");

    let output = vinpst_command()
        .args(["adapter", "install", "mtran-proxy", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--adapter-root")
        .arg(&adapter_root)
        .args(["--in-place", "--json"])
        .output()
        .expect("run adapter install");
    let requested_path = server.join().expect("join adapter HTTP server");
    assert_eq!(requested_path, "/entry.py");

    let value = assert_json_success(output, "adapter install json");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["replacing_managed"], false);
    assert_eq!(value["wrote_script"], true);
    assert_eq!(value["wrote_config"], true);
    let script_path = adapter_root.join("mtranserver/proxy");
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
    let adapter = document["llm"]["adapters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|adapter| adapter["id"] == "adapter.mtranserver.proxy")
        .expect("installed adapter config");
    assert_eq!(adapter["command"], "python3");
    assert_eq!(adapter["args"], serde_json::json!([script_path]));
    assert_eq!(adapter["env"]["MTRAN_URL"], "");
    assert_eq!(adapter["env"]["MTRAN_TOKEN"], "");
    assert!(config_path.with_extension("json.bak").exists());
    fs::remove_dir_all(root).expect("remove adapter install root");
}

#[test]
fn adapter_install_refuses_user_defined_adapter_before_download() {
    let root = unique_temp_dir("vinpst-adapter-install-refuse-custom");
    let config_path = root.join("config.json");
    fs::write(&config_path, user_defined_adapter_config_fixture_json()).expect("write config");
    let registry_path = root.join("adapters.json");
    fs::write(
        &registry_path,
        live_adapter_registry_fixture_json("http://127.0.0.1:9/should-not-fetch.py"),
    )
    .expect("write adapter registry");

    let output = vinpst_command()
        .args(["adapter", "install", "mtran-proxy", "--registry"])
        .arg(&registry_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--adapter-root")
        .arg(root.join("managed-adapters"))
        .args(["--dry-run", "--json"])
        .output()
        .expect("run adapter install against custom adapter");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("refusing to overwrite user-defined text adapter"));
    fs::remove_dir_all(root).expect("remove custom adapter root");
}

#[test]
fn adapter_list_json_reports_bundled_default_empty_adapters() {
    let (_home, mut command) = isolated_vinpst_command("vinpst-adapter-list-default");
    let output = command
        .args(["adapter", "list", "--json"])
        .output()
        .expect("run vinpst adapter list --json");

    let value = assert_json_success(output, "adapter list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"], "bundled-default");
    assert_eq!(value["config_path"], serde_json::Value::Null);
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapters"].as_array().unwrap().len(), 0);
}

#[test]
fn adapter_list_json_reports_adapter_metadata_without_secrets() {
    let path = write_llm_fixture("vinpst-adapter-list-json");

    let output = vinpst_command()
        .args(["adapter", "ls", "--config"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run vinpst adapter ls --json");
    fs::remove_file(&path).expect("remove temporary adapter config");

    let value = assert_json_success(output, "adapter list json with fixture");
    assert_eq!(value["source"], "file");
    assert_eq!(value["config_path"], path.to_string_lossy().as_ref());
    assert_eq!(value["adapter_count"], 2);
    let adapters = value["adapters"].as_array().unwrap();
    assert_eq!(adapters[0]["id"], "command-adapter");
    assert_eq!(adapters[0]["command_configured"], true);
    assert_eq!(adapters[0]["args_count"], 2);
    assert_eq!(adapters[0]["env_count"], 1);
    assert_eq!(adapters[0]["working_dir_configured"], true);
    assert_eq!(adapters[1]["id"], "simple-adapter");
    assert_eq!(adapters[1]["args_count"], 0);
}

#[test]
fn adapter_list_text_prints_table_without_secret_values() {
    let path = write_llm_fixture("vinpst-adapter-list-text");

    let output = vinpst_command()
        .args(["adapter", "list", "--config"])
        .arg(&path)
        .output()
        .expect("run vinpst adapter list text");
    fs::remove_file(&path).expect("remove temporary adapter config");

    let stdout = assert_stdout_success(output, "adapter list text");
    assert!(stdout.contains("ID\tSTATUS"));
    assert!(stdout.contains("command-adapter\tconfigured"));
    assert!(stdout.contains("simple-adapter\tconfigured"));
    for internal in [
        "source:",
        "config_path:",
        "adapter_count:",
        "command\t",
        "args\t",
        "env\t",
        "working_dir",
        "extra_fields",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
    assert!(!stdout.contains("secret-token"));
}

fn write_live_adapter_config_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, live_adapter_config_fixture_json())
}

fn write_live_adapter_registry_fixture(prefix: &str, script_url: &str) -> std::path::PathBuf {
    let input = live_adapter_registry_fixture_json(script_url);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::write(&path, input).expect("write live adapter registry fixture");
    path
}

fn live_adapter_registry_fixture_json(script_url: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": 1,
        "items": [
            {
                "id": "adapter.mtranserver.proxy",
                "short_id": "mtran-proxy",
                "command": "python3",
                "script_urls": [script_url],
                "readme_url": "https://example.test/mtran/README.md",
                "envs": [
                    {"name": "MTRAN_URL", "required": false},
                    {"name": "MTRAN_TOKEN", "required": true}
                ]
            },
            {
                "id": "adapter.other.proxy",
                "short_id": "other",
                "command": "python3",
                "script_urls": ["https://example.test/other.py"],
                "readme_url": "https://example.test/other/README.md",
                "envs": []
            }
        ]
    }))
    .expect("serialize live adapter registry fixture")
}

fn managed_adapter_config_fixture_json(script_path: &std::path::Path) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": 1,
        "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
        },
        "llm": {
            "providers": [],
            "adapters": [{
                "id": "adapter.mtranserver.proxy",
                "command": "python3",
                "args": [script_path],
                "env": {"MTRAN_TOKEN":"preserve-me"}
            }]
        },
        "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
        }
    }))
    .expect("serialize managed adapter config fixture")
}

fn live_adapter_config_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {
        "providers": [],
        "adapters": [
          {
            "id":"adapter.mtranserver.proxy",
            "command":"python3",
            "args":["MANAGED_SCRIPT_PATH_REPLACED_BY_TEST"],
            "env":{"MTRAN_TOKEN":"preserve-me"}
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

fn empty_adapter_config_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {"providers": [], "adapters": []},
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
      }
    }
    "#
}

fn user_defined_adapter_config_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {
        "providers": [],
        "adapters": [
          {
            "id":"adapter.mtranserver.proxy",
            "command":"custom-adapter",
            "args":["--serve"]
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

fn write_llm_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, llm_fixture_json())
}

fn write_registry_fixture(prefix: &str) -> std::path::PathBuf {
    write_temp_json(prefix, registry_fixture_json())
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

fn registry_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "models": [],
      "adapters": [
        {
          "id": "command-adapter",
          "label": "Command Adapter",
          "kind": "command",
          "assets": [{"path":"adapters/command-adapter.tar.zst","size_bytes":1234}]
        },
        {
          "id": "registry-only",
          "label": "Registry Only",
          "kind": "command",
          "assets": [{"path":"adapters/registry-only.tar.zst"}]
        }
      ]
    }
    "#
}

fn llm_fixture_json() -> &'static str {
    r#"
    {
      "version": 1,
      "asr": {
        "active_provider": "p",
        "providers": [{"id":"p","type":"local"}]
      },
      "llm": {
        "providers": [
          {"id":"openai","base_url":"https://llm.example.test/v1","api_key":"secret-token","model":"gpt-4o-mini","extra_body":{"temperature":0.2}},
          {"id":"local","base_url":"http://127.0.0.1:11434/v1"}
        ],
        "adapters": [
          {"id":"command-adapter","command":"adapter-helper","args":["--token","secret-token"],"env":{"TOKEN":"secret-token"},"working_dir":"/tmp"},
          {"id":"simple-adapter","command":"simple-helper"}
        ]
      },
      "scenes": {
        "active_scene": "raw",
        "definitions": [{"id":"raw","label":"Raw","provider_id":"openai","candidate_count":0}]
      }
    }
    "#
}
