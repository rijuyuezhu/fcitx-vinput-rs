//! Daemon binary CLI integration tests.

use std::{fs, process::Command};

#[test]
fn asr_state_uses_config_file() {
    let path = std::env::temp_dir().join(format!(
        "vinput-daemon-test-{}-{}.json",
        std::process::id(),
        "asr-state"
    ));
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
