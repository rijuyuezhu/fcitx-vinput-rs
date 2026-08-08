use crate::{
    AsrProviderKind, COMMAND_SCENE_ID, ConfigError, RAW_SCENE_ID, VadConfig, VinpstConfig,
};
use vinpst_protocol::CandidateSource;

#[test]
fn config_file_parses_and_normalizes() {
    let path = std::env::temp_dir().join(format!(
        "vinpst-config-test-{}-file.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "raw",
            "definitions": [{"id":"raw","label":"Raw","candidate_count":0}]
          }
        }"#,
    )
    .unwrap();

    let config = VinpstConfig::from_json_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(config.version, 1);
    assert_eq!(config.asr.active_provider, "p");
    config.validate().unwrap();
}

#[test]
fn config_file_reports_read_errors() {
    let path = std::env::temp_dir().join(format!(
        "vinpst-config-test-{}-missing.json",
        std::process::id()
    ));

    let error = VinpstConfig::from_json_file(&path).unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::ReadFile { path: error_path, .. } if error_path == path
    ));
}

#[test]
fn parser_requires_explicit_schema_version() {
    let error = VinpstConfig::from_json_str(
        r#"{
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          }
        }"#,
    )
    .unwrap_err();

    assert!(matches!(error, crate::ConfigError::Json(_)));
}

#[test]
fn normalization_promotes_legacy_zero_version_to_one() {
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 0,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          }
        }"#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(config.version, 1);
    assert_eq!(config.scenes.active_scene, RAW_SCENE_ID);
}

#[test]
fn parser_rejects_future_schema_versions() {
    let error = VinpstConfig::from_json_str(
        r#"{
          "version": 2,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          }
        }"#,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        crate::ConfigError::UnsupportedSchemaVersion {
            found: 2,
            supported: crate::CURRENT_CONFIG_VERSION
        }
    ));
}

#[test]
fn validation_rejects_manually_constructed_future_schema_versions() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.version = crate::CURRENT_CONFIG_VERSION + 1;

    assert!(matches!(
        config.validate(),
        Err(crate::ConfigError::UnsupportedSchemaVersion {
            found: 2,
            supported: crate::CURRENT_CONFIG_VERSION
        })
    ));
}

#[test]
fn normalization_inserts_legacy_builtin_scenes_for_minimal_configs() {
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          }
        }"#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(config.scenes.active_scene, RAW_SCENE_ID);
    let raw = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == RAW_SCENE_ID)
        .unwrap();
    let command = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == COMMAND_SCENE_ID)
        .unwrap();
    assert_eq!(raw.label, "__label_raw__");
    assert_eq!(raw.candidate_count, 0);
    assert_eq!(command.label, "__label_command__");
    assert_eq!(command.candidate_count, 1);
    assert!(command.prompt.as_deref().is_some_and(|prompt| {
        prompt.contains("<vinput-selected>") && prompt.contains("{{asr}}")
    }));
}

#[test]
fn normalization_defaults_blank_active_scene_to_raw() {
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "",
            "definitions": [
              {"id":"__raw__","label":"Custom Raw","candidate_count":2}
            ]
          }
        }"#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(config.scenes.active_scene, RAW_SCENE_ID);
    assert_eq!(config.active_scene().unwrap().label, "Custom Raw");
}

#[test]
fn normalization_defaults_missing_active_scene_to_raw_with_existing_definitions() {
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "definitions": [
              {"id":"__command__","label":"Custom Command","candidate_count":3}
            ]
          }
        }"#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(config.scenes.active_scene, RAW_SCENE_ID);
    assert_eq!(config.scenes.definitions.len(), 2);
    let command = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == COMMAND_SCENE_ID)
        .unwrap();
    let raw = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == RAW_SCENE_ID)
        .unwrap();
    assert_eq!(command.label, "Custom Command");
    assert_eq!(command.candidate_count, 3);
    assert_eq!(raw.label, "__label_raw__");
}

