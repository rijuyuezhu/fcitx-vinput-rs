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
