//! Integration tests for live registry model list CLI paths.

mod common;

use common::{
    assert_json_success, assert_stdout_success, vinpst_command, workspace_file, write_temp_json,
};

fn live_models_fixture() -> std::path::PathBuf {
    let path = workspace_file("crates/vinpst-registry/tests/fixtures/live-models-sensevoice.json");
    assert!(path.exists(), "live model registry fixture should exist");
    path
}

fn live_i18n_fixture() -> std::path::PathBuf {
    let path = workspace_file("crates/vinpst-registry/tests/fixtures/live-i18n-zh-cn.json");
    assert!(path.exists(), "live registry i18n fixture should exist");
    path
}

#[test]
fn model_list_json_accepts_live_sensevoice_fixture() {
    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--json")
        .output()
        .expect("run vinpst model list --json");

    let value = assert_json_success(output, "model list json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"]["kind"], "file");
    assert_eq!(value["source"]["mirror_count"], 3);
    assert_eq!(value["i18n"]["kind"], "file");
    assert_eq!(value["model_count"], 1);

    let model = &value["models"][0];
    assert_eq!(
        model["id"],
        "model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"
    );
    assert_eq!(model["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(model["language"], "zh");
    assert_eq!(model["size_bytes"], 165_675_008);
    assert_eq!(model["backend"], "sherpa-offline");
    assert_eq!(model["family"], "sense_voice");
    assert_eq!(model["runtime"], "offline");
    assert_eq!(model["supported"], true);
    assert_eq!(model["support"], "supported");
    assert_eq!(model["title"], "SenseVoice 五语");
    assert_eq!(
        model["description"],
        "SenseVoice 多语言模型，支持中文、英文、日语、韩语和粤语。"
    );
    assert_eq!(model["url_count"], 3);
    assert_eq!(
        model["sha256"],
        "7305f7905bfcf77fa0b39388a313f3da35c68d971661a65475b56fb2162c8e63"
    );
}

#[test]
fn model_list_json_reports_qwen3_asr_as_supported() {
    let registry_path = write_temp_json(
        "live-model-qwen3-support",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.sherpa-onnx.qwen3-asr-0.6b-int8",
                    "short_id": "onnx-qwen3-0.6b-int8-off",
                    "urls": ["https://example.invalid/qwen3.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "qwen3_asr",
                        "runtime": "offline",
                        "model": {
                            "qwen3_asr": {
                                "conv_frontend": "conv_frontend.onnx",
                                "encoder": "encoder.int8.onnx",
                                "decoder": "decoder.int8.onnx",
                                "tokenizer": "tokenizer"
                            }
                        }
                    }
                }
            ]
        })
        .to_string(),
    );

    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst model list qwen3 --json");

    let value = assert_json_success(output, "model list qwen3 json");
    assert_eq!(value["models"][0]["family"], "qwen3_asr");
    assert_eq!(value["models"][0]["runtime"], "offline");
    assert_eq!(value["models"][0]["supported"], true);
    assert_eq!(value["models"][0]["support"], "supported");

    std::fs::remove_file(registry_path).ok();
}

#[test]
fn model_list_json_reports_offline_transducer_as_supported() {
    let registry_path = write_temp_json(
        "live-model-offline-transducer-support",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.sherpa-onnx.zipformer-multi-zh-hans",
                    "short_id": "onnx-zf-zh-multi-int8-off",
                    "urls": ["https://example.invalid/transducer.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "transducer",
                        "runtime": "offline",
                        "model": {
                            "tokens": "tokens.txt",
                            "transducer": {
                                "encoder": "encoder.onnx",
                                "decoder": "decoder.onnx",
                                "joiner": "joiner.onnx"
                            }
                        }
                    }
                }
            ]
        })
        .to_string(),
    );

    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst model list offline transducer --json");

    let value = assert_json_success(output, "model list offline transducer json");
    assert_eq!(value["models"][0]["family"], "transducer");
    assert_eq!(value["models"][0]["runtime"], "offline");
    assert_eq!(value["models"][0]["supported"], true);
    assert_eq!(value["models"][0]["support"], "supported");

    std::fs::remove_file(registry_path).ok();
}

#[test]
fn model_list_json_reports_dolphin_as_supported() {
    let registry_path = write_temp_json(
        "live-model-dolphin-support",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.sherpa-onnx.dolphin-base-ctc-multi-lang-int8",
                    "short_id": "onnx-dolphin-multi-int8-off",
                    "urls": ["https://example.invalid/dolphin.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "dolphin",
                        "runtime": "offline",
                        "model": {
                            "tokens": "tokens.txt",
                            "dolphin": {"model": "model.int8.onnx"}
                        }
                    }
                }
            ]
        })
        .to_string(),
    );

    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst model list dolphin --json");

    let value = assert_json_success(output, "model list dolphin json");
    assert_eq!(value["models"][0]["family"], "dolphin");
    assert_eq!(value["models"][0]["runtime"], "offline");
    assert_eq!(value["models"][0]["supported"], true);
    assert_eq!(value["models"][0]["support"], "supported");

    std::fs::remove_file(registry_path).ok();
}

#[test]
fn model_list_json_reports_paraformer_as_supported() {
    let registry_path = write_temp_json(
        "live-model-paraformer-support",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.sherpa-onnx.paraformer-zh-small",
                    "short_id": "onnx-pf-zh-sm-off",
                    "urls": ["https://example.invalid/paraformer.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "paraformer",
                        "runtime": "offline",
                        "model": {
                            "tokens": "tokens.txt",
                            "paraformer": {"model": "model.int8.onnx"}
                        }
                    }
                }
            ]
        })
        .to_string(),
    );

    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst model list paraformer --json");

    let value = assert_json_success(output, "model list paraformer json");
    assert_eq!(value["models"][0]["family"], "paraformer");
    assert_eq!(value["models"][0]["runtime"], "offline");
    assert_eq!(value["models"][0]["supported"], true);
    assert_eq!(value["models"][0]["support"], "supported");

    std::fs::remove_file(registry_path).ok();
}