#[test]
fn normalization_inserts_missing_builtin_scene_without_replacing_existing_one() {
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "__raw__",
            "definitions": [
              {"id":"__raw__","label":"Custom Raw","candidate_count":2}
            ]
          }
        }"#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(config.scenes.definitions.len(), 2);
    let raw = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == RAW_SCENE_ID)
        .unwrap();
    let command = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == COMMAND_SCENE_ID)
        .unwrap();
    assert_eq!(raw.label, "Custom Raw");
    assert_eq!(raw.candidate_count, 2);
    assert_eq!(command.label, "__label_command__");
    assert_eq!(command.candidate_count, 1);
}

#[test]
fn normalization_preserves_existing_builtin_scene_definitions() {
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "providers": [{"id":"p","type":"local"}]
          },
          "scenes": {
            "active_scene": "__raw__",
            "definitions": [
              {"id":"__raw__","label":"Custom Raw","candidate_count":2},
              {"id":"__command__","label":"Custom Command","candidate_count":3}
            ]
          }
        }"#,
    )
    .unwrap();

    config.validate().unwrap();
    assert_eq!(config.scenes.definitions.len(), 2);
    assert_eq!(config.scenes.definitions[0].label, "Custom Raw");
    assert_eq!(config.scenes.definitions[0].candidate_count, 2);
    assert_eq!(config.scenes.definitions[1].label, "Custom Command");
    assert_eq!(config.scenes.definitions[1].candidate_count, 3);
}

#[test]
fn committed_default_file_matches_bundled_default() {
    let disk_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/default-config.json");
    let disk_json = std::fs::read_to_string(&disk_path).unwrap();

    let disk_config = VinpstConfig::from_json_str(&disk_json).unwrap();
    let bundled_config = VinpstConfig::bundled_default().unwrap();

    assert_eq!(disk_config, bundled_config);
    disk_config.validate().unwrap();
}

#[test]
fn bundled_default_parses_and_validates() {
    let config = VinpstConfig::bundled_default().unwrap();
    config.validate().unwrap();
    assert_eq!(config.version, 1);
    assert_eq!(config.global.default_language, "zh");
    assert!(!config.global.duck_output_while_recording);
    assert!((config.global.duck_output_volume - 0.25).abs() < f32::EPSILON);
    assert_eq!(config.asr.active_provider, "sherpa-onnx");
    assert_eq!(config.asr.providers[0].kind, AsrProviderKind::Local);
    assert_eq!(config.scenes.active_scene, RAW_SCENE_ID);
    assert_eq!(config.active_scene().unwrap().id, RAW_SCENE_ID);
}

#[test]
fn vad_defaults_match_legacy_offline_contract() {
    assert_eq!(
        VadConfig::default(),
        VadConfig {
            enabled: true,
            threshold: 0.45,
            min_speech_duration: 0.15,
            min_silence_duration: 0.5,
            speech_pad_ms: 300,
        }
    );
    let config = VinpstConfig::from_json_str(
        r#"{
          "version": 1,
          "asr": {
            "active_provider": "p",
            "vad": {"enabled": true},
            "providers": [{"id":"p","type":"local"}]
          }
        }"#,
    )
    .unwrap();
    assert_eq!(config.asr.vad, VadConfig::default());
}

#[test]
fn validation_rejects_out_of_range_vad_values() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.vad.threshold = 1.0;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidVadThreshold(value)) if (value - 1.0).abs() < f32::EPSILON
    ));

    config.asr.vad = VadConfig::default();
    config.asr.vad.min_speech_duration = 0.0;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidVadMinSpeechDuration(value)) if value.abs() < f32::EPSILON
    ));

    config.asr.vad = VadConfig::default();
    config.asr.vad.min_silence_duration = 6.0;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidVadMinSilenceDuration(value)) if (value - 6.0).abs() < f32::EPSILON
    ));

    config.asr.vad = VadConfig::default();
    config.asr.vad.speech_pad_ms = 2_001;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidVadSpeechPadMs(2_001))
    ));
}

