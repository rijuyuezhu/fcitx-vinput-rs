//! Daemon binary CLI integration tests.

use std::{fs, path::PathBuf, process::Command};

struct TempConfig {
    path: PathBuf,
}

impl TempConfig {
    fn write(name: &str, contents: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vinput-daemon-test-{}-{unique}-{name}.json",
            std::process::id()
        ));
        fs::write(&path, contents).expect("write temporary daemon config");
        Self { path }
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
fn asr_state_uses_config_file() {
    let config = TempConfig::write(
        "asr-state",
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
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("asr-state")
        .output()
        .expect("run vinput-daemon asr-state");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "mock");
    assert_eq!(value["target_model_id"], "fixture");
    assert_eq!(value["has_effective_backend"], true);
}

#[test]
fn asr_state_preserves_remote_endpoint() {
    let config = TempConfig::write(
        "remote-asr-state",
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

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("asr-state")
        .output()
        .expect("run vinput-daemon asr-state");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "remote");
    assert_eq!(value["target_model_id"], "cloud");
    assert_eq!(value["has_effective_backend"], false);
    assert_eq!(
        value["remote_endpoints"],
        serde_json::json!(["https://asr.example.test"])
    );
}

#[test]
fn asr_state_preserves_command_provider_metadata() {
    let config = TempConfig::write(
        "command-asr-state",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "providers": [{"id":"cmd","type":"command","command":"helper","model":"cmd-model","args":["--json"],"hotwords_file":"/tmp/hotwords.txt"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("asr-state")
        .output()
        .expect("run vinput-daemon asr-state");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ASR state should be JSON");
    assert_eq!(value["target_provider_id"], "cmd");
    assert_eq!(value["target_model_id"], "cmd-model");
    assert_eq!(value["effective_provider_id"], "cmd");
    assert_eq!(value["effective_model_id"], "cmd-model");
    assert_eq!(value["has_effective_backend"], true);
}

#[test]
fn text_adapters_uses_config_file() {
    let config = TempConfig::write(
        "text-adapters",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{"id":"mock","type":"local","model":"fixture"}]
          },
          "llm": {
            "adapters": [{"id":"cmd-adapter","command":"vinput-postprocess","args":["--json"]}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("text-adapters")
        .output()
        .expect("run vinput-daemon text-adapters");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("text adapter diagnostics should be JSON");
    assert_eq!(value["adapter_count"], 1);
    assert_eq!(value["adapter_ids"], serde_json::json!(["cmd-adapter"]));
    assert_eq!(value["single_adapter_id"], "cmd-adapter");
}

#[test]
fn once_can_use_configured_backends() {
    let config = TempConfig::write(
        "configured-backends-once",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{"id":"mock","type":"local","model":"fixture"}]
          },
          "llm": {
            "adapters": [{
              "id":"cmd-adapter",
              "command":"python3",
              "args":["-c", "import sys; sys.stdin.read(); print('{\"text\":\"cli configured final\"}')"]
            }]
          },
          "scenes": {
            "active_scene": "needs-adapter",
            "definitions": [{"id":"needs-adapter","label":"Needs adapter","prompt":"polish","candidate_count":1}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("--configured-backends")
        .arg("--once")
        .output()
        .expect("run vinput-daemon --once with configured backends");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("recognition payload should be JSON");
    assert_eq!(value["commit_text"], "cli configured final");
}

#[test]
fn once_reports_ambiguous_configured_text_adapters() {
    let config = TempConfig::write(
        "ambiguous-configured-backends-once",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{"id":"mock","type":"local","model":"fixture"}]
          },
          "llm": {
            "adapters": [
              {"id":"first","command":"python3","args":["-c", "import sys; sys.stdin.read(); print('{\"text\":\"first\"}')"]},
              {"id":"second","command":"python3","args":["-c", "import sys; sys.stdin.read(); print('{\"text\":\"second\"}')"]}
            ]
          },
          "scenes": {
            "active_scene": "needs-adapter",
            "definitions": [{"id":"needs-adapter","label":"Needs adapter","prompt":"polish","candidate_count":1}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("--configured-backends")
        .arg("--once")
        .output()
        .expect("run vinput-daemon --once with ambiguous configured backends");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("ambiguous text adapter selection"));
}

#[test]
fn once_reports_missing_configured_text_adapter() {
    let config = TempConfig::write(
        "missing-configured-backends-once",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "mock",
            "providers": [{"id":"mock","type":"local","model":"fixture"}]
          },
          "scenes": {
            "active_scene": "needs-adapter",
            "definitions": [{"id":"needs-adapter","label":"Needs adapter","prompt":"polish","candidate_count":1}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("--configured-backends")
        .arg("--once")
        .output()
        .expect("run vinput-daemon --once with missing configured text adapter");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("requires a text adapter"));
}

#[test]
fn text_adapters_reports_configured_adapter_summary() {
    let config = TempConfig::write(
        "text-adapters",
        r#"
        {
          "version": 1,
          "llm": {
            "adapters": [{
              "id":"cmd-adapter",
              "command":"helper",
              "args":["--json"],
              "env":{"TOKEN":"secret"},
              "working_dir":"/tmp/adapter-work"
            }]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("text-adapters")
        .output()
        .expect("run vinput-daemon text-adapters");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(!stdout.contains("secret"));
    assert!(!stdout.contains("/tmp/adapter-work"));
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("text adapter summary should be JSON");
    assert_eq!(value["adapter_count"], 1);
    assert_eq!(value["adapter_ids"], serde_json::json!(["cmd-adapter"]));
    assert_eq!(value["single_adapter_id"], "cmd-adapter");
    assert_eq!(value["adapters"][0]["id"], "cmd-adapter");
    assert_eq!(value["adapters"][0]["kind"], "command");
    assert_eq!(value["adapters"][0]["command"], "helper");
    assert_eq!(value["adapters"][0]["args"], serde_json::json!(["--json"]));
    assert_eq!(value["adapters"][0]["env_count"], 1);
    assert_eq!(value["adapters"][0]["has_working_dir"], true);
}

#[test]
fn text_adapters_reports_empty_adapter_summary() {
    let config = TempConfig::write(
        "text-adapters-empty",
        r#"
        {
          "version": 1,
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("text-adapters")
        .output()
        .expect("run vinput-daemon text-adapters without adapters");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("text adapter summary should be JSON");
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapter_ids"], serde_json::json!([]));
    assert!(value["single_adapter_id"].is_null());
}

#[test]
fn text_adapters_reports_multiple_adapter_ids() {
    let config = TempConfig::write(
        "text-adapters-multiple",
        r#"
        {
          "version": 1,
          "llm": {
            "adapters": [
              {"id":"first","command":"first-helper"},
              {"id":"second","command":"second-helper"}
            ]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
        .arg("--config")
        .arg(&config.path)
        .arg("text-adapters")
        .output()
        .expect("run vinput-daemon text-adapters with multiple adapters");

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("text adapter summary should be JSON");
    assert_eq!(value["adapter_count"], 2);
    assert_eq!(value["adapter_ids"], serde_json::json!(["first", "second"]));
    assert!(value["single_adapter_id"].is_null());
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
    assert!(stdout.contains("--configured-backends"));
    assert!(stdout.contains("text-adapters"));
}
