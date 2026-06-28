//! Daemon binary CLI integration tests.

use std::{
    ffi::OsStr,
    fs,
    path::PathBuf,
    process::{Command, Output},
};

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

fn default_config_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../data/default-config.json");
    assert!(path.exists(), "default config fixture should exist");
    path
}

fn daemon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vinput-daemon"))
}

fn run_daemon(args: &[&str], context: &str) -> Output {
    daemon_command()
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{context}: {error}"))
}

fn run_daemon_with_config(config_path: impl AsRef<OsStr>, args: &[&str], context: &str) -> Output {
    let mut command = daemon_command();
    command.arg("--config").arg(config_path).args(args);
    command
        .output()
        .unwrap_or_else(|error| panic!("{context}: {error}"))
}

fn assert_json_success(output: Output, context: &str) -> serde_json::Value {
    let stdout = assert_success_stdout(output, context);
    serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("{context}: stdout should be JSON: {error}; stdout: {stdout}")
    })
}

fn assert_success_stdout(output: Output, context: &str) -> String {
    let Output {
        status,
        stdout,
        stderr,
    } = output;
    assert!(
        status.success(),
        "{context}: command failed with status {:?}, stderr: {}",
        status.code(),
        String::from_utf8_lossy(&stderr)
    );
    String::from_utf8(stdout)
        .unwrap_or_else(|error| panic!("{context}: stdout should be UTF-8: {error}"))
}

fn assert_failure_stderr(output: Output, context: &str) -> String {
    let Output { status, stderr, .. } = output;
    assert!(
        !status.success(),
        "{context}: command unexpectedly succeeded"
    );
    String::from_utf8(stderr)
        .unwrap_or_else(|error| panic!("{context}: stderr should be UTF-8: {error}"))
}

#[test]
fn print_config_accepts_committed_default_fixture() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["print-config"],
            "run vinput-daemon print-config on default fixture",
        ),
        "print-config output",
    );
    assert_eq!(value["asr"]["active_provider"], "sherpa-onnx");
    assert_eq!(value["scenes"]["active_scene"], "__raw__");
}

#[test]
fn print_config_with_default_fixture_ignores_configured_backend_runtime_init() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["--configured-backends", "print-config"],
            "run vinput-daemon print-config with configured default fixture",
        ),
        "config",
    );
    assert_eq!(value["version"], 1);
    assert_eq!(value["asr"]["active_provider"], "sherpa-onnx");
}

#[test]
fn asr_state_accepts_committed_default_fixture() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["asr-state"],
            "run vinput-daemon asr-state on default fixture",
        ),
        "ASR state",
    );
    assert_eq!(value["target_provider_id"], "sherpa-onnx");
    assert_eq!(value["target_model_id"], "");
    assert_eq!(value["has_effective_backend"], false);
}

#[test]
fn asr_state_with_default_fixture_ignores_configured_backend_runtime_init() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["--configured-backends", "asr-state"],
            "run vinput-daemon asr-state with configured default fixture",
        ),
        "ASR state",
    );
    assert_eq!(value["target_provider_id"], "sherpa-onnx");
    assert_eq!(value["has_effective_backend"], false);
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

    let value = assert_json_success(
        run_daemon_with_config(&config.path, &["asr-state"], "run vinput-daemon asr-state"),
        "ASR state",
    );
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

    let value = assert_json_success(
        run_daemon_with_config(&config.path, &["asr-state"], "run vinput-daemon asr-state"),
        "ASR state",
    );
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

    let value = assert_json_success(
        run_daemon_with_config(&config.path, &["asr-state"], "run vinput-daemon asr-state"),
        "ASR state",
    );
    assert_eq!(value["target_provider_id"], "cmd");
    assert_eq!(value["target_model_id"], "cmd-model");
    assert_eq!(value["effective_provider_id"], "cmd");
    assert_eq!(value["effective_model_id"], "cmd-model");
    assert_eq!(value["has_effective_backend"], true);
}

#[test]
fn print_config_ignores_configured_backend_runtime_init() {
    let value = assert_json_success(
        run_daemon(
            &["--configured-backends", "print-config"],
            "run vinput-daemon --configured-backends print-config",
        ),
        "config",
    );
    assert_eq!(value["version"], 1);
    assert_eq!(value["asr"]["active_provider"], "sherpa-onnx");
}

