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

struct TempBytes {
    path: PathBuf,
}

impl TempBytes {
    fn write(name: &str, extension: &str, contents: &[u8]) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "vinput-daemon-test-{}-{unique}-{name}.{extension}",
            std::process::id()
        ));
        fs::write(&path, contents).expect("write temporary daemon bytes");
        Self { path }
    }
}

impl Drop for TempBytes {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn pcm16le_bytes(samples: &[i16]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn wav_pcm16le_bytes(sample_rate_hz: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data = pcm16le_bytes(samples);
    let data_len = u32::try_from(data.len()).expect("test data should fit in u32");
    let byte_rate = sample_rate_hz * u32::from(channels) * 2;
    let block_align = channels * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate_hz.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&data);
    wav
}

fn default_config_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../data/default-config.json");
    assert!(path.exists(), "default config fixture should exist");
    path
}

fn e2e_demo_config_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../data/e2e-command-demo-config.json");
    assert!(path.exists(), "E2E demo config fixture should exist");
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

fn assert_audio_devices_backend_shape(value: &serde_json::Value) {
    assert_eq!(
        value["backend"],
        if cfg!(feature = "pipewire-backend") {
            "pipewire"
        } else {
            "unavailable"
        }
    );
    assert!(value["live"].is_boolean());
    if value["live"] == true {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    } else {
        assert_eq!(value["devices"].as_array().unwrap().len(), 0);
    }
    if cfg!(feature = "pipewire-backend") {
        assert!(value["enumeration_error"].is_null() || value["enumeration_error"].is_string());
    } else {
        assert_eq!(value["enumeration_error"], serde_json::Value::Null);
    }
}

#[derive(Debug)]
struct CapturedHttpRequest {
    head: String,
    body: String,
}

fn serve_single_http_response(
    response_body: String,
) -> (String, std::thread::JoinHandle<CapturedHttpRequest>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
            assert_ne!(read, 0, "HTTP client closed before headers were complete");
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
        let headers = head
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect::<std::collections::HashMap<_, _>>();
        let body = if headers
            .get("transfer-encoding")
            .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
        {
            while !buffer[header_end..]
                .windows(5)
                .any(|window| window == b"0\r\n\r\n")
            {
                let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
                assert_ne!(
                    read, 0,
                    "HTTP client closed before chunked body was complete"
                );
                buffer.extend_from_slice(&chunk[..read]);
            }
            decode_chunked_http_body(&buffer[header_end..])
        } else {
            let content_length = headers
                .get("content-length")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while buffer.len() < header_end + content_length {
                let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
                assert_ne!(read, 0, "HTTP client closed before body was complete");
                buffer.extend_from_slice(&chunk[..read]);
            }
            String::from_utf8_lossy(&buffer[header_end..header_end + content_length]).into_owned()
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        CapturedHttpRequest { head, body }
    });
    (base_url, handle)
}

fn decode_chunked_http_body(input: &[u8]) -> String {
    let mut position = 0;
    let mut decoded = Vec::new();
    while let Some(line_end) = input[position..]
        .windows(2)
        .position(|window| window == b"\r\n")
    {
        let line = String::from_utf8_lossy(&input[position..position + line_end]);
        let chunk_len = usize::from_str_radix(line.trim(), 16).unwrap();
        position += line_end + 2;
        if chunk_len == 0 {
            break;
        }
        decoded.extend_from_slice(&input[position..position + chunk_len]);
        position += chunk_len + 2;
    }
    String::from_utf8(decoded).unwrap()
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
    assert_eq!(value["ok"], true);
    assert_eq!(value["active_provider"], "sherpa-onnx");
    assert_eq!(value["active_scene"], "__raw__");
    assert_eq!(value["provider_count"], 1);
    assert_eq!(value["scene_count"], 2);
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
    assert_eq!(value["active_provider"], "sherpa-onnx");
}