#[test]
fn model_list_json_reports_moonshine_as_supported() {
    let registry_path = write_temp_json(
        "live-model-moonshine-support",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.sherpa-onnx.moonshine-tiny-en-int8",
                    "short_id": "onnx-ms-tiny-en-int8-off",
                    "urls": ["https://example.invalid/moonshine.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "moonshine",
                        "model_type": "moonshine_v1",
                        "runtime": "offline",
                        "model": {
                            "moonshine": {
                                "preprocessor": "preprocess.onnx",
                                "encoder": "encode.int8.onnx",
                                "uncached_decoder": "uncached_decode.int8.onnx",
                                "cached_decoder": "cached_decode.int8.onnx"
                            },
                            "tokens": "tokens.txt"
                        }
                    }
                }
            ]
        })
        .to_string(),
    );

    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst model list moonshine --json");

    let value = assert_json_success(output, "model list moonshine json");
    assert_eq!(value["models"][0]["family"], "moonshine");
    assert_eq!(value["models"][0]["runtime"], "offline");
    assert_eq!(value["models"][0]["supported"], true);
    assert_eq!(value["models"][0]["support"], "supported");

    std::fs::remove_file(registry_path).ok();
}

#[test]
fn model_list_json_reports_native_streaming_families_as_supported() {
    let registry_path = write_temp_json(
        "live-model-streaming-support",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.sherpa-onnx.streaming-transducer",
                    "short_id": "online-transducer",
                    "urls": ["https://example.invalid/transducer.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-streaming",
                        "family": "transducer",
                        "runtime": "online"
                    }
                },
                {
                    "id": "model.sherpa-onnx.streaming-zipformer2-ctc",
                    "short_id": "online-zipformer2-ctc",
                    "urls": ["https://example.invalid/zipformer2.tar.bz2"],
                    "vinpst_model": {
                        "backend": "sherpa-streaming",
                        "family": "zipformer2_ctc",
                        "runtime": "online"
                    }
                }
            ]
        })
        .to_string(),
    );

    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(&registry_path)
        .arg("--json")
        .output()
        .expect("run vinpst model list streaming --json");

    let value = assert_json_success(output, "model list streaming json");
    assert_eq!(value["model_count"], 2);
    for model in value["models"].as_array().unwrap() {
        assert_eq!(model["backend"], "sherpa-streaming");
        assert_eq!(model["runtime"], "online");
        assert_eq!(model["supported"], true);
        assert_eq!(model["support"], "supported");
    }

    std::fs::remove_file(registry_path).ok();
}

