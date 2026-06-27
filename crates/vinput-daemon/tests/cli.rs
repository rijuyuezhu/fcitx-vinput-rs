//! Daemon binary CLI integration tests.

use std::{fs, path::PathBuf, process::Command};

fn temp_config_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "vinput-daemon-test-{}-{name}.json",
        std::process::id()
    ))
}

#[test]
fn asr_state_uses_config_file() {
    let path = temp_config_path("asr-state");
    fs::write(
        &path,
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{"id":"mock","type":"local","model":"fixture"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    )
    .expect("write temporary daemon config");

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&path)
        .arg("asr-state")
        .output()
        .expect("run vinput-daemon asr-state");
    fs::remove_file(&path).expect("remove temporary daemon config");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "mock");
    assert_eq!(value["target_model_id"], "fixture");
    assert_eq!(value["has_effective_backend"], true);
}

#[test]
fn asr_state_preserves_remote_endpoint() {
    let path = temp_config_path("remote-asr-state");
    fs::write(
        &path,
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
    )
    .expect("write temporary daemon config");

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&path)
        .arg("asr-state")
        .output()
        .expect("run vinput-daemon asr-state");
    fs::remove_file(&path).expect("remove temporary daemon config");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "remote");
    assert_eq!(
        value["remote_endpoints"],
        serde_json::json!(["https://asr.example.test"])
    );
}

#[test]
fn help_lists_config_option() {
    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--help")
        .output()
        .expect("run vinput-daemon --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
    assert!(stdout.contains("--config"));
}