#[test]
fn print_config_summary_omits_sensitive_config_details() {
    let config = TempConfig::write(
        "print-config-summary-redaction",
        r#"
        {
          "version": 1,
          "registry": {"base_urls": ["https://registry-leak-marker.example.invalid/index.json"]},
          "asr": {
            "active_provider": "cmd",
            "providers": [{
              "id":"cmd",
              "type":"command",
              "command":"vinput-asr-helper",
              "args":["--flag", "asr-arg-leak-marker"],
              "env":{"ASR_KEY":"asr-env-leak-marker"},
              "model":"asr-model-leak-marker",
              "hotwords_file":"/tmp/asr-hotwords-leak-marker.txt"
            }]
          },
          "llm": {
            "providers": [{
              "id":"llm",
              "base_url":"https://llm-leak-marker.example.invalid/v1",
              "api_key":"llm-key-leak-marker",
              "model":"llm-model-leak-marker",
              "extra_body":{"trace":"llm-extra-leak-marker"},
              "future_field":"provider-extra-leak-marker"
            }],
            "adapters": [{
              "id":"adapter",
              "command":"vinput-text-helper",
              "args":["--flag", "adapter-arg-leak-marker"],
              "env":{"ADAPTER_KEY":"adapter-env-leak-marker"},
              "working_dir":"/tmp/adapter-workdir-leak-marker",
              "adapter_field":"adapter-extra-leak-marker"
            }]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );

    let stdout = assert_success_stdout(
        run_daemon_with_config(
            &config.path,
            &["--configured-backends", "print-config"],
            "run vinput-daemon print-config",
        ),
        "print-config output",
    );
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(value["ok"], true);
    assert_eq!(value["active_provider"], "cmd");
    assert_eq!(value["active_scene"], "raw");
    assert_eq!(value["provider_count"], 1);
    assert_eq!(value["registry_mirror_count"], 1);

    for forbidden_key in [
        "api_key",
        "base_url",
        "env",
        "args",
        "command",
        "working_dir",
        "extra_body",
        "future_field",
        "adapter_field",
    ] {
        assert!(
            !stdout.contains(&format!("\"{forbidden_key}\"")),
            "print-config summary must not expose {forbidden_key}"
        );
    }
    for marker in [
        "registry-leak-marker",
        "asr-arg-leak-marker",
        "asr-env-leak-marker",
        "asr-model-leak-marker",
        "asr-hotwords-leak-marker",
        "llm-leak-marker",
        "llm-key-leak-marker",
        "llm-model-leak-marker",
        "llm-extra-leak-marker",
        "provider-extra-leak-marker",
        "adapter-arg-leak-marker",
        "adapter-env-leak-marker",
        "adapter-workdir-leak-marker",
        "adapter-extra-leak-marker",
    ] {
        assert!(
            !stdout.contains(marker),
            "print-config summary must not leak {marker}"
        );
    }
}

#[test]
fn audio_devices_reports_default_capture_target_and_unavailable_backend() {
    let value = assert_json_success(
        run_daemon_with_config(
            default_config_path(),
            &["audio-devices"],
            "run vinput-daemon audio-devices on default fixture",
        ),
        "audio devices",
    );
    assert_eq!(value["ok"], true);
    assert_eq!(value["capture_device"], "default");
    assert_eq!(value["capture_target"]["kind"], "default");
    assert_audio_devices_backend_shape(&value);
}

#[test]
fn audio_devices_preserves_configured_capture_target_object() {
    let config = TempConfig::write(
        "audio-devices",
        r#"
        {
          "version": 1,
          "global": {"capture_device": "alsa_input.usb-mic"},
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
        run_daemon_with_config(
            &config.path,
            &["audio-devices"],
            "run vinput-daemon audio-devices with object capture target",
        ),
        "audio devices",
    );
    assert_eq!(value["ok"], true);
    assert_eq!(value["capture_device"], "alsa_input.usb-mic");
    assert_eq!(value["capture_target"]["kind"], "object");
    assert_eq!(value["capture_target"]["value"], "alsa_input.usb-mic");
    assert_audio_devices_backend_shape(&value);
}

#[cfg(feature = "pipewire-backend")]
#[test]
fn audio_devices_reports_pipewire_enumeration_error_without_failing() {
    let config_dir = std::env::temp_dir().join(format!(
        "vinput-daemon-missing-pipewire-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    fs::create_dir(&config_dir).expect("create empty PipeWire config dir");

    let output = daemon_command()
        .arg("--config")
        .arg(default_config_path())
        .env("PIPEWIRE_CONFIG_DIR", &config_dir)
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_DATA_DIRS", &config_dir)
        .arg("audio-devices")
        .output()
        .expect("run vinput-daemon audio-devices without PipeWire client config");
    fs::remove_dir(&config_dir).expect("remove empty PipeWire config dir");

    let value = assert_json_success(output, "audio devices without PipeWire config");
    assert_eq!(value["ok"], true);
    assert_eq!(value["backend"], "pipewire");
    assert_eq!(value["live"], false);
    assert_eq!(value["devices"].as_array().unwrap().len(), 0);
    assert!(
        value["enumeration_error"]
            .as_str()
            .is_some_and(|message| message.contains("enumerate PipeWire audio sources"))
    );
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
    assert!(
        value["last_error"]
            .as_str()
            .unwrap_or_default()
            .contains("sherpa-onnx runtime")
    );
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
    assert!(
        value["last_error"]
            .as_str()
            .unwrap_or_default()
            .contains("sherpa-onnx runtime")
    );
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
    assert_eq!(value["active_provider"], "sherpa-onnx");
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
fn once_can_use_configured_openai_provider_over_http() {
    let response_body = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({"candidates": ["cli http final"]}).to_string()
            }
        }]
    })
    .to_string();
    let (base_url, handle) = serve_single_http_response(response_body);
    let config_json = serde_json::json!({
        "version": 1,
        "asr": {
            "active_provider": "mock",
            "providers": [{"id": "mock", "type": "local", "model": "fixture"}]
        },
        "llm": {
            "providers": [{
                "id": "openai",
                "base_url": base_url,
                "api_key": "secret-token",
                "model": "provider-model"
            }]
        },
        "scenes": {
            "active_scene": "needs-provider",
            "definitions": [{
                "id": "needs-provider",
                "label": "Needs provider",
                "prompt": "Polish: {{ asr }}",
                "provider_id": "openai",
                "model": "scene-model",
                "candidate_count": 1,
                "timeout_ms": 2000
            }]
        }
    });
    let config = TempConfig::write("configured-openai-once", &config_json.to_string());

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &["--configured-backends", "--once"],
            "run vinput-daemon --once with configured OpenAI-compatible provider",
        ),
        "recognition payload",
    );

    assert_eq!(value["commit_text"], "cli http final");
    let captured = handle.join().unwrap();
    assert!(captured.head.starts_with("POST /chat/completions HTTP/1.1"));
    assert!(
        captured
            .head
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-token")
    );
    let posted: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
    assert_eq!(posted["model"], "scene-model");
    let content = posted["messages"][0]["content"].as_str().unwrap();
    assert!(content.contains("mock recognition result"));
}