#[test]
fn model_list_text_prints_source_columns_and_support_marker() {
    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .output()
        .expect("run vinpst model list");

    let stdout = assert_stdout_success(output, "model list text");
    assert!(stdout.contains("ID\tTITLE\tLANGUAGE\tSIZE\tTYPE\tHOTWORDS\tSTATUS"));
    assert!(stdout.contains("model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8\tSenseVoice 五语"));
    assert!(stdout.contains("sense_voice"));
    assert!(stdout.contains("available"));
    for internal in [
        "registry_source:",
        "i18n_source:",
        "short_id",
        "backend",
        "support:",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
}

#[test]
fn model_list_text_falls_back_to_short_id_without_i18n() {
    let output = vinpst_command()
        .args(["model", "list", "--registry"])
        .arg(live_models_fixture())
        .output()
        .expect("run vinpst model list without i18n");

    let stdout = assert_stdout_success(output, "model list text without i18n");
    assert!(stdout.contains("onnx-sv-zh-int8-off"));
    assert!(!stdout.contains("i18n_source:"));
    assert!(!stdout.contains("SenseVoice 五语"));
}

#[test]
fn model_ls_available_alias_matches_live_registry_list() {
    let output = vinpst_command()
        .args(["model", "ls", "--available", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--json")
        .output()
        .expect("run vinpst model ls --available --json");

    let value = assert_json_success(output, "model ls available json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["model_count"], 1);
    assert_eq!(value["models"][0]["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(value["models"][0]["title"], "SenseVoice 五语");
}

#[test]
fn model_info_json_accepts_short_id_and_includes_raw_metadata() {
    let output = vinpst_command()
        .args(["model", "info", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--json")
        .output()
        .expect("run vinpst model info --json");

    let value = assert_json_success(output, "model info json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["source"]["kind"], "file");
    assert_eq!(value["i18n"]["kind"], "file");

    let model = &value["model"];
    assert_eq!(
        model["id"],
        "model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"
    );
    assert_eq!(model["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(model["title"], "SenseVoice 五语");
    assert_eq!(model["backend"], "sherpa-offline");
    assert_eq!(model["family"], "sense_voice");
    assert_eq!(model["runtime"], "offline");
    assert_eq!(model["support"], "supported");
    assert_eq!(
        model["vinpst_model"]["model"]["sense_voice"]["model"],
        "model.int8.onnx"
    );
    assert_eq!(
        model["vinpst_model"]["model"]["sense_voice"]["use_itn"],
        true
    );
}

#[test]
fn model_info_text_accepts_full_id() {
    let output = vinpst_command()
        .args([
            "model",
            "info",
            "model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8",
            "--registry",
        ])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .output()
        .expect("run vinpst model info text");

    let stdout = assert_stdout_success(output, "model info text");
    assert!(stdout.contains("registry_source: file:"));
    assert!(stdout.contains("id: model.sherpa-onnx.sense-voice-zh-en-ja-ko-yue-int8"));
    assert!(stdout.contains("short_id: onnx-sv-zh-int8-off"));
    assert!(stdout.contains("title: SenseVoice 五语"));
    assert!(stdout.contains("backend: sherpa-offline"));
    assert!(stdout.contains("family: sense_voice"));
    assert!(stdout.contains("support: supported"));
    assert!(stdout.contains("urls:"));
}

#[test]
fn model_info_rejects_unknown_id_or_short_id() {
    let output = vinpst_command()
        .args(["model", "info", "missing-model", "--registry"])
        .arg(live_models_fixture())
        .output()
        .expect("run vinpst model info unknown id");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unknown model id or short_id `missing-model`"));
}

#[test]
fn model_install_dry_run_json_plans_target_and_archive_without_mutation() {
    let output = vinpst_command()
        .args(["model", "install", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .args(["--model-root", "/tmp/vinpst-models"])
        .args(["--staging-root", "/tmp/vinpst-stage"])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst model install --dry-run --json");

    let value = assert_json_success(output, "model install dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_download"], false);
    assert_eq!(value["will_extract"], false);
    assert_eq!(value["will_write_config"], false);
    assert_eq!(value["model"]["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(
        value["archive"]["file_name"],
        "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2"
    );
    assert_eq!(value["archive"]["format"], "tar_bz2");
    assert_eq!(value["archive"]["supported"], true);
    assert_eq!(value["archive"]["supported_formats"][0], "tar");
    assert_eq!(value["archive"]["supported_formats"][2], "tar_bz2");
    assert_eq!(value["archive"]["size_bytes"], 165_675_008);
    assert_eq!(
        value["target"]["model_dir"],
        "/tmp/vinpst-models/onnx-sv-zh-int8-off"
    );
    assert_eq!(
        value["target"]["metadata_path"],
        "/tmp/vinpst-models/onnx-sv-zh-int8-off/vinpst-model.json"
    );
    assert_eq!(
        value["staging"]["archive_path"],
        "/tmp/vinpst-stage/onnx-sv-zh-int8-off/archives/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2"
    );
}

#[test]
fn model_install_dry_run_text_reports_no_side_effects() {
    let output = vinpst_command()
        .args(["model", "install", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--model-root", "/tmp/vinpst-models"])
        .args(["--staging-root", "/tmp/vinpst-stage"])
        .arg("--dry-run")
        .output()
        .expect("run vinpst model install --dry-run");

    let stdout = assert_stdout_success(output, "model install dry-run text");
    assert!(stdout.contains("Would install model"));
    assert!(stdout.contains("onnx-sv-zh-int8-off"));
    assert!(stdout.contains("Location: /tmp/vinpst-models/onnx-sv-zh-int8-off"));
    for internal in [
        "dry_run:",
        "archive_format:",
        "archive_supported:",
        "will_download",
        "will_extract",
        "will_write_config",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal install detail: {internal}"
        );
    }
}

#[test]
fn model_add_alias_matches_install_dry_run() {
    let output = vinpst_command()
        .args(["model", "add", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--model-root", "/tmp/vinpst-models"])
        .args(["--staging-root", "/tmp/vinpst-stage"])
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst model add --dry-run --json");

    let value = assert_json_success(output, "model add alias dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["model"]["short_id"], "onnx-sv-zh-int8-off");
    assert_eq!(value["archive"]["format"], "tar_bz2");
    assert_eq!(value["will_write_config"], false);
}

#[test]
fn model_install_without_dry_run_downloads_local_archive_without_config_mutation() {
    let temp_root = unique_temp_dir("vinpst-cli-model-install");
    std::fs::create_dir_all(&temp_root).expect("create temp root");

    let archive =
        build_test_tar_archive(&[("model.int8.onnx", b"onnx"), ("tokens.txt", b"tokens")]);
    let archive_sha256 = vinpst_registry::sha256_hex(&archive);
    let (url, handle) = serve_single_binary_response(archive);
    let registry_path = write_temp_json(
        "live-model-install",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.test.install",
                    "short_id": "test-install",
                    "urls": [url],
                    "sha256": archive_sha256,
                    "size_bytes": 123,
                    "language": "zh",
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "sense_voice",
                        "language": "zh",
                        "runtime": "offline",
                        "size_bytes": 123,
                        "supports_hotwords": false,
                        "model": {
                            "tokens": "tokens.txt",
                            "sense_voice": {
                                "model": "model.int8.onnx",
                                "language": "zh",
                                "use_itn": true
                            }
                        }
                    }
                }
            ]
        })
        .to_string(),
    );
    let model_root = temp_root.join("models");
    let staging_root = temp_root.join("stage");

    let output = vinpst_command()
        .args(["model", "install", "test-install", "--registry"])
        .arg(&registry_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--staging-root")
        .arg(&staging_root)
        .arg("--json")
        .output()
        .expect("run vinpst model install");

    let value = assert_json_success(output, "model install json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["will_write_config"], false);
    assert_eq!(value["install"]["checksum_verified"], true);
    assert_eq!(value["install"]["file_count"], 2);
    assert_eq!(
        value["install"]["model_dir"],
        model_root.join("test-install").to_string_lossy().as_ref()
    );
    assert_eq!(
        std::fs::read_to_string(model_root.join("test-install/model.int8.onnx")).unwrap(),
        "onnx"
    );
    assert_eq!(
        std::fs::read_to_string(model_root.join("test-install/tokens.txt")).unwrap(),
        "tokens"
    );
    let metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(model_root.join("test-install/vinpst-model.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["backend"], "sherpa-offline");
    assert_eq!(metadata["model"]["sense_voice"]["model"], "model.int8.onnx");

    let request = handle.join().expect("HTTP thread should finish");
    assert!(request.starts_with("GET /model.tar HTTP/1.1"));
    let _ = std::fs::remove_file(registry_path);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    ));
    path
}

fn serve_single_binary_response(bytes: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
    let url = format!("http://{}/model.tar", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept HTTP request");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = std::io::Read::read(&mut stream, &mut buffer).expect("read HTTP request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response_header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        std::io::Write::write_all(&mut stream, response_header.as_bytes())
            .expect("write HTTP response header");
        std::io::Write::write_all(&mut stream, &bytes).expect("write HTTP response body");
        String::from_utf8_lossy(&request).into_owned()
    });
    (url, handle)
}

fn build_test_tar_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut output = Vec::new();
    for (path, data) in entries {
        write_raw_tar_file(&mut output, path, data);
    }
    output.extend_from_slice(&[0_u8; 1024]);
    output
}

fn write_raw_tar_file(output: &mut Vec<u8>, path: &str, data: &[u8]) {
    assert!(path.len() <= 100, "test tar path is too long");
    let mut header = [0_u8; 512];
    header[..path.len()].copy_from_slice(path.as_bytes());
    write_tar_octal(&mut header[100..108], 0o644);
    write_tar_octal(&mut header[108..116], 0);
    write_tar_octal(&mut header[116..124], 0);
    write_tar_octal(&mut header[124..136], data.len() as u64);
    write_tar_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum = header.iter().map(|byte| u32::from(*byte)).sum::<u32>();
    let checksum_text = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(checksum_text.as_bytes());
    output.extend_from_slice(&header);
    output.extend_from_slice(data);
    let padding = (512 - (data.len() % 512)) % 512;
    output.extend(std::iter::repeat_n(0_u8, padding));
}

fn write_tar_octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let text = format!("{value:0width$o}\0");
    field.copy_from_slice(text.as_bytes());
}

#[test]
fn model_use_dry_run_json_previews_config_patch_for_registry_model() {
    let output = vinpst_command()
        .args(["model", "use", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .args(["--model-root", "/tmp/vinpst-models"])
        .args(["--reload-daemon", "--dry-run", "--json"])
        .output()
        .expect("run vinpst model use --dry-run --json");

    let value = assert_json_success(output, "model use dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_write_config"], false);
    assert_eq!(value["selector"]["kind"], "registry");
    assert_eq!(
        value["selector"]["resolved_short_id"],
        "onnx-sv-zh-int8-off"
    );
    assert_eq!(value["selector"]["title"], "SenseVoice 五语");
    assert_eq!(
        value["patch"]["asr.active_provider"]["before"],
        "sherpa-onnx"
    );
    assert_eq!(
        value["patch"]["asr.active_provider"]["after"],
        "sherpa-onnx"
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["provider_id"],
        "sherpa-onnx"
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["provider_type"],
        "local"
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["before"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["after"],
        "/tmp/vinpst-models/onnx-sv-zh-int8-off"
    );
    assert_eq!(value["reload_daemon"]["requested"], true);
    assert_eq!(value["reload_daemon"]["will_call_dbus"], true);
    assert_eq!(value["reload_daemon"]["called"], false);
    assert_eq!(value["reload_daemon"]["dbus"]["method"], "ReloadAsrBackend");
}

#[test]
fn model_use_dry_run_json_accepts_managed_model_name_without_registry() {
    let output = vinpst_command()
        .args([
            "model",
            "use",
            "custom-managed",
            "--model-root",
            "/tmp/vinpst-models",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst model use managed name --dry-run --json");

    let value = assert_json_success(output, "model use managed name dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["selector"]["kind"], "managed-dir");
    assert_eq!(
        value["patch"]["asr.providers[].model"]["after"],
        "/tmp/vinpst-models/custom-managed"
    );
    assert_eq!(value["will_write_config"], false);
}

#[test]
fn model_use_installed_bypasses_registry_id_resolution() {
    let output = vinpst_command()
        .args([
            "model",
            "use",
            "onnx-sv-zh-int8-off",
            "--installed",
            "--registry",
        ])
        .arg(live_models_fixture())
        .args(["--model-root", "/tmp/vinpst-models", "--dry-run", "--json"])
        .output()
        .expect("run vinpst model use --installed --dry-run --json");

    let value = assert_json_success(output, "model use installed dry-run json");
    assert_eq!(value["selector"]["kind"], "managed-dir");
    assert_eq!(
        value["selector"]["resolved_short_id"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["after"],
        "/tmp/vinpst-models/onnx-sv-zh-int8-off"
    );
}

#[test]
fn model_use_installed_rejects_path_selector() {
    let output = vinpst_command()
        .args([
            "model",
            "use",
            "foo/bar",
            "--installed",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst model use --installed with path selector");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("model use --installed expects a managed model directory name, not a path")
    );
}

#[test]
fn model_use_dry_run_text_accepts_installed_path_without_registry() {
    let output = vinpst_command()
        .args([
            "model",
            "use",
            "/tmp/vinpst-models/custom",
            "--reload-daemon",
            "--dry-run",
        ])
        .output()
        .expect("run vinpst model use path --dry-run");

    let stdout = assert_stdout_success(output, "model use path dry-run text");
    assert!(stdout.contains(
        "Would select model `/tmp/vinpst-models/custom` for ASR provider `sherpa-onnx`."
    ));
    for internal in [
        "dry_run:",
        "selector_kind:",
        "model_before:",
        "model_after:",
        "will_write_config",
        "reload_daemon_requested",
        "daemon_reloaded",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal model-use detail: {internal}"
        );
    }
}

#[test]
fn model_use_without_write_target_requires_output_or_in_place() {
    let output = vinpst_command()
        .args(["model", "use", "/tmp/vinpst-models/custom"])
        .output()
        .expect("run vinpst model use without dry-run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains(
        "model use writes require --output <path> or --in-place; rerun with --dry-run to inspect the config patch"
    ));
}

#[test]
fn model_use_output_writes_updated_config_without_overwriting_input() {
    let temp_root = unique_temp_dir("vinpst-cli-model-use-output");
    std::fs::create_dir_all(&temp_root).expect("create temp root");
    let input_config = temp_root.join("input.json");
    let output_config = temp_root.join("out/updated.json");
    std::fs::copy(workspace_file("data/default-config.json"), &input_config)
        .expect("copy default config fixture");

    let output = vinpst_command()
        .args(["model", "use", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .arg("--config")
        .arg(&input_config)
        .arg("--model-root")
        .arg(temp_root.join("models"))
        .arg("--output")
        .arg(&output_config)
        .arg("--json")
        .output()
        .expect("run vinpst model use --output --json");

    let value = assert_json_success(output, "model use output json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["will_write_config"], true);
    assert_eq!(value["wrote_config"], true);
    assert_eq!(
        value["output_path"],
        output_config.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["patch"]["asr.providers[].model"]["after"],
        temp_root
            .join("models/onnx-sv-zh-int8-off")
            .to_string_lossy()
            .as_ref()
    );

    let original: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&input_config).expect("read input config"))
            .expect("parse input config");
    assert_eq!(original["asr"]["providers"][0].get("model"), None);

    let updated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&output_config).expect("read output config"))
            .expect("parse output config");
    assert_eq!(updated["asr"]["active_provider"], "sherpa-onnx");
    assert_eq!(
        updated["asr"]["providers"][0]["model"],
        temp_root
            .join("models/onnx-sv-zh-int8-off")
            .to_string_lossy()
            .as_ref()
    );

    let validate = vinpst_command()
        .args(["config", "validate"])
        .arg(&output_config)
        .arg("--summary-only")
        .output()
        .expect("validate updated config");
    assert_json_success(validate, "updated config validate");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_use_output_refuses_to_overwrite_input_config() {
    let config_path = workspace_file("data/default-config.json");
    let output = vinpst_command()
        .args(["model", "use", "/tmp/vinpst-models/custom"])
        .arg("--config")
        .arg(&config_path)
        .arg("--output")
        .arg(&config_path)
        .output()
        .expect("run vinpst model use same input and output");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("refusing to overwrite input config"));
}

#[test]
fn model_use_in_place_updates_config_and_writes_backup() {
    let temp_root = unique_temp_dir("vinpst-cli-model-use-in-place");
    std::fs::create_dir_all(&temp_root).expect("create temp root");
    let config_path = temp_root.join("config.json");
    std::fs::copy(workspace_file("data/default-config.json"), &config_path)
        .expect("copy default config fixture");
    let original = std::fs::read_to_string(&config_path).expect("read original config");
    let backup_path = temp_root.join("config.json.bak");

    let output = vinpst_command()
        .args(["model", "use", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .arg("--config")
        .arg(&config_path)
        .arg("--model-root")
        .arg(temp_root.join("models"))
        .args(["--in-place", "--json"])
        .output()
        .expect("run vinpst model use --in-place --json");

    let value = assert_json_success(output, "model use in-place json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["wrote_config"], true);
    assert_eq!(value["in_place"], true);
    assert_eq!(value["output_path"], config_path.to_string_lossy().as_ref());
    assert_eq!(value["backup_path"], backup_path.to_string_lossy().as_ref());

    let backup = std::fs::read_to_string(&backup_path).expect("read backup config");
    assert_eq!(backup, original);
    let updated: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).expect("read updated config"))
            .expect("parse updated config");
    assert_eq!(updated["asr"]["active_provider"], "sherpa-onnx");
    assert_eq!(
        updated["asr"]["providers"][0]["model"],
        temp_root
            .join("models/onnx-sv-zh-int8-off")
            .to_string_lossy()
            .as_ref()
    );

    let validate = vinpst_command()
        .args(["config", "validate"])
        .arg(&config_path)
        .arg("--summary-only")
        .output()
        .expect("validate in-place updated config");
    assert_json_success(validate, "in-place updated config validate");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_use_in_place_requires_config_path() {
    let output = vinpst_command()
        .args(["model", "use", "/tmp/vinpst-models/custom", "--in-place"])
        .output()
        .expect("run vinpst model use --in-place without config");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("model use --in-place requires --config <path>"));
}

#[test]
fn model_use_rejects_output_and_in_place_together() {
    let output = vinpst_command()
        .args(["model", "use", "/tmp/vinpst-models/custom"])
        .arg("--output")
        .arg("/tmp/vinpst-updated.json")
        .arg("--in-place")
        .output()
        .expect("run vinpst model use with conflicting write targets");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("model use cannot combine --output and --in-place"));
}

#[test]
fn model_remove_dry_run_json_plans_registry_model_under_model_root() {
    let temp_root = unique_temp_dir("vinpst-cli-model-remove");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("onnx-sv-zh-int8-off");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");

    let output = vinpst_command()
        .args(["model", "remove", "onnx-sv-zh-int8-off", "--registry"])
        .arg(live_models_fixture())
        .args(["--i18n"])
        .arg(live_i18n_fixture())
        .arg("--model-root")
        .arg(&model_root)
        .args(["--dry-run", "--json"])
        .output()
        .expect("run vinpst model remove --dry-run --json");

    let value = assert_json_success(output, "model remove dry-run json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], true);
    assert_eq!(value["will_remove"], false);
    assert_eq!(value["selector"]["kind"], "registry");
    assert_eq!(
        value["selector"]["resolved_short_id"],
        "onnx-sv-zh-int8-off"
    );
    assert_eq!(value["selector"]["title"], "SenseVoice 五语");
    assert_eq!(
        value["target"]["model_root"],
        model_root.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["target"]["path"],
        model_dir.to_string_lossy().as_ref()
    );
    assert_eq!(value["target"]["exists"], true);
    assert_eq!(value["target"]["is_dir"], true);
    assert_eq!(value["target"]["managed"], true);
    assert!(model_dir.exists(), "dry-run must not delete the model dir");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_rm_alias_dry_run_text_accepts_managed_dir_name() {
    let temp_root = unique_temp_dir("vinpst-cli-model-rm-alias");
    let model_root = temp_root.join("models");
    std::fs::create_dir_all(&model_root).expect("create model root");

    let output = vinpst_command()
        .args(["model", "rm", "custom-model"])
        .arg("--model-root")
        .arg(&model_root)
        .arg("--dry-run")
        .output()
        .expect("run vinpst model rm --dry-run");

    let stdout = assert_stdout_success(output, "model rm dry-run text");
    assert!(stdout.contains("Would remove model `custom-model`."));
    assert!(stdout.contains(&format!(
        "Location: {}",
        model_root.join("custom-model").display()
    )));
    for internal in [
        "dry_run:",
        "selector_kind:",
        "model_root:",
        "target_path:",
        "exists:",
        "will_remove:",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal removal detail: {internal}"
        );
    }
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_remove_installed_bypasses_registry_id_resolution() {
    let output = vinpst_command()
        .args([
            "model",
            "remove",
            "onnx-sv-zh-int8-off",
            "--installed",
            "--registry",
        ])
        .arg(live_models_fixture())
        .args(["--model-root", "/tmp/vinpst-models", "--dry-run", "--json"])
        .output()
        .expect("run vinpst model remove --installed --dry-run --json");

    let value = assert_json_success(output, "model remove installed dry-run json");
    assert_eq!(value["selector"]["kind"], "managed-dir");
    assert_eq!(
        value["selector"]["resolved_short_id"],
        serde_json::Value::Null
    );
    assert_eq!(
        value["target"]["path"],
        "/tmp/vinpst-models/onnx-sv-zh-int8-off"
    );
    assert_eq!(value["removed"], false);
}

#[test]
fn model_remove_installed_rejects_path_selector() {
    let output = vinpst_command()
        .args([
            "model",
            "remove",
            "foo/bar",
            "--installed",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("run vinpst model remove --installed with path selector");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains(
            "model remove --installed expects a managed model directory name, not a path"
        )
    );
}

#[test]
fn model_remove_dry_run_rejects_path_outside_model_root() {
    let temp_root = unique_temp_dir("vinpst-cli-model-remove-outside");
    let model_root = temp_root.join("models");
    std::fs::create_dir_all(&model_root).expect("create model root");

    let output = vinpst_command()
        .args(["model", "remove", "/tmp/outside-vinpst-model"])
        .arg("--model-root")
        .arg(&model_root)
        .arg("--dry-run")
        .output()
        .expect("run vinpst model remove outside path");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("because it is outside model root"));
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_remove_without_yes_requires_confirmation() {
    let output = vinpst_command()
        .args(["model", "remove", "custom-model"])
        .output()
        .expect("run vinpst model remove without dry-run");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains(
        "model remove requires --yes to delete; rerun with --dry-run to inspect the removal plan"
    ));
}

#[test]
fn model_remove_yes_deletes_inactive_managed_model_dir() {
    let temp_root = unique_temp_dir("vinpst-cli-model-remove-yes");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("custom-model");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");
    std::fs::write(model_dir.join("vinpst-model.json"), b"{}\n").expect("write model metadata");

    let output = vinpst_command()
        .args(["model", "remove", "custom-model"])
        .arg("--model-root")
        .arg(&model_root)
        .args(["--yes", "--json"])
        .output()
        .expect("run vinpst model remove --yes --json");

    let value = assert_json_success(output, "model remove yes json");
    assert_eq!(value["ok"], true);
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["will_remove"], true);
    assert_eq!(value["removed"], true);
    assert_eq!(
        value["target"]["path"],
        model_dir.to_string_lossy().as_ref()
    );
    assert_eq!(value["target"]["exists"], false);
    assert!(
        !model_dir.exists(),
        "confirmed remove should delete the model dir"
    );
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_remove_yes_refuses_active_config_model() {
    let temp_root = unique_temp_dir("vinpst-cli-model-remove-active");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("active-model");
    std::fs::create_dir_all(&model_dir).expect("create active model dir");
    let config_path = temp_root.join("active-config.json");
    let mut config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(workspace_file("data/default-config.json"))
            .expect("read default config"),
    )
    .expect("parse default config");
    config["asr"]["providers"][0]["model"] =
        serde_json::Value::String(model_dir.to_string_lossy().into_owned());
    std::fs::write(
        &config_path,
        format!("{}\n", serde_json::to_string_pretty(&config).unwrap()),
    )
    .expect("write active config");

    let output = vinpst_command()
        .args(["model", "remove", "active-model"])
        .arg("--model-root")
        .arg(&model_root)
        .arg("--config")
        .arg(&config_path)
        .arg("--yes")
        .output()
        .expect("run vinpst model remove active model");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("refusing to remove active model"));
    assert!(model_dir.exists(), "active model dir should not be deleted");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_info_json_reads_installed_model_metadata_from_path() {
    let temp_root = unique_temp_dir("vinpst-cli-model-info-installed-json");
    let model_dir = temp_root.join("models/test-installed");
    std::fs::create_dir_all(model_dir.join("subdir")).expect("create installed model dirs");
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").expect("write model file");
    std::fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens file");
    std::fs::write(model_dir.join("subdir/extra.bin"), b"extra").expect("write nested file");
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        serde_json::json!({
            "backend": "sherpa-offline",
            "family": "sense_voice",
            "language": "zh",
            "runtime": "offline",
            "size_bytes": 42,
            "supports_hotwords": false,
            "model": {
                "tokens": "tokens.txt",
                "sense_voice": {
                    "model": "model.int8.onnx",
                    "language": "zh",
                    "use_itn": true
                }
            }
        })
        .to_string(),
    )
    .expect("write installed metadata");

    let output = vinpst_command()
        .args(["model", "info"])
        .arg(&model_dir)
        .arg("--json")
        .output()
        .expect("run vinpst model info installed path --json");

    let value = assert_json_success(output, "installed model info json");
    assert_eq!(value["source"]["kind"], "installed");
    assert_eq!(
        value["model"]["model_dir"],
        model_dir.to_string_lossy().as_ref()
    );
    assert_eq!(value["model"]["backend"], "sherpa-offline");
    assert_eq!(value["model"]["family"], "sense_voice");
    assert_eq!(value["model"]["runtime"], "offline");
    assert_eq!(value["model"]["file_count"], 4);
    assert_eq!(
        value["model"]["vinpst_model"]["model"]["sense_voice"]["model"],
        "model.int8.onnx"
    );
    let files = value["model"]["files"]
        .as_array()
        .expect("files should be array");
    assert!(files.iter().any(|file| file == "model.int8.onnx"));
    assert!(files.iter().any(|file| file == "tokens.txt"));
    assert!(files.iter().any(|file| file == "subdir/extra.bin"));
    assert!(files.iter().any(|file| file == "vinpst-model.json"));
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_info_json_reads_installed_model_metadata_by_managed_name() {
    let temp_root = unique_temp_dir("vinpst-cli-model-info-installed-name");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("test-installed");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").expect("write model file");
    std::fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens file");
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        serde_json::json!({
            "backend": "sherpa-offline",
            "family": "sense_voice",
            "language": "zh",
            "runtime": "offline",
            "supports_hotwords": false
        })
        .to_string(),
    )
    .expect("write installed metadata");

    let output = vinpst_command()
        .args([
            "model",
            "info",
            "test-installed",
            "--installed",
            "--model-root",
        ])
        .arg(&model_root)
        .arg("--json")
        .output()
        .expect("run vinpst model info --installed --json");

    let value = assert_json_success(output, "installed model info by name json");
    assert_eq!(value["source"]["kind"], "installed");
    assert_eq!(
        value["model"]["model_dir"],
        model_dir.to_string_lossy().as_ref()
    );
    assert_eq!(value["model"]["family"], "sense_voice");
    assert_eq!(value["model"]["file_count"], 3);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_info_installed_rejects_path_selector() {
    let output = vinpst_command()
        .args(["model", "info", "foo/bar", "--installed", "--json"])
        .output()
        .expect("run vinpst model info --installed with path selector");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr
            .contains("model info --installed expects a managed model directory name, not a path")
    );
}

#[test]
fn model_info_text_reads_installed_model_metadata_from_path() {
    let temp_root = unique_temp_dir("vinpst-cli-model-info-installed-text");
    let model_dir = temp_root.join("models/test-installed");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").expect("write model file");
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        serde_json::json!({
            "backend": "sherpa-offline",
            "family": "sense_voice",
            "language": "zh",
            "runtime": "offline",
            "size_bytes": 42,
            "supports_hotwords": true
        })
        .to_string(),
    )
    .expect("write installed metadata");

    let output = vinpst_command()
        .args(["model", "info"])
        .arg(&model_dir)
        .output()
        .expect("run vinpst model info installed path");

    let stdout = assert_stdout_success(output, "installed model info text");
    assert!(stdout.contains("source: installed"));
    assert!(stdout.contains(&format!("model_dir: {}", model_dir.display())));
    assert!(stdout.contains("backend: sherpa-offline"));
    assert!(stdout.contains("family: sense_voice"));
    assert!(stdout.contains("runtime: offline"));
    assert!(stdout.contains("supports_hotwords: true"));
    assert!(stdout.contains("  - model.int8.onnx"));
    assert!(stdout.contains("  - vinpst-model.json"));
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_info_installed_path_requires_metadata_file() {
    let temp_root = unique_temp_dir("vinpst-cli-model-info-installed-missing");
    let model_dir = temp_root.join("models/missing-metadata");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");

    let output = vinpst_command()
        .args(["model", "info"])
        .arg(&model_dir)
        .output()
        .expect("run vinpst model info missing metadata");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("read installed model metadata"));
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_cli_workflow_installs_inspects_uses_and_removes_local_archive() {
    let temp_root = unique_temp_dir("vinpst-cli-model-workflow");
    std::fs::create_dir_all(&temp_root).expect("create temp root");
    let (registry_path, handle) = live_workflow_registry_fixture();
    let model_root = temp_root.join("models");
    let staging_root = temp_root.join("stage");
    let model_dir = model_root.join("test-workflow");
    let output_config = temp_root.join("updated-config.json");

    let install = vinpst_command()
        .args(["model", "install", "test-workflow", "--registry"])
        .arg(&registry_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--staging-root")
        .arg(&staging_root)
        .arg("--json")
        .output()
        .expect("run workflow model install");
    let install_json = assert_json_success(install, "workflow install json");
    assert_eq!(install_json["dry_run"], false);
    assert_eq!(install_json["install"]["checksum_verified"], true);
    assert!(model_dir.join("model.int8.onnx").exists());
    assert!(model_dir.join("tokens.txt").exists());
    assert!(model_dir.join("vinpst-model.json").exists());

    let info = vinpst_command()
        .args(["model", "info"])
        .arg(&model_dir)
        .arg("--json")
        .output()
        .expect("run workflow model info");
    let info_json = assert_json_success(info, "workflow installed info json");
    assert_eq!(info_json["source"]["kind"], "installed");
    assert_eq!(info_json["model"]["backend"], "sherpa-offline");
    assert_eq!(info_json["model"]["family"], "sense_voice");
    assert_eq!(info_json["model"]["file_count"], 3);

    let use_output = vinpst_command()
        .args(["model", "use", "test-workflow", "--registry"])
        .arg(&registry_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--output")
        .arg(&output_config)
        .arg("--json")
        .output()
        .expect("run workflow model use output");
    let use_json = assert_json_success(use_output, "workflow use output json");
    assert_eq!(use_json["wrote_config"], true);
    assert_eq!(
        use_json["patch"]["asr.providers[].model"]["after"],
        model_dir.to_string_lossy().as_ref()
    );

    let active_remove = vinpst_command()
        .args(["model", "remove", "test-workflow", "--registry"])
        .arg(&registry_path)
        .arg("--model-root")
        .arg(&model_root)
        .arg("--config")
        .arg(&output_config)
        .arg("--yes")
        .output()
        .expect("run workflow active model remove");
    assert!(!active_remove.status.success());
    let stderr = String::from_utf8(active_remove.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("refusing to remove active model"));
    assert!(model_dir.exists());

    let remove = vinpst_command()
        .args(["model", "remove", "test-workflow", "--registry"])
        .arg(&registry_path)
        .arg("--model-root")
        .arg(&model_root)
        .args(["--yes", "--json"])
        .output()
        .expect("run workflow model remove");
    let remove_json = assert_json_success(remove, "workflow remove json");
    assert_eq!(remove_json["removed"], true);
    assert_eq!(remove_json["target"]["exists"], false);
    assert!(!model_dir.exists());

    let request = handle.join().expect("HTTP thread should finish");
    assert!(request.starts_with("GET /model.tar HTTP/1.1"));
    let _ = std::fs::remove_file(registry_path);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn live_workflow_registry_fixture() -> (std::path::PathBuf, std::thread::JoinHandle<String>) {
    let archive =
        build_test_tar_archive(&[("model.int8.onnx", b"onnx"), ("tokens.txt", b"tokens")]);
    let archive_sha256 = vinpst_registry::sha256_hex(&archive);
    let (url, handle) = serve_single_binary_response(archive);
    let registry_path = write_temp_json(
        "live-model-workflow",
        &serde_json::json!({
            "version": 2,
            "items": [
                {
                    "id": "model.test.workflow",
                    "short_id": "test-workflow",
                    "urls": [url],
                    "sha256": archive_sha256,
                    "size_bytes": 123,
                    "language": "zh",
                    "vinpst_model": {
                        "backend": "sherpa-offline",
                        "family": "sense_voice",
                        "language": "zh",
                        "runtime": "offline",
                        "size_bytes": 123,
                        "supports_hotwords": false,
                        "model": {
                            "tokens": "tokens.txt",
                            "sense_voice": {
                                "model": "model.int8.onnx",
                                "language": "zh",
                                "use_itn": true
                            }
                        }
                    }
                }
            ]
        })
        .to_string(),
    );
    (registry_path, handle)
}

#[test]
fn model_list_installed_json_scans_model_root_metadata() {
    let temp_root = unique_temp_dir("vinpst-cli-model-list-installed-json");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("installed-one");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");
    std::fs::write(model_dir.join("model.int8.onnx"), b"onnx").expect("write model file");
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        serde_json::json!({
            "backend": "sherpa-offline",
            "family": "sense_voice",
            "language": "zh",
            "runtime": "offline",
            "size_bytes": 42,
            "supports_hotwords": true
        })
        .to_string(),
    )
    .expect("write installed metadata");
    std::fs::create_dir_all(model_root.join("incomplete-model")).expect("create incomplete dir");

    let output = vinpst_command()
        .args(["model", "list", "--installed", "--model-root"])
        .arg(&model_root)
        .arg("--json")
        .output()
        .expect("run vinpst model list --installed --json");

    let value = assert_json_success(output, "model list installed json");
    assert_eq!(value["source"]["kind"], "installed");
    assert_eq!(
        value["source"]["model_root"],
        model_root.to_string_lossy().as_ref()
    );
    assert_eq!(value["model_count"], 1);
    let model = &value["models"][0];
    assert_eq!(model["id"], "installed-one");
    assert_eq!(model["name"], "installed-one");
    assert_eq!(model["model_dir"], model_dir.to_string_lossy().as_ref());
    assert_eq!(model["backend"], "sherpa-offline");
    assert_eq!(model["family"], "sense_voice");
    assert_eq!(model["runtime"], "offline");
    assert_eq!(model["supports_hotwords"], true);
    assert_eq!(model["file_count"], 2);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_list_installed_text_prints_local_rows() {
    let temp_root = unique_temp_dir("vinpst-cli-model-list-installed-text");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("installed-text");
    std::fs::create_dir_all(&model_dir).expect("create installed model dir");
    std::fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens file");
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        serde_json::json!({
            "backend": "sherpa-offline",
            "family": "sense_voice",
            "language": "zh",
            "runtime": "offline",
            "size_bytes": 42,
            "supports_hotwords": false
        })
        .to_string(),
    )
    .expect("write installed metadata");

    let output = vinpst_command()
        .args(["model", "list", "--installed", "--model-root"])
        .arg(&model_root)
        .output()
        .expect("run vinpst model list --installed");

    let stdout = assert_stdout_success(output, "model list installed text");
    assert!(stdout.contains("ID\tLANGUAGE\tSIZE\tTYPE\tHOTWORDS\tSTATUS"));
    assert!(stdout.contains("installed-text\tzh\t42 B\tsense_voice\tno\tinstalled"));
    for internal in [
        "model_root:",
        "models:",
        "path\t",
        "backend",
        "runtime",
        "files",
    ] {
        assert!(
            !stdout.contains(internal),
            "leaked internal list detail: {internal}"
        );
    }
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_list_installed_scans_legacy_engine_model_layout() {
    let temp_root = unique_temp_dir("vinpst-cli-model-list-installed-legacy-layout");
    let model_root = temp_root.join("models");
    let model_dir = model_root.join("sherpa-onnx").join("moonshine-v1");
    std::fs::create_dir_all(&model_dir).expect("create legacy installed model dir");
    std::fs::write(model_dir.join("tokens.txt"), b"tokens").expect("write tokens file");
    std::fs::write(
        model_dir.join("vinpst-model.json"),
        serde_json::json!({
            "backend": "sherpa-offline",
            "family": "moonshine",
            "language": "en",
            "runtime": "offline"
        })
        .to_string(),
    )
    .expect("write legacy installed metadata");

    let output = vinpst_command()
        .args(["model", "list", "--installed", "--model-root"])
        .arg(&model_root)
        .arg("--json")
        .output()
        .expect("run vinpst model list for legacy layout");

    let value = assert_json_success(output, "legacy installed model list json");
    assert_eq!(value["model_count"], 1);
    assert_eq!(value["models"][0]["id"], "model.sherpa-onnx.moonshine-v1");
    assert_eq!(
        value["models"][0]["model_dir"],
        model_dir.to_string_lossy().as_ref()
    );
    assert_eq!(value["models"][0]["family"], "moonshine");
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_list_installed_empty_root_returns_empty_list() {
    let temp_root = unique_temp_dir("vinpst-cli-model-list-installed-empty");
    let model_root = temp_root.join("missing-models");

    let output = vinpst_command()
        .args(["model", "list", "--installed", "--model-root"])
        .arg(&model_root)
        .arg("--json")
        .output()
        .expect("run vinpst model list --installed empty");

    let value = assert_json_success(output, "model list installed empty json");
    assert_eq!(value["model_count"], 0);
    assert_eq!(value["models"].as_array().expect("models array").len(), 0);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[test]
fn model_list_rejects_available_and_installed_together() {
    let output = vinpst_command()
        .args(["model", "list", "--available", "--installed"])
        .output()
        .expect("run vinpst model list conflicting modes");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("model list cannot combine --available and --installed"));
}