#[test]
fn scene_source_policy_is_explicit() {
    let config = VinpstConfig::bundled_default().unwrap();
    let raw = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == RAW_SCENE_ID)
        .unwrap();
    let command = config
        .scenes
        .definitions
        .iter()
        .find(|scene| scene.id == COMMAND_SCENE_ID)
        .unwrap();
    assert_eq!(raw.default_candidate_source(), CandidateSource::Raw);
    assert_eq!(command.default_candidate_source(), CandidateSource::Llm);
}

#[test]
fn summary_reports_config_counts() {
    let config = VinpstConfig::bundled_default().unwrap();
    let summary = config.summary();
    assert!(summary.ok);
    assert_eq!(summary.version, 1);
    assert_eq!(summary.active_scene, RAW_SCENE_ID);
    assert_eq!(summary.active_provider, "sherpa-onnx");
    assert_eq!(summary.scene_count, config.scenes.definitions.len());
    assert_eq!(summary.provider_count, config.asr.providers.len());
    assert_eq!(
        summary.registry_mirror_count,
        config.registry.base_urls.len()
    );
}

#[test]
fn summary_json_shape_is_stable() {
    let config = VinpstConfig::bundled_default().unwrap();
    let value = serde_json::to_value(config.summary()).unwrap();
    let object = value
        .as_object()
        .expect("summary should serialize as object");
    let keys = object.keys().map(String::as_str).collect::<Vec<_>>();

    assert_eq!(
        keys,
        [
            "active_provider",
            "active_scene",
            "ok",
            "provider_count",
            "registry_mirror_count",
            "scene_count",
            "version",
        ]
    );
}
#[test]
fn summary_serialization_omits_secret_bearing_fields() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers.push(crate::AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: Some("vinpst-asr-helper".to_owned()),
        args: vec!["--token".to_owned(), "asr-arg-secret".to_owned()],
        env: std::collections::HashMap::from([(
            "ASR_TOKEN".to_owned(),
            "asr-env-secret".to_owned(),
        )]),
        endpoint: None,
    });
    config.llm.providers.push(crate::LlmProviderConfig {
        id: "openai".to_owned(),
        base_url: "https://secret.example.invalid/v1".to_owned(),
        api_key: "llm-secret-token".to_owned(),
        model: Some("gpt-test".to_owned()),
        extra_body: serde_json::json!({"trace": "extra-body-secret"}),
        extra: std::collections::HashMap::from([(
            "future_secret".to_owned(),
            serde_json::json!("provider-extra-secret"),
        )]),
    });
    config.llm.adapters.push(crate::LlmAdapterConfig {
        id: "adapter".to_owned(),
        command: "vinpst-adapter".to_owned(),
        args: vec!["--token".to_owned(), "adapter-arg-secret".to_owned()],
        env: std::collections::HashMap::from([(
            "ADAPTER_TOKEN".to_owned(),
            "adapter-env-secret".to_owned(),
        )]),
        working_dir: Some("/tmp/vinpst-secret-workdir".to_owned()),
        extra: std::collections::HashMap::from([(
            "adapter_secret".to_owned(),
            serde_json::json!("adapter-extra-secret"),
        )]),
    });

    let json = serde_json::to_string(&config.summary()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["provider_count"], config.asr.providers.len());
    for forbidden_key in [
        "api_key",
        "base_url",
        "env",
        "args",
        "command",
        "working_dir",
        "extra_body",
        "extra",
    ] {
        assert!(
            !json.contains(&format!("\"{forbidden_key}\"")),
            "summary JSON must not expose {forbidden_key}"
        );
    }
    for secret in [
        "asr-arg-secret",
        "asr-env-secret",
        "https://secret.example.invalid/v1",
        "llm-secret-token",
        "extra-body-secret",
        "provider-extra-secret",
        "adapter-arg-secret",
        "adapter-env-secret",
        "/tmp/vinpst-secret-workdir",
        "adapter-extra-secret",
    ] {
        assert!(
            !json.contains(secret),
            "summary JSON must not leak {secret}"
        );
    }
}

