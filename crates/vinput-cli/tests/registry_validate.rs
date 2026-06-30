//! Integration tests for registry validation CLI paths.

mod common;

use std::fs;

use common::{assert_json_success, vinput_command, workspace_file, write_temp_json};
use vinput_registry::RegistryIndex;

fn write_temp_registry(contents: &str) -> std::path::PathBuf {
    write_temp_json("vinput-registry", contents)
}
fn write_temp_config(contents: &str) -> std::path::PathBuf {
    write_temp_json("vinput-plan-config", contents)
}
fn sample_registry_path() -> std::path::PathBuf {
    let path = workspace_file("data/sample-registry-index.json");
    assert!(path.exists(), "sample registry fixture should exist");
    path
}
#[test]
fn registry_sample_fixture_preserves_contract_ids() {
    let path = sample_registry_path();
    let contents = fs::read_to_string(&path).expect("read sample registry fixture");
    let index = RegistryIndex::from_json_str(&contents).expect("sample registry should be valid");

    assert_eq!(index.version, 1);
    assert_eq!(index.models.len(), 1);
    assert_eq!(index.adapters.len(), 1);

    let model = index.model("sherpa-zh-small").expect("sample model id");
    assert_eq!(model.provider, "sherpa-onnx");
    assert_eq!(model.assets.len(), 1);
    assert_eq!(model.assets[0].path, "models/sherpa-zh-small.tar.zst");
    assert_eq!(
        model.assets[0].sha256.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );

    let adapter = index.adapter("mock-adapter").expect("sample adapter id");
    assert_eq!(adapter.kind, "command");
    assert!(adapter.assets.is_empty());
}

#[test]
fn registry_validate_accepts_committed_sample_fixture() {
    let path = sample_registry_path();

    let output = vinput_command()
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate on sample fixture");

    let value = assert_json_success(output, "registry summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["model_count"], 1);
    assert_eq!(value["adapter_count"], 1);
    assert_eq!(value["asset_count"], 1);
}

#[test]
fn registry_plan_accepts_committed_sample_fixture() {
    let path = sample_registry_path();

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&path)
        .arg("--summary-only")
        .output()
        .expect("run vinput registry plan on sample fixture");

    let value = assert_json_success(output, "registry plan summary");
    assert_eq!(value["ok"], true);
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["unknown_size_count"], 1);
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

    let output = vinput_command()
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry summary");
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

    let output = vinput_command()
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

    let output = vinput_command()
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

    let output = vinput_command()
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
fn registry_validate_fails_for_empty_asset_paths() {
    let path = write_temp_registry(
        r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"   "}]}]}"#,
    );

    let output = vinput_command()
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("asset path must not be empty"));
}

#[test]
fn registry_validate_fails_for_invalid_checksum() {
    let path = write_temp_registry(
        r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"m.tar","sha256":"ABC"}]}]}"#,
    );

    let output = vinput_command()
        .args(["registry", "validate"])
        .arg(&path)
        .output()
        .expect("run vinput registry validate");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("invalid sha256 checksum `ABC`"));
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

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&path)
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry plan");
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
fn registry_install_plan_prints_targets_and_checksum_policy() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {
              "id": "m",
              "label": "M",
              "provider": "p",
              "assets": [
                {
                  "path":"models/m.tar",
                  "sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                  "size_bytes":5
                }
              ]
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

    let output = vinput_command()
        .args(["registry", "install-plan"])
        .arg(&path)
        .args(["--target-root", "/tmp/vinput-assets"])
        .output()
        .expect("run vinput registry install-plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry install plan");
    assert_eq!(value["ok"], true);
    assert_eq!(value["target_root"], "/tmp/vinput-assets");
    assert_eq!(value["asset_count"], 2);
    assert_eq!(value["known_size_bytes"], 5);
    assert_eq!(value["missing_checksum_count"], 1);
    assert_eq!(value["assets"][0]["source_path"], "models/m.tar");
    assert_eq!(
        value["assets"][0]["target_path"],
        "/tmp/vinput-assets/models/m.tar"
    );
    assert_eq!(value["assets"][0]["checksum_policy"], "sha256");
    assert_eq!(value["assets"][1]["checksum_policy"], "missing");
}

#[test]
fn registry_install_plan_preserves_filesystem_root_target() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}
          ]
        }
        "#,
    );

    let output = vinput_command()
        .args(["registry", "install-plan"])
        .arg(&path)
        .args(["--target-root", "/"])
        .output()
        .expect("run vinput registry install-plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry install plan");
    assert_eq!(value["ok"], true);
    assert_eq!(value["target_root"], "/");
    assert_eq!(value["assets"][0]["target_path"], "/models/m.tar");
}

