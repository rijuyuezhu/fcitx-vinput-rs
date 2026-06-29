//! Golden tests for explicit legacy config compatibility policy decisions.

use std::collections::HashMap;

use vinput_config::{
    AsrProviderConfig, AsrProviderKind, ConfigError, LlmAdapterConfig, LlmProviderConfig,
    RAW_SCENE_ID, VinputConfig,
};

fn bundled() -> VinputConfig {
    VinputConfig::bundled_default().expect("bundled default config should parse")
}

fn llm_provider(id: &str) -> LlmProviderConfig {
    LlmProviderConfig {
        id: id.to_owned(),
        base_url: "https://llm.example.invalid/v1".to_owned(),
        api_key: String::new(),
        model: None,
        extra_body: serde_json::json!({}),
        extra: HashMap::new(),
    }
}

fn llm_adapter(id: &str) -> LlmAdapterConfig {
    LlmAdapterConfig {
        id: id.to_owned(),
        command: "vinput-adapter".to_owned(),
        args: Vec::new(),
        env: HashMap::new(),
        working_dir: None,
        extra: HashMap::new(),
    }
}

fn command_asr_provider(id: &str, command: Option<&str>) -> AsrProviderConfig {
    AsrProviderConfig {
        id: id.to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: command.map(str::to_owned),
        args: Vec::new(),
        env: HashMap::new(),
        endpoint: None,
    }
}

#[test]
fn legacy_registry_mirror_repair_is_not_implicit() {
    let mut duplicate = bundled();
    let mirror = duplicate.registry.base_urls[0].clone();
    duplicate.registry.base_urls.push(mirror.clone());
    assert!(matches!(
        duplicate.validate().unwrap_err(),
        ConfigError::DuplicateRegistryBaseUrl(url) if url == mirror
    ));

    let mut empty = bundled();
    empty.registry.base_urls.push("  ".to_owned());
    assert!(matches!(
        empty.validate().unwrap_err(),
        ConfigError::InvalidRegistryBaseUrl(url) if url == "  "
    ));
}

#[test]
fn legacy_provider_and_adapter_id_repair_is_not_implicit() {
    let mut duplicate_llm_provider = bundled();
    duplicate_llm_provider
        .llm
        .providers
        .push(llm_provider("llm"));
    duplicate_llm_provider
        .llm
        .providers
        .push(llm_provider("llm"));
    assert!(matches!(
        duplicate_llm_provider.validate().unwrap_err(),
        ConfigError::DuplicateLlmProviderId(id) if id == "llm"
    ));

    let mut empty_llm_provider = bundled();
    empty_llm_provider.llm.providers.push(llm_provider("  "));
    assert!(matches!(
        empty_llm_provider.validate().unwrap_err(),
        ConfigError::InvalidLlmProviderId(id) if id == "  "
    ));

    let mut duplicate_adapter = bundled();
    duplicate_adapter.llm.adapters.push(llm_adapter("adapter"));
    duplicate_adapter.llm.adapters.push(llm_adapter("adapter"));
    assert!(matches!(
        duplicate_adapter.validate().unwrap_err(),
        ConfigError::DuplicateLlmAdapterId(id) if id == "adapter"
    ));

    let mut empty_adapter = bundled();
    empty_adapter.llm.adapters.push(llm_adapter("  "));
    assert!(matches!(
        empty_adapter.validate().unwrap_err(),
        ConfigError::InvalidLlmAdapterId(id) if id == "  "
    ));

    let mut duplicate_asr = bundled();
    let duplicate = duplicate_asr.asr.providers[0].clone();
    duplicate_asr.asr.providers.push(duplicate);
    assert!(matches!(
        duplicate_asr.validate().unwrap_err(),
        ConfigError::DuplicateAsrProviderId(id) if id == "sherpa-onnx"
    ));

    let mut empty_asr = bundled();
    empty_asr
        .asr
        .providers
        .push(command_asr_provider("  ", Some("helper")));
    assert!(matches!(
        empty_asr.validate().unwrap_err(),
        ConfigError::InvalidAsrProviderId(id) if id == "  "
    ));
}

#[test]
fn legacy_command_asr_without_command_is_strictly_rejected() {
    let mut config = bundled();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(command_asr_provider("cmd", None));

    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::InvalidCommandAsrProviderCommand(id) if id == "cmd"
    ));
}

#[test]
fn legacy_scene_bounds_are_strict_not_clamped() {
    let mut candidates = bundled();
    candidates.scenes.definitions[0].candidate_count = 33;
    assert!(matches!(
        candidates.validate().unwrap_err(),
        ConfigError::TooManyCandidates { scene_id, candidate_count }
            if scene_id == RAW_SCENE_ID && candidate_count == 33
    ));

    let mut timeout = bundled();
    timeout.scenes.definitions[0].timeout_ms = Some(0);
    assert!(matches!(
        timeout.validate().unwrap_err(),
        ConfigError::InvalidSceneTimeoutMs(id) if id == RAW_SCENE_ID
    ));

    let mut context = bundled();
    context.scenes.definitions[0].context_lines = 33;
    assert!(matches!(
        context.validate().unwrap_err(),
        ConfigError::TooManyContextLines { scene_id, context_lines }
            if scene_id == RAW_SCENE_ID && context_lines == 33
    ));
}

#[test]
fn legacy_missing_active_scene_and_provider_are_strictly_rejected() {
    let mut scene = bundled();
    scene.scenes.active_scene = "missing".to_owned();
    assert!(matches!(
        scene.validate().unwrap_err(),
        ConfigError::UnknownActiveScene(id) if id == "missing"
    ));

    let mut provider = bundled();
    provider.asr.active_provider = "missing".to_owned();
    assert!(matches!(
        provider.validate().unwrap_err(),
        ConfigError::UnknownActiveAsrProvider(id) if id == "missing"
    ));
}

#[test]
fn legacy_scene_provider_prompt_and_model_policy_is_explicit() {
    let mut unknown_provider = bundled();
    unknown_provider.scenes.definitions[0].provider_id = Some("missing".to_owned());
    assert!(matches!(
        unknown_provider.validate().unwrap_err(),
        ConfigError::UnknownSceneProviderId { scene_id, provider_id }
            if scene_id == RAW_SCENE_ID && provider_id == "missing"
    ));

    let mut empty_model = bundled();
    empty_model.scenes.definitions[0].model = Some("  ".to_owned());
    assert!(matches!(
        empty_model.validate().unwrap_err(),
        ConfigError::InvalidSceneModelId(id) if id == RAW_SCENE_ID
    ));

    let mut empty_prompt = bundled();
    empty_prompt.scenes.definitions[0].prompt = Some("  ".to_owned());
    assert!(matches!(
        empty_prompt.validate().unwrap_err(),
        ConfigError::InvalidScenePrompt(id) if id == RAW_SCENE_ID
    ));

    let mut model_without_provider = bundled();
    model_without_provider.scenes.definitions[0].model = Some("model-id".to_owned());
    model_without_provider.validate().unwrap();

    let mut prompt_without_provider = bundled();
    prompt_without_provider.scenes.definitions[0].prompt = Some("Polish: {{text}}".to_owned());
    prompt_without_provider.validate().unwrap();
}