#[test]
fn validation_rejects_duplicate_registry_base_urls() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    let duplicate = config.registry.base_urls[0].clone();
    config.registry.base_urls.push(duplicate.clone());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::DuplicateRegistryBaseUrl(url) if url == duplicate
    ));
}

#[test]
fn validation_rejects_empty_registry_base_urls() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.registry.base_urls.push("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidRegistryBaseUrl(url) if url == "  "
    ));
}

#[test]
fn validation_rejects_empty_capture_device() {
    let mut c = VinpstConfig::bundled_default().unwrap();
    c.global.capture_device = "  ".to_owned();
    assert!(matches!(
        c.validate().unwrap_err(),
        crate::ConfigError::InvalidCaptureDevice
    ));
}

#[test]
fn validation_rejects_empty_default_language() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.global.default_language = "  ".to_owned();
    let error = config.validate().unwrap_err();
    assert!(matches!(error, crate::ConfigError::InvalidDefaultLanguage));
}

#[test]
fn output_ducking_defaults_and_normalization_match_legacy() {
    let defaults = crate::GlobalConfig::default();
    assert!(!defaults.duck_output_while_recording);
    assert!((defaults.duck_output_volume - 0.25).abs() < f32::EPSILON);

    let low = VinpstConfig::from_json_str(r#"{"version":1,"global":{"duck_output_volume":-0.5}}"#)
        .unwrap();
    assert!(low.global.duck_output_volume.abs() < f32::EPSILON);

    let high = VinpstConfig::from_json_str(r#"{"version":1,"global":{"duck_output_volume":1.5}}"#)
        .unwrap();
    assert!((high.global.duck_output_volume - 1.0).abs() < f32::EPSILON);
}

#[test]
fn validation_rejects_non_finite_duck_output_volume() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.global.duck_output_volume = f32::NAN;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::InvalidDuckOutputVolume(value)) if value.is_nan()
    ));
}

#[test]
fn validation_accepts_empty_active_provider_as_unselected() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider.clear();
    config.validate().unwrap();
}

#[test]
fn validation_rejects_whitespace_active_provider() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "  ".to_owned();
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidActiveAsrProviderId
    ));
}

#[test]
fn validation_rejects_duplicate_scene_ids() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    let duplicate = config.scenes.definitions[0].clone();
    config.scenes.definitions.push(duplicate);
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::DuplicateSceneId(id) if id == RAW_SCENE_ID
    ));
}

#[test]
fn validation_rejects_empty_scene_labels() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].label = "  ".to_owned();
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidSceneLabel(id) if id == RAW_SCENE_ID
    ));
}

#[test]
fn validation_rejects_duplicate_asr_provider_ids() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    let duplicate = config.asr.providers[0].clone();
    config.asr.providers.push(duplicate);
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::DuplicateAsrProviderId(id) if id == "sherpa-onnx"
    ));
}

#[test]
fn validation_rejects_missing_active_scene() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.active_scene = "missing".to_owned();
    assert!(config.validate().is_err());
}

