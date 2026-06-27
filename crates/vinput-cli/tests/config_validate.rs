//! Integration tests for config validation CLI paths.

use std::{fs, process::Command};

fn write_temp_config(contents: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "vinput-config-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::write(&path, contents).expect("write temporary config fixture");
    path
}

#[test]
fn config_validate_prints_summary_for_valid_config() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
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

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("config summary should be JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["active_scene"], "raw");
    assert_eq!(value["active_provider"], "p");
    assert_eq!(value["scene_count"], 1);
    assert_eq!(value["provider_count"], 1);
    assert_eq!(value["registry_mirror_count"], 0);
}

#[test]
fn config_validate_fails_for_duplicate_scene_ids() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [
              {"id":"raw","label":"Raw","candidate_count":0},
              {"id":"raw","label":"Raw again","candidate_count":0}
            ]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("duplicate scene id `raw`"));
}

#[test]
fn config_validate_fails_for_empty_registry_mirror() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "registry": {"base_urls": [""]},
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

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty registry base URL"));
}

#[test]
fn config_validate_fails_for_duplicate_provider_ids() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [
              {"id":"p","type":"local"},
              {"id":"p","type":"local"}
            ]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("duplicate ASR provider id `p`"));
}

#[test]
fn config_validate_fails_for_unknown_active_provider() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "missing",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("active ASR provider `missing` is not defined"));
}

#[test]
fn config_validate_fails_for_unknown_active_scene() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "missing",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("active scene `missing` is not defined"));
}

#[test]
fn config_validate_fails_for_duplicate_registry_mirrors() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "registry": {
            "base_urls": ["https://mirror.invalid/root", "https://mirror.invalid/root"]
          },
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

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("duplicate registry base URL `https://mirror.invalid/root`"));
}

#[test]
fn config_validate_summary_only_matches_summary_shape() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
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

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .args(["--summary-only"])
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("config summary should be JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["scene_count"], 1);
    assert!(value.get("scenes").is_none());
}

#[test]
fn config_validate_fails_for_empty_provider_ids() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "",
            "providers": [{"id":"   ","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty ASR provider id"));
}

#[test]
fn config_validate_fails_for_empty_capture_device() {
    let path = write_temp_config(
        r#"{"version":1,"global":{"capture_device":"   "},"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty capture device"));
}

#[test]
fn config_validate_fails_for_empty_active_provider() {
    let path = write_temp_config(
        r#"{"version":1,"asr":{"active_provider":"   ","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty active ASR provider id"));
}

#[test]
fn config_validate_fails_for_empty_default_language() {
    let path = write_temp_config(
        r#"{"version":1,"global":{"default_language":"   "},"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty default language"));
}

#[test]
fn config_validate_fails_for_empty_scene_ids() {
    let path = write_temp_config(
        r#"{"version":1,"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"","definitions":[{"id":"   ","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty scene id"));
}

#[test]
fn config_validate_fails_for_empty_scene_labels() {
    let path = write_temp_config(
        r#"{"version":1,"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"   ","candidate_count":0}]}}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid empty scene label for scene `raw`"));
}

#[test]
fn config_validate_fails_for_too_many_candidates() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":33}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("scene `raw` asks for 33 candidates"));
}

#[test]
fn config_validate_fails_for_unknown_scene_provider() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "llm": {
            "providers": [{"id":"known","base_url":"https://example.invalid/v1"}]
          },
          "scenes": {
            "active_scene": "rewrite",
            "definitions": [
              {"id":"raw","label":"Raw","candidate_count":0},
              {"id":"rewrite","label":"Rewrite","provider_id":"missing","candidate_count":1}
            ]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unknown LLM provider `missing`"));
}

#[test]
fn asr_state_reports_mock_provider_ready() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{"id":"mock","type":"local","model":"fixture-model"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .arg("asr-state")
        .arg("--config")
        .arg(&path)
        .output()
        .expect("run vinput asr-state");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "mock");
    assert_eq!(value["target_model_id"], "fixture-model");
    assert_eq!(value["effective_provider_id"], "mock");
    assert_eq!(value["has_effective_backend"], true);
}

#[test]
fn asr_state_reports_unavailable_provider() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "sherpa-onnx",
            "providers": [{"id":"sherpa-onnx","type":"local","model":"paraformer"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .arg("asr-state")
        .arg("--config")
        .arg(&path)
        .output()
        .expect("run vinput asr-state");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "sherpa-onnx");
    assert_eq!(value["target_model_id"], "paraformer");
    assert_eq!(value["has_effective_backend"], false);
    assert!(
        value["last_error"]
            .as_str()
            .unwrap_or_default()
            .contains("not implemented")
    );
}

#[test]
fn asr_state_reports_remote_provider_endpoint() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "remote",
            "providers": [{"id":"remote","type":"remote","model":"cloud","endpoint":"https://asr.example.test"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .arg("asr-state")
        .arg("--config")
        .arg(&path)
        .output()
        .expect("run vinput asr-state");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "remote");
    assert_eq!(value["target_model_id"], "cloud");
    assert_eq!(
        value["remote_endpoints"],
        serde_json::json!(["https://asr.example.test"])
    );
}

#[test]
fn asr_state_reports_command_provider_unavailable() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "providers": [{"id":"cmd","type":"command","command":"helper","args":["--json"]}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .arg("asr-state")
        .arg("--config")
        .arg(&path)
        .output()
        .expect("run vinput asr-state");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "cmd");
    assert_eq!(value["has_effective_backend"], false);
    assert!(
        value["last_error"]
            .as_str()
            .unwrap_or_default()
            .contains("not implemented")
    );
}

#[test]
fn config_validate_fails_for_command_provider_without_command() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "providers": [{"id":"cmd","type":"command"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("command ASR provider `cmd` must configure a command"));
}

#[test]
fn config_prints_bundled_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config"])
        .output()
        .expect("run vinput config");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("config summary should be JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["active_scene"], "__raw__");
    assert_eq!(value["active_provider"], "sherpa-onnx");
    assert!(value["registry_mirror_count"].as_u64().unwrap_or_default() > 0);
}

#[test]
fn config_validate_fails_for_remote_provider_without_endpoint() {
    let path = write_temp_config(
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "remote",
            "providers": [{"id":"remote","type":"remote"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["config", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput config validate");
    fs::remove_file(&path).expect("remove temporary config fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("remote ASR provider `remote` must configure an endpoint"));
}