#[test]
fn registry_install_plan_summary_only_can_select_one_model() {
    let path = write_temp_registry(
        r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar","size_bytes":5}]},
            {"id":"other","label":"Other","provider":"p","assets":[{"path":"models/other.tar","size_bytes":7}]}
          ],
          "adapters": [
            {"id":"a","label":"A","kind":"command","assets":[{"path":"adapters/a.tar"}]}
          ]
        }
        "#,
    );

    let output = vinput_command()
        .args(["registry", "install-plan"])
        .arg(&path)
        .args(["--target-root", "/tmp/vinput-assets"])
        .args(["--model", "m", "--summary-only"])
        .output()
        .expect("run vinput registry install-plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry install plan");
    assert_eq!(value["ok"], true);
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["known_size_bytes"], 5);
    assert_eq!(value["missing_checksum_count"], 1);
    assert!(value.get("assets").is_none());
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
    let config_path = write_temp_config(
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
    );

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&registry_path)
        .args(["--config"])
        .arg(&config_path)
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&registry_path).expect("remove temporary registry fixture");
    fs::remove_file(&config_path).expect("remove temporary config fixture");

    let value = assert_json_success(output, "registry plan");
    assert_eq!(
        value["assets"][0]["urls"][0],
        "https://custom.invalid/root/models/m.tar"
    );
}

#[test]
fn registry_install_plan_rejects_invalid_config_mirrors() {
    let registry_path = write_temp_registry(
        r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}]}"#,
    );
    let config_path = write_temp_config(
        r#"{"version":1,"registry":{"base_urls":[""]},"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = vinput_command()
        .args(["registry", "install-plan"])
        .arg(&registry_path)
        .args(["--target-root", "/tmp/vinput-assets", "--config"])
        .arg(&config_path)
        .output()
        .expect("run vinput registry install-plan");
    fs::remove_file(&registry_path).expect("remove temporary registry fixture");
    fs::remove_file(&config_path).expect("remove temporary config fixture");

    assert!(!output.status.success());
}

#[test]
fn registry_install_plan_uses_custom_config_mirrors() {
    let registry_path = write_temp_registry(
        r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}]}"#,
    );
    let config_path = write_temp_config(
        r#"{"version":1,"registry":{"base_urls":["mirror"]},"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = vinput_command()
        .args(["registry", "install-plan"])
        .arg(&registry_path)
        .args(["--target-root", "/tmp/vinput-assets"])
        .args(["--config"])
        .arg(&config_path)
        .output()
        .expect("run vinput registry install-plan");
    fs::remove_file(&registry_path).expect("remove temporary registry fixture");
    fs::remove_file(&config_path).expect("remove temporary config fixture");

    let value = assert_json_success(output, "registry install plan");
    assert_eq!(value["assets"][0]["urls"][0], "mirror/models/m.tar");
}

#[test]
fn registry_plan_rejects_invalid_config_mirrors() {
    let registry_path = write_temp_registry(
        r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}]}"#,
    );
    let config_path = write_temp_config(
        r#"{"version":1,"registry":{"base_urls":[""]},"asr":{"active_provider":"p","providers":[{"id":"p","type":"local"}]},"scenes":{"active_scene":"raw","definitions":[{"id":"raw","label":"Raw","candidate_count":0}]}}"#,
    );

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&registry_path)
        .args(["--config"])
        .arg(&config_path)
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&registry_path).expect("remove temporary registry fixture");
    fs::remove_file(&config_path).expect("remove temporary config fixture");

    assert!(!output.status.success());
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

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--model", "m"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry plan");
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

    let output = vinput_command()
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

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--adapter", "a"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry plan");
    assert_eq!(value["asset_count"], 1);
    assert_eq!(value["assets"][0]["entry_kind"], "adapter");
    assert_eq!(value["assets"][0]["entry_id"], "a");
}

#[test]
fn registry_plan_rejects_model_and_adapter_together() {
    let path = write_temp_registry(r#"{"version":1}"#);

    let output = vinput_command()
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

    let output = vinput_command()
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
fn registry_install_plan_fails_for_unknown_adapter() {
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

    let output = vinput_command()
        .args(["registry", "install-plan"])
        .arg(&path)
        .args(["--target-root", "/tmp/vinput-assets"])
        .args(["--adapter", "missing"])
        .output()
        .expect("run vinput registry install-plan");
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

    let output = vinput_command()
        .args(["registry", "plan"])
        .arg(&path)
        .args(["--summary-only"])
        .output()
        .expect("run vinput registry plan");
    fs::remove_file(&path).expect("remove temporary registry fixture");

    let value = assert_json_success(output, "registry plan");
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

    let output = vinput_command()
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

    let output = vinput_command()
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

    let output = vinput_command()
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

    let output = vinput_command()
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
    let output = vinput_command()
        .args(["registry"])
        .output()
        .expect("run vinput registry");

    let value = assert_json_success(output, "registry summary");
    assert!(value["base_url_count"].as_u64().unwrap_or_default() > 0);
    assert!(
        value["index_urls"]
            .as_array()
            .is_some_and(|urls| !urls.is_empty())
    );
}