#[test]
fn typed_llm_and_command_provider_config_parses() {
    let input = r#"
    {
      "version": 1,
      "global": { "default_language": "zh", "capture_device": "default" },
      "asr": {
        "active_provider": "cmd",
        "providers": [
          {
            "id": "cmd",
            "type": "command",
            "command": "vinpst-asr-helper",
            "args": ["--json"],
            "model": "paraformer",
            "hotwords_file": "/tmp/hotwords.txt",
            "timeout_ms": 1500,
            "env": { "RUST_LOG": "info" }
          }
        ]
      },
      "llm": {
        "providers": [
          {
            "id": "openai",
            "base_url": "https://example.invalid/v1",
            "api_key": "env:OPENAI_API_KEY",
            "model": "gpt-test",
            "extra_body": { "temperature": 0.2 },
            "future_field": "preserved"
          }
        ],
        "adapters": [
          {
            "id": "local-adapter",
            "command": "vinpst-adapter",
            "args": ["serve"],
            "env": { "ADAPTER_MODE": "test" },
            "working_dir": "/tmp"
          }
        ]
      },
      "scenes": {
        "active_scene": "__raw__",
        "definitions": [
          { "id": "__raw__", "label": "Raw", "candidate_count": 0 },
          { "id": "__command__", "label": "Command", "candidate_count": 1 }
        ]
      }
    }
    "#;

    let config = VinpstConfig::from_json_str(input).unwrap();
    config.validate().unwrap();
    let asr = &config.asr.providers[0];
    assert_eq!(asr.command.as_deref(), Some("vinpst-asr-helper"));
    assert_eq!(asr.args, ["--json"]);
    assert_eq!(asr.model.as_deref(), Some("paraformer"));
    assert_eq!(asr.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
    assert_eq!(asr.timeout_ms, Some(1500));
    assert_eq!(asr.env.get("RUST_LOG").map(String::as_str), Some("info"));

    let provider = &config.llm.providers[0];
    assert_eq!(provider.id, "openai");
    assert_eq!(provider.model.as_deref(), Some("gpt-test"));
    assert_eq!(provider.extra_body["temperature"], serde_json::json!(0.2));
    assert_eq!(
        provider.extra["future_field"],
        serde_json::json!("preserved")
    );

    let adapter = &config.llm.adapters[0];
    assert_eq!(adapter.command, "vinpst-adapter");
    assert_eq!(adapter.working_dir.as_deref(), Some("/tmp"));
}

#[test]
fn validation_rejects_command_asr_without_command() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(crate::AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    });

    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidCommandAsrProviderCommand(id) if id == "cmd"
    ));
}

#[test]
fn validation_rejects_empty_asr_provider_model() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers[0].model = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidAsrProviderModelId(id) if id == "sherpa-onnx"
    ));
}

#[test]
fn validation_rejects_empty_asr_provider_hotwords_file() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers[0].hotwords_file = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidAsrProviderHotwordsFile(id) if id == "sherpa-onnx"
    ));
}

#[test]
fn validation_rejects_empty_asr_provider_command_for_non_command_backend() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers[0].command = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidAsrProviderCommand(id) if id == "sherpa-onnx"
    ));
}

#[test]
fn validation_rejects_empty_asr_provider_endpoint() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers[0].endpoint = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidAsrProviderEndpoint(id) if id == "sherpa-onnx"
    ));
}

#[test]
fn validation_rejects_zero_asr_provider_timeout() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers[0].timeout_ms = Some(0);
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidAsrProviderTimeoutMs(id) if id == "sherpa-onnx"
    ));
}

#[test]
fn validation_accepts_positive_asr_provider_timeout() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.providers[0].timeout_ms = Some(1);
    config.validate().unwrap();
}

#[test]
fn validation_rejects_remote_asr_without_endpoint() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "remote".to_owned();
    config.asr.providers.push(crate::AsrProviderConfig {
        id: "remote".to_owned(),
        kind: AsrProviderKind::Remote,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: None,
    });

    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidRemoteAsrProviderEndpoint(id) if id == "remote"
    ));
}

#[test]
fn validation_accepts_remote_asr_with_endpoint() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "remote".to_owned();
    config.asr.providers.push(crate::AsrProviderConfig {
        id: "remote".to_owned(),
        kind: AsrProviderKind::Remote,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        endpoint: Some("https://asr.example.test".to_owned()),
    });

    config.validate().unwrap();
}