#[test]
fn once_can_read_pcm_file_into_configured_command_pipeline() {
    let pcm = TempBytes::write(
        "configured-command-pcm-once",
        "pcm",
        &pcm16le_bytes(&[1_000, -1_000, 2_000, -2_000]),
    );
    let config = TempConfig::write(
        "configured-command-pcm-once",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "normalize_audio": false,
            "input_gain": 1.0,
            "providers": [{
              "id":"cmd",
              "type":"command",
              "command":"python3",
              "args":["-c", "import sys; data=sys.stdin.buffer.read(); print('pcm bytes: %d' % len(data))"]
            }]
          },
          "llm": {
            "adapters": [{
              "id":"cmd-adapter",
              "command":"python3",
              "args":["-c", "import json,sys; req=json.load(sys.stdin); print(json.dumps({'text':'adapted '+req['raw_text']}))"]
            }]
          },
          "scenes": {
            "active_scene": "needs-adapter",
            "definitions": [{"id":"needs-adapter","label":"Needs adapter","prompt":"polish","candidate_count":1}]
          }
        }
        "#,
    );
    let pcm_path = pcm.path.to_string_lossy().into_owned();

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &[
                "--configured-backends",
                "--once",
                "--pcm16le",
                pcm_path.as_str(),
            ],
            "run vinput-daemon --once with PCM and configured command pipeline",
        ),
        "recognition payload",
    );
    assert_eq!(value["commit_text"], "adapted pcm bytes: 8");
}

