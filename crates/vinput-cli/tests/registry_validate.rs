//! Integration tests for registry validation CLI paths.

use std::{fs, process::Command};

fn write_temp_registry(contents: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "vinput-registry-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::write(&path, contents).expect("write temporary registry fixture");
    path
}

#[test]
fn registry_validate_prints_summary_for_valid_index() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {
              "id": "sherpa-zh-small",
              "label": "Sherpa zh small",
              "provider": "sherpa-onnx",
              "assets": [
                {
                  "path": "models/sherpa-zh-small.tar.zst",
                  "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                }
              ]
            }
          ],
          "adapters": [
            {
              "id": "mock-adapter",
              "label": "Mock adapter",
              "kind": "command",
              "assets": []
            }
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry summary should be JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["model_count"], 1);
    assert_eq!(value["adapter_count"], 1);
    assert_eq!(value["asset_count"], 1);
}

#[test]
fn registry_validate_fails_for_unsafe_asset_path() {
    let path = write_temp_registry(
        r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"../bad"}]}]}"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unsafe asset path `../bad`"));
}

#[test]
fn registry_validate_fails_for_duplicate_model_ids() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[]},
            {"id":"m","label":"M again","provider":"p","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("duplicate model id `m`"));
}

#[test]
fn registry_validate_fails_for_duplicate_asset_paths() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {
              "id":"m",
              "label":"M",
              "provider":"p",
              "assets":[{"path":"m.tar"},{"path":"m.tar"}]
            }
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("duplicate asset path `m.tar`"));
}

#[test]
fn registry_plan_prints_assets_with_resolved_urls() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {
              "id": "m",
              "label": "M",
              "provider": "p",
              "assets": [{"path":"models/m.tar","size_bytes":5}]
            }
          ],
          "adapters": [
            {
              "id":"a",
              "label":"A",
              "kind":"command",
              "assets":[{"path":"adapters/a.tar"}]
            }
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry plan should be JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["asset_count"], 2);
    assert_eq!(value["known_size_bytes"], 5);
    assert_eq!(value["unknown_size_count"], 1);
    assert_eq!(value["assets"][0]["entry_kind"], "model");
    assert_eq!(value["assets"][0]["entry_id"], "m");
    assert_eq!(value["assets"][0]["path"], "models/m.tar");
    assert!(
        value["assets"][0]["urls"][0]
            .as_str()
            .expect("planned URL should be a string")
            .ends_with("/models/m.tar")
    );
    assert_eq!(value["assets"][1]["entry_kind"], "adapter");
    assert_eq!(value["assets"][1]["entry_id"], "a");
}

#[test]
fn registry_plan_uses_custom_config_mirrors() {
    let registry_path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {
              "id": "m",
              "label": "M",
              "provider": "p",
              "assets": [{"path":"models/m.tar"}]
            }
          ]
        }
        "#,
    );
    let mut config_path = std::env::temp_dir();
    config_path.push(format!(
        "vinput-plan-config-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::write(
        &config_path,
        r#"
        {
          "version": 1,
          "registry": {"base_urls": ["https://custom.invalid/root"]},
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
    )
    .expect("write temporary config fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&registry_path)
        .args(["--config"])
        .arg(&config_path)
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&registry_path).expect("remove temporary registry fixture");
    fs::remove_file(&config_path).expect("remove temporary config fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry plan should be JSON");
    assert_eq!(
        value["assets"][0]["urls"][0],
        "https://custom.invalid/root/models/m.tar"
    );
}

#[test]
fn registry_plan_can_select_one_model() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}
          ],
          "adapters": [
            {"id":"a","label":"A","kind":"command","assets":[{"path":"adapters/a.tar"}]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--model", "m"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry plan should be JSON");
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["assets"][0]["entry_kind"], "model");
    assert_eq!(value["assets"][0]["entry_id"], "m");
}

#[test]
fn registry_plan_fails_for_unknown_model() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--model", "missing"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unknown model id `missing`"));
}

#[test]
fn registry_plan_can_select_one_adapter() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}
          ],
          "adapters": [
            {"id":"a","label":"A","kind":"command","assets":[{"path":"adapters/a.tar"}]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--adapter", "a"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry plan should be JSON");
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["assets"][0]["entry_kind"], "adapter");
    assert_eq!(value["assets"][0]["entry_id"], "a");
}

#[test]
fn registry_plan_rejects_model_and_adapter_together() {
    let path = write_temp_registry(r#"{"version":1}"#);

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--model", "m", "--adapter", "a"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("cannot be used with"));
}

#[test]
fn registry_plan_fails_for_unknown_adapter() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "adapters": [
            {"id":"a","label":"A","kind":"command","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--adapter", "missing"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unknown adapter id `missing`"));
}

#[test]
fn registry_plan_summary_only_omits_assets() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar","size_bytes":9}]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--summary-only"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry plan should be JSON");
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["known_size_bytes"], 9);
    assert_eq!(value["unknown_size_count"], 0);
    assert!(value.get("assets").is_none());
}

#[test]
fn registry_validate_fails_for_empty_model_ids() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"  ","label":"M","provider":"p","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("registry id must not be empty"));
}

#[test]
fn registry_validate_fails_for_empty_adapter_ids() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "adapters": [
            {"id":"","label":"A","kind":"command","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("registry id must not be empty"));
}

#[test]
fn registry_validate_fails_for_empty_model_provider() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"   ","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("model `m` has an empty provider"));
}

#[test]
fn registry_validate_fails_for_empty_adapter_kind() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "adapters": [
            {"id":"a","label":"A","kind":"","assets":[]}
          ]
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("adapter `a` has an empty kind"));
}

#[test]
fn registry_prints_bundled_mirror_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_vinput"))
        .args(["registry"])
        .output()
        .expect("run vinput registry");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry summary should be JSON");
    assert!(value["base_url_count"].as_u64().unwrap_or_default() > 0);
    assert!(
        value["index_urls"]
            .as_array()
            .is_some_and(|urls| !urls.is_empty())
    );
}