#[test]
fn validation_accepts_object_llm_provider_extra_body() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.providers.push(crate::LlmProviderConfig {
        id: "llm".to_owned(),
        base_url: "https://example.invalid/v1".to_owned(),
        api_key: String::new(),
        model: None,
        extra_body: serde_json::json!({"temperature": 0.1}),
        extra: std::collections::HashMap::default(),
    });

    config.validate().unwrap();
}

#[test]
fn validation_rejects_invalid_llm_entries() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.providers.push(crate::LlmProviderConfig {
        id: "llm".to_owned(),
        base_url: "  ".to_owned(),
        api_key: String::new(),
        model: None,
        extra_body: serde_json::json!({}),
        extra: std::collections::HashMap::default(),
    });
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidLlmProviderBaseUrl(id) if id == "llm"
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.providers.push(crate::LlmProviderConfig {
        id: "llm".to_owned(),
        base_url: "https://example.invalid/v1".to_owned(),
        api_key: String::new(),
        model: Some("  ".to_owned()),
        extra_body: serde_json::json!({}),
        extra: std::collections::HashMap::default(),
    });
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidLlmProviderModelId(id) if id == "llm"
    ));
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.providers.push(crate::LlmProviderConfig {
        id: "llm".to_owned(),
        base_url: "https://example.invalid/v1".to_owned(),
        api_key: String::new(),
        model: None,
        extra_body: serde_json::json!(["not", "object"]),
        extra: std::collections::HashMap::default(),
    });
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidLlmProviderExtraBody(id) if id == "llm"
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(crate::LlmAdapterConfig {
        id: "adapter".to_owned(),
        command: "  ".to_owned(),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidLlmAdapterCommand(id) if id == "adapter"
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(crate::LlmAdapterConfig {
        id: "adapter".to_owned(),
        command: "vinpst-adapter".to_owned(),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: Some("  ".to_owned()),
        extra: std::collections::HashMap::default(),
    });
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidLlmAdapterWorkingDir(id) if id == "adapter"
    ));
}

#[test]
fn validation_rejects_invalid_scene_postprocess_fields() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].provider_id = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidSceneProviderId(id) if id == RAW_SCENE_ID
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].model = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidSceneModelId(id) if id == RAW_SCENE_ID
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].prompt = Some("  ".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidScenePrompt(id) if id == RAW_SCENE_ID
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].timeout_ms = Some(0);
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::InvalidSceneTimeoutMs(id) if id == RAW_SCENE_ID
    ));
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].context_lines = 33;
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::TooManyContextLines { scene_id, context_lines }
            if scene_id == RAW_SCENE_ID && context_lines == 33
    ));
}

#[test]
fn validation_accepts_max_context_lines() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].context_lines = 32;
    config.validate().unwrap();
}

#[test]
fn validation_accepts_positive_timeout_ms() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].timeout_ms = Some(1);
    config.validate().unwrap();
}

#[test]
fn scene_effective_timeout_preserves_legacy_default_and_explicit_value() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    let scene = &mut config.scenes.definitions[0];
    scene.timeout_ms = None;
    assert_eq!(scene.effective_timeout_ms(), 4_000);
    scene.timeout_ms = Some(2_500);
    assert_eq!(scene.effective_timeout_ms(), 2_500);
}

#[test]
fn validation_rejects_unknown_scene_provider() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.definitions[0].provider_id = Some("missing-provider".to_owned());
    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::UnknownSceneProviderId { scene_id, provider_id }
            if scene_id == RAW_SCENE_ID && provider_id == "missing-provider"
    ));
}

#[test]
fn validation_rejects_scene_provider_that_only_matches_adapter() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(crate::LlmAdapterConfig {
        id: "adapter-only".to_owned(),
        command: "adapter-helper".to_owned(),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    config.scenes.definitions[0].provider_id = Some("adapter-only".to_owned());

    let error = config.validate().unwrap_err();
    assert!(matches!(
        error,
        crate::ConfigError::UnknownSceneProviderId { scene_id, provider_id }
            if scene_id == RAW_SCENE_ID && provider_id == "adapter-only"
    ));
}