#[test]
fn once_can_read_wav_file_into_configured_command_asr() {
    let wav = TempBytes::write(
        "configured-command-wav-once",
        "wav",
        &wav_pcm16le_bytes(16_000, 1, &[1_000, -1_000, 2_000, -2_000]),
    );
    let config = TempConfig::write(
        "configured-command-wav-once",
        r#"
        {
          "version": 1,
          "asr": {
            "active_provider": "cmd",
            "normalize_audio": false,
            "input_gain": 1.0,
            "providers": [{"id":"cmd","type":"command","command":"wc","args":["-c"]}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }
        "#,
    );
    let wav_path = wav.path.to_string_lossy().into_owned();

    let value = assert_json_success(
        run_daemon_with_config(
            &config.path,
            &[
                "--configured-backends",
                "--once",
                "--wav",
                wav_path.as_str(),
            ],
            "run vinput-daemon --once with WAV and configured command ASR",
        ),
        "recognition payload",
    );
    assert_eq!(value["commit_text"], "8");
}

#[test]
fn once_runs_committed_e2e_demo_config_with_wav() {
    let wav = TempBytes::write(
        "committed-e2e-demo",
        "wav",
        &wav_pcm16le_bytes(16_000, 1, &[1_000, -1_000, 2_000, -2_000]),
    );
    let wav_path = wav.path.to_string_lossy().into_owned();

    let value = assert_json_success(
        run_daemon_with_config(
            e2e_demo_config_path(),
            &[
                "--configured-backends",
                "--once",
                "--wav",
                wav_path.as_str(),
            ],
            "run committed E2E demo config with WAV input",
        ),
        "recognition payload",
    );
    assert_eq!(value["commit_text"], "demo final: demo heard 8 bytes");
}
#[test]
fn once_rejects_odd_pcm_file() {
    let pcm = TempBytes::write("odd-pcm", "pcm", &[0]);
    let pcm_path = pcm.path.to_string_lossy().into_owned();

    let output = run_daemon(
        &["--once", "--pcm16le", pcm_path.as_str()],
        "run vinput-daemon --once with odd PCM file",
    );
    let stderr = assert_failure_stderr(output, "vinput-daemon --once with odd PCM file");
    assert!(stderr.contains("odd number of bytes"));
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
    assert!(!stdout.contains("TOKEN"));
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
    assert_eq!(value["adapters"][0]["is_running"], false);
    assert!(value["adapters"][0]["pid"].is_null());
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
    assert!(stdout.contains("--pcm16le"));
    assert!(stdout.contains("--wav"));
    assert!(stdout.contains("--pcm-sample-rate"));
    assert!(stdout.contains("--pcm-channels"));
    assert!(stdout.contains("print-config"));
    assert!(stdout.contains("asr-state"));
    assert!(stdout.contains("configured ASR backend diagnostics"));
    assert!(stdout.contains("text-adapters"));
    assert!(stdout.contains("audio-devices"));
    assert!(stdout.contains("configured command text adapter diagnostics"));
}