#[test]
fn asr_state_ignores_configured_backend_runtime_init() {
    let value = assert_json_success(
        run_daemon(
            &["--configured-backends", "asr-state"],
            "run vinput-daemon --configured-backends asr-state",
        ),
        "ASR state",
    );
    assert_eq!(value["target_provider_id"], "sherpa-onnx");
    assert_eq!(value["has_effective_backend"], false);
}

#[test]
fn text_adapters_accepts_committed_default_fixture() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["text-adapters"],
            "run vinput-daemon text-adapters on default fixture",
        ),
        "text adapter state",
    );
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapter_ids"], serde_json::json!([]));
    assert!(value["single_adapter_id"].is_null());
}

#[test]
fn text_adapters_with_default_fixture_ignores_configured_backend_runtime_init() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["--configured-backends", "text-adapters"],
            "run vinput-daemon text-adapters with configured default fixture",
        ),
        "text adapter diagnostics",
    );
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapter_ids"], serde_json::json!([]));
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

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &["text-adapters"],
            "run vinput-daemon text-adapters",
        ),
        "text adapter diagnostics",
    );
    assert_eq!(value["adapter_count"], 1);
    assert_eq!(value["adapter_ids"], serde_json::json!(["cmd-adapter"]));
    assert_eq!(value["single_adapter_id"], "cmd-adapter");
}

#[test]
fn text_adapters_ignores_configured_backend_runtime_init() {
    let value = assert_json_success(
        run_daemon(
            &["--configured-backends", "text-adapters"],
            "run vinput-daemon --configured-backends text-adapters",
        ),
        "text adapter diagnostics",
    );
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapter_ids"], serde_json::json!([]));
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

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &["--configured-backends", "--once"],
            "run vinput-daemon --once with configured backends",
        ),
        "recognition payload",
    );
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

    let output = run_daemon_with_config(
        &config.path,
        &["--configured-backends", "--once"],
        "run vinput-daemon --once with ambiguous configured backends",
    );

    let stderr = assert_failure_stderr(
        output,
        "vinput-daemon --once with ambiguous configured backends",
    );
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

    let output = run_daemon_with_config(
        &config.path,
        &["--configured-backends", "--once"],
        "run vinput-daemon --once with missing configured text adapter",
    );

    let stderr = assert_failure_stderr(
        output,
        "vinput-daemon --once with missing configured text adapter",
    );
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

    let output = run_daemon_with_config(
        &config.path,
        &["text-adapters"],
        "run vinput-daemon text-adapters",
    );

    let stdout = assert_success_stdout(output, "text adapter summary");
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

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &["text-adapters"],
            "run vinput-daemon text-adapters without adapters",
        ),
        "text adapter summary",
    );
    assert_eq!(value["adapter_count"], 0);
    assert_eq!(value["adapter_ids"], serde_json::json!([]));
    assert!(value["single_adapter_id"].is_null());
    assert_eq!(value["adapters"], serde_json::json!([]));
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

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &["text-adapters"],
            "run vinput-daemon text-adapters with multiple adapters",
        ),
        "text adapter summary",
    );
    assert_eq!(value["adapter_count"], 2);
    assert_eq!(value["adapter_ids"], serde_json::json!(["first", "second"]));
    assert!(value["single_adapter_id"].is_null());
    assert_eq!(value["adapters"][0]["id"], "first");
    assert_eq!(value["adapters"][0]["command"], "first-helper");
    assert_eq!(value["adapters"][1]["id"], "second");
    assert_eq!(value["adapters"][1]["command"], "second-helper");
}

#[test]
fn help_lists_diagnostics_commands() {
    let output = run_daemon(&["--help"], "run vinput-daemon --help");

    let stdout = assert_success_stdout(output, "help output");
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--configured-backends"));
    assert!(stdout.contains("print-config"));
    assert!(stdout.contains("asr-state"));
    assert!(stdout.contains("configured ASR backend diagnostics"));
    assert!(stdout.contains("text-adapters"));
    assert!(stdout.contains("configured command text adapter diagnostics"));
}
