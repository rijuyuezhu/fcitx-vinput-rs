//! Configuration model and validation for vinput.
//!
//! The first implementation preserves the original `default-config.json` shape
//! and focuses on typed deserialization plus lightweight validation. Later
//! migrations can add versioned upgrades here without touching daemon code.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_protocol::CandidateSource;

/// Built-in raw scene id used by the legacy project.
pub const RAW_SCENE_ID: &str = "__raw__";

/// Built-in command scene id used by the legacy project.
pub const COMMAND_SCENE_ID: &str = "__command__";

/// Complete config document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VinputConfig {
    /// Config schema version.
    pub version: u32,
    /// Registry mirror settings.
    #[serde(default)]
    pub registry: RegistryConfig,
    /// Global daemon and UI defaults.
    #[serde(default)]
    pub global: GlobalConfig,
    /// ASR settings.
    #[serde(default)]
    pub asr: AsrConfig,
    /// LLM provider/adapter settings.
    #[serde(default)]
    pub llm: LlmConfig,
    /// Scene selection and definitions.
    #[serde(default)]
    pub scenes: ScenesConfig,
}

impl VinputConfig {
    /// Parses config from JSON.
    pub fn from_json_str(input: &str) -> Result<Self, ConfigError> {
        Ok(serde_json::from_str::<Self>(input)?.normalized())
    }

    /// Reads and parses config from a JSON file.
    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&input)
    }

    /// Parses the bundled upstream-compatible default config.
    pub fn bundled_default() -> Result<Self, ConfigError> {
        Self::from_json_str(include_str!("../../../data/default-config.json"))
    }

    /// Applies non-destructive defaults for optional sections.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.version == 0 {
            self.version = 1;
        }
        if self.scenes.active_scene.is_empty() {
            RAW_SCENE_ID.clone_into(&mut self.scenes.active_scene);
        }
        ensure_builtin_scenes(&mut self.scenes.definitions);
        self
    }

    /// Validates cross-field invariants that serde cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_registry(&self.registry)?;
        validate_global(&self.global)?;
        validate_scenes(&self.scenes, &self.llm)?;
        validate_asr(&self.asr)?;
        validate_llm(&self.llm)?;
        Ok(())
    }

    /// Builds a compact summary for CLI and diagnostics.
    #[must_use]
    pub fn summary(&self) -> VinputConfigSummary {
        VinputConfigSummary {
            ok: true,
            version: self.version,
            active_scene: self.scenes.active_scene.clone(),
            active_provider: self.asr.active_provider.clone(),
            scene_count: self.scenes.definitions.len(),
            provider_count: self.asr.providers.len(),
            registry_mirror_count: self.registry.base_urls.len(),
        }
    }

    /// Returns the active scene definition, if it exists.
    #[must_use]
    pub fn active_scene(&self) -> Option<&SceneDefinition> {
        self.scenes
            .definitions
            .iter()
            .find(|scene| scene.id == self.scenes.active_scene)
    }
}

fn ensure_builtin_scenes(definitions: &mut Vec<SceneDefinition>) {
    if !definitions.iter().any(|scene| scene.id == RAW_SCENE_ID) {
        definitions.push(SceneDefinition {
            id: RAW_SCENE_ID.to_owned(),
            label: "__label_raw__".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 0,
            timeout_ms: None,
            context_lines: 0,
        });
    }
    if !definitions.iter().any(|scene| scene.id == COMMAND_SCENE_ID) {
        definitions.push(SceneDefinition {
            id: COMMAND_SCENE_ID.to_owned(),
            label: "__label_command__".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    }
}

fn validate_registry(registry: &RegistryConfig) -> Result<(), ConfigError> {
    let mut registry_base_urls = HashSet::new();
    for base_url in &registry.base_urls {
        if base_url.trim().is_empty() {
            return Err(ConfigError::InvalidRegistryBaseUrl(base_url.clone()));
        }
        if !registry_base_urls.insert(base_url.as_str()) {
            return Err(ConfigError::DuplicateRegistryBaseUrl(base_url.clone()));
        }
    }
    Ok(())
}

fn validate_global(global: &GlobalConfig) -> Result<(), ConfigError> {
    if global.default_language.trim().is_empty() {
        return Err(ConfigError::InvalidDefaultLanguage);
    }
    if global.capture_device.trim().is_empty() {
        return Err(ConfigError::InvalidCaptureDevice);
    }
    Ok(())
}

fn validate_scenes(scenes: &ScenesConfig, llm: &LlmConfig) -> Result<(), ConfigError> {
    let mut scene_ids = HashSet::new();
    for scene in &scenes.definitions {
        validate_scene_definition(scene, &mut scene_ids, llm)?;
    }

    if !scene_ids.contains(scenes.active_scene.as_str()) {
        return Err(ConfigError::UnknownActiveScene(scenes.active_scene.clone()));
    }
    Ok(())
}

fn validate_scene_definition<'a>(
    scene: &'a SceneDefinition,
    scene_ids: &mut HashSet<&'a str>,
    llm: &LlmConfig,
) -> Result<(), ConfigError> {
    if scene.id.trim().is_empty() {
        return Err(ConfigError::InvalidSceneId(scene.id.clone()));
    }
    if scene.label.trim().is_empty() {
        return Err(ConfigError::InvalidSceneLabel(scene.id.clone()));
    }
    if !scene_ids.insert(scene.id.as_str()) {
        return Err(ConfigError::DuplicateSceneId(scene.id.clone()));
    }
    if scene.candidate_count > 32 {
        return Err(ConfigError::TooManyCandidates {
            scene_id: scene.id.clone(),
            candidate_count: scene.candidate_count,
        });
    }
    if let Some(provider_id) = &scene.provider_id {
        if provider_id.trim().is_empty() {
            return Err(ConfigError::InvalidSceneProviderId(scene.id.clone()));
        }
        if !llm
            .providers
            .iter()
            .any(|provider| provider.id == *provider_id)
        {
            return Err(ConfigError::UnknownSceneProviderId {
                scene_id: scene.id.clone(),
                provider_id: provider_id.clone(),
            });
        }
    }
    if scene
        .model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(ConfigError::InvalidSceneModelId(scene.id.clone()));
    }
    if scene
        .prompt
        .as_deref()
        .is_some_and(|prompt| prompt.trim().is_empty())
    {
        return Err(ConfigError::InvalidScenePrompt(scene.id.clone()));
    }

    if scene.timeout_ms == Some(0) {
        return Err(ConfigError::InvalidSceneTimeoutMs(scene.id.clone()));
    }
    if scene.context_lines > 32 {
        return Err(ConfigError::TooManyContextLines {
            scene_id: scene.id.clone(),
            context_lines: scene.context_lines,
        });
    }
    Ok(())
}

fn validate_asr(asr: &AsrConfig) -> Result<(), ConfigError> {
    let mut provider_ids = HashSet::new();
    for provider in &asr.providers {
        validate_asr_provider(provider, &mut provider_ids)?;
    }

    if asr.active_provider.trim().is_empty() {
        return Err(ConfigError::InvalidActiveAsrProviderId);
    }

    if !asr.providers.is_empty() && !provider_ids.contains(asr.active_provider.as_str()) {
        return Err(ConfigError::UnknownActiveAsrProvider(
            asr.active_provider.clone(),
        ));
    }
    Ok(())
}

fn validate_asr_provider<'a>(
    provider: &'a AsrProviderConfig,
    provider_ids: &mut HashSet<&'a str>,
) -> Result<(), ConfigError> {
    if provider.id.trim().is_empty() {
        return Err(ConfigError::InvalidAsrProviderId(provider.id.clone()));
    }
    if !provider_ids.insert(provider.id.as_str()) {
        return Err(ConfigError::DuplicateAsrProviderId(provider.id.clone()));
    }
    if provider
        .model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderModelId(provider.id.clone()));
    }
    if provider
        .hotwords_file
        .as_deref()
        .is_some_and(|hotwords_file| hotwords_file.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderHotwordsFile(
            provider.id.clone(),
        ));
    }
    if provider.kind != AsrProviderKind::Command
        && provider
            .command
            .as_deref()
            .is_some_and(|command| command.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderCommand(provider.id.clone()));
    }
    if provider
        .endpoint
        .as_deref()
        .is_some_and(|endpoint| endpoint.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderEndpoint(provider.id.clone()));
    }
    if provider.timeout_ms == Some(0) {
        return Err(ConfigError::InvalidAsrProviderTimeoutMs(
            provider.id.clone(),
        ));
    }
    if provider.kind == AsrProviderKind::Command
        && provider
            .command
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(ConfigError::InvalidCommandAsrProviderCommand(
            provider.id.clone(),
        ));
    }
    if provider.kind == AsrProviderKind::Remote
        && provider
            .endpoint
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(ConfigError::InvalidRemoteAsrProviderEndpoint(
            provider.id.clone(),
        ));
    }
    for key in provider.env.keys() {
        if key.trim().is_empty() {
            return Err(ConfigError::InvalidProviderEnvKey {
                provider_id: provider.id.clone(),
                key: key.clone(),
            });
        }
    }
    Ok(())
}

fn validate_llm(llm: &LlmConfig) -> Result<(), ConfigError> {
    let mut llm_provider_ids = HashSet::new();
    for provider in &llm.providers {
        validate_llm_provider(provider, &mut llm_provider_ids)?;
    }

    let mut adapter_ids = HashSet::new();
    for adapter in &llm.adapters {
        validate_llm_adapter(adapter, &mut adapter_ids)?;
    }
    Ok(())
}

fn validate_llm_provider<'a>(
    provider: &'a LlmProviderConfig,
    provider_ids: &mut HashSet<&'a str>,
) -> Result<(), ConfigError> {
    if provider.id.trim().is_empty() {
        return Err(ConfigError::InvalidLlmProviderId(provider.id.clone()));
    }
    if !provider_ids.insert(provider.id.as_str()) {
        return Err(ConfigError::DuplicateLlmProviderId(provider.id.clone()));
    }
    if provider.base_url.trim().is_empty() {
        return Err(ConfigError::InvalidLlmProviderBaseUrl(provider.id.clone()));
    }
    if provider
        .model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(ConfigError::InvalidLlmProviderModelId(provider.id.clone()));
    }
    if !provider.extra_body.is_object() {
        return Err(ConfigError::InvalidLlmProviderExtraBody(
            provider.id.clone(),
        ));
    }
    Ok(())
}

fn validate_llm_adapter<'a>(
    adapter: &'a LlmAdapterConfig,
    adapter_ids: &mut HashSet<&'a str>,
) -> Result<(), ConfigError> {
    if adapter.id.trim().is_empty() {
        return Err(ConfigError::InvalidLlmAdapterId(adapter.id.clone()));
    }
    if !adapter_ids.insert(adapter.id.as_str()) {
        return Err(ConfigError::DuplicateLlmAdapterId(adapter.id.clone()));
    }
    if adapter.command.trim().is_empty() {
        return Err(ConfigError::InvalidLlmAdapterCommand(adapter.id.clone()));
    }
    if adapter
        .working_dir
        .as_deref()
        .is_some_and(|working_dir| working_dir.trim().is_empty())
    {
        return Err(ConfigError::InvalidLlmAdapterWorkingDir(adapter.id.clone()));
    }
    for key in adapter.env.keys() {
        if key.trim().is_empty() {
            return Err(ConfigError::InvalidLlmAdapterEnvKey {
                adapter_id: adapter.id.clone(),
                key: key.clone(),
            });
        }
    }
    Ok(())
}

/// Compact config summary for CLI and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VinputConfigSummary {
    /// Whether validation succeeded.
    pub ok: bool,
    /// Config schema version.
    pub version: u32,
    /// Active scene id.
    pub active_scene: String,
    /// Active ASR provider id.
    pub active_provider: String,
    /// Number of configured scenes.
    pub scene_count: usize,
    /// Number of configured ASR providers.
    pub provider_count: usize,
    /// Number of configured registry mirrors.
    pub registry_mirror_count: usize,
}

/// Registry mirror settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct RegistryConfig {
    /// Ordered registry base URLs.
    #[serde(default)]
    pub base_urls: Vec<String>,
}

/// Global daemon/UI defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GlobalConfig {
    /// Default recognition language.
    #[serde(default = "default_language")]
    pub default_language: String,
    /// `PipeWire` target capture device.
    #[serde(default = "default_capture_device")]
    pub capture_device: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            default_language: default_language(),
            capture_device: default_capture_device(),
        }
    }
}

/// ASR settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AsrConfig {
    /// Selected provider id.
    #[serde(default = "default_asr_provider")]
    pub active_provider: String,
    /// Whether captured audio should be normalized before recognition.
    #[serde(default = "default_true")]
    pub normalize_audio: bool,
    /// Input gain applied before ASR.
    #[serde(default = "default_input_gain")]
    pub input_gain: f32,
    /// VAD settings.
    #[serde(default)]
    pub vad: VadConfig,
    /// Known providers.
    #[serde(default)]
    pub providers: Vec<AsrProviderConfig>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            active_provider: default_asr_provider(),
            normalize_audio: true,
            input_gain: default_input_gain(),
            vad: VadConfig::default(),
            providers: Vec::new(),
        }
    }
}

/// VAD settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VadConfig {
    /// Whether VAD trimming is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// ASR provider type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AsrProviderKind {
    /// Local backend, usually sherpa-onnx.
    Local,
    /// Remote HTTP/WebSocket backend.
    Remote,
    /// External command backend.
    Command,
}

/// ASR provider config entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AsrProviderConfig {
    /// Stable provider id.
    pub id: String,
    /// Backend kind.
    #[serde(rename = "type")]
    pub kind: AsrProviderKind,
    /// Provider timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Optional model id selected for this provider.
    #[serde(default)]
    pub model: Option<String>,
    /// Optional hotwords file for local ASR backends.
    #[serde(default)]
    pub hotwords_file: Option<String>,
    /// External command used by command ASR providers.
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments passed to the external command ASR provider.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables passed to the external command ASR provider.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Optional endpoint label or URL for remote ASR providers.
    #[serde(default)]
    pub endpoint: Option<String>,
}

/// LLM provider/adapter config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct LlmConfig {
    /// Provider entries used by scene and command post-processing.
    #[serde(default)]
    pub providers: Vec<LlmProviderConfig>,
    /// Adapter process entries used by local/remote text adapters.
    #[serde(default)]
    pub adapters: Vec<LlmAdapterConfig>,
}

/// OpenAI-compatible or adapter-backed LLM provider config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmProviderConfig {
    /// Stable provider id.
    pub id: String,
    /// Base URL for OpenAI-compatible providers.
    #[serde(default)]
    pub base_url: String,
    /// API key or environment-reference expression.
    #[serde(default)]
    pub api_key: String,
    /// Optional default model name.
    #[serde(default)]
    pub model: Option<String>,
    /// Extra JSON body merged into provider requests.
    #[serde(default = "default_json_object")]
    pub extra_body: serde_json::Value,
    /// Forward-compatible unknown provider fields.
    #[serde(default, flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// External text adapter process config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmAdapterConfig {
    /// Stable adapter id.
    pub id: String,
    /// Adapter executable path or command name.
    pub command: String,
    /// Arguments passed to the adapter process.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables passed to the adapter process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Optional working directory for the adapter process.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Forward-compatible unknown adapter fields.
    #[serde(default, flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Scene collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenesConfig {
    /// Selected scene id.
    #[serde(default = "default_active_scene")]
    pub active_scene: String,
    /// Known scenes.
    #[serde(default)]
    pub definitions: Vec<SceneDefinition>,
}

impl Default for ScenesConfig {
    fn default() -> Self {
        Self {
            active_scene: default_active_scene(),
            definitions: Vec::new(),
        }
    }
}

/// A post-processing scene definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneDefinition {
    /// Stable scene id.
    pub id: String,
    /// Translation key or display label.
    pub label: String,
    /// Optional prompt template.
    #[serde(default)]
    pub prompt: Option<String>,
    /// LLM provider id used for post-processing.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional model override for this scene.
    #[serde(default)]
    pub model: Option<String>,
    /// Number of result candidates to ask the post-processor for.
    #[serde(default)]
    pub candidate_count: u8,
    /// Optional per-scene timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Number of recent input context lines to include.
    #[serde(default)]
    pub context_lines: u8,
}

impl SceneDefinition {
    /// Candidate source expected for this scene when no LLM is needed.
    #[must_use]
    pub fn default_candidate_source(&self) -> CandidateSource {
        if self.id == RAW_SCENE_ID {
            CandidateSource::Raw
        } else {
            CandidateSource::Llm
        }
    }
}

/// Config errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// JSON parsing failed.
    #[error("invalid config JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Reading a config file failed.
    #[error("failed to read config file `{}`: {source}", path.display())]
    ReadFile {
        /// Config file path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Registry base URL is empty.
    #[error("invalid empty registry base URL")]
    InvalidRegistryBaseUrl(String),
    /// Registry base URL is duplicated.
    #[error("duplicate registry base URL `{0}`")]
    DuplicateRegistryBaseUrl(String),
    /// Default language is empty.
    #[error("invalid empty default language")]
    InvalidDefaultLanguage,
    /// Capture device is empty.
    #[error("invalid empty capture device")]
    InvalidCaptureDevice,
    /// Active scene is not listed in scene definitions.
    #[error("active scene `{0}` is not defined")]
    UnknownActiveScene(String),
    /// Active ASR provider id is empty.
    #[error("invalid empty active ASR provider id")]
    InvalidActiveAsrProviderId,
    /// Active ASR provider is not listed in provider definitions.
    #[error("active ASR provider `{0}` is not defined")]
    UnknownActiveAsrProvider(String),
    /// Empty scene id.
    #[error("invalid empty scene id")]
    InvalidSceneId(String),
    /// Empty scene label.
    #[error("invalid empty scene label for scene `{0}`")]
    InvalidSceneLabel(String),
    /// Duplicate scene id.
    #[error("duplicate scene id `{0}`")]
    DuplicateSceneId(String),
    /// Scene provider id is present but empty.
    #[error("scene `{0}` has an invalid empty provider id")]
    InvalidSceneProviderId(String),
    /// Scene model id is present but empty.
    #[error("scene `{0}` has an invalid empty model id")]
    InvalidSceneModelId(String),
    /// Scene prompt is present but empty.
    #[error("scene `{0}` has an invalid empty prompt")]
    InvalidScenePrompt(String),
    /// Scene provider id does not match a configured LLM provider.
    #[error("scene `{scene_id}` references unknown LLM provider `{provider_id}`")]
    UnknownSceneProviderId {
        /// Scene id.
        scene_id: String,
        /// Missing provider id.
        provider_id: String,
    },
    /// Scene timeout must be positive when configured.
    #[error("scene `{0}` has invalid timeout_ms 0")]
    InvalidSceneTimeoutMs(String),
    /// Scene asks for too many recent context lines.
    #[error("scene `{scene_id}` asks for {context_lines} context lines, max is 32")]
    TooManyContextLines {
        /// Scene id.
        scene_id: String,
        /// Requested context lines.
        context_lines: u8,
    },
    /// Empty ASR provider id.
    #[error("invalid empty ASR provider id")]
    InvalidAsrProviderId(String),
    /// Duplicate ASR provider id.
    #[error("duplicate ASR provider id `{0}`")]
    DuplicateAsrProviderId(String),
    /// ASR provider model id is present but empty.
    #[error("ASR provider `{0}` has an invalid empty model id")]
    InvalidAsrProviderModelId(String),
    /// ASR provider hotwords file is present but empty.
    #[error("ASR provider `{0}` has an invalid empty hotwords_file")]
    InvalidAsrProviderHotwordsFile(String),
    /// ASR provider command is present but empty for a non-command backend.
    #[error("ASR provider `{0}` has an invalid empty command")]
    InvalidAsrProviderCommand(String),
    /// ASR provider endpoint is present but empty.
    #[error("ASR provider `{0}` has an invalid empty endpoint")]
    InvalidAsrProviderEndpoint(String),
    /// ASR provider timeout must be positive when configured.
    #[error("ASR provider `{0}` has invalid timeout_ms 0")]
    InvalidAsrProviderTimeoutMs(String),
    /// Command ASR provider requires a command.
    #[error("command ASR provider `{0}` must configure a command")]
    InvalidCommandAsrProviderCommand(String),
    /// Remote ASR provider requires an endpoint.
    #[error("remote ASR provider `{0}` must configure an endpoint")]
    InvalidRemoteAsrProviderEndpoint(String),
    /// Provider environment contains an empty key.
    #[error("provider `{provider_id}` has an invalid environment key `{key}`")]
    InvalidProviderEnvKey {
        /// Provider id.
        provider_id: String,
        /// Invalid environment key.
        key: String,
    },
    /// Empty LLM provider id.
    #[error("invalid empty LLM provider id")]
    InvalidLlmProviderId(String),
    /// Duplicate LLM provider id.
    #[error("duplicate LLM provider id `{0}`")]
    DuplicateLlmProviderId(String),
    /// LLM provider base URL is empty.
    #[error("LLM provider `{0}` must configure a base URL")]
    InvalidLlmProviderBaseUrl(String),
    /// LLM provider model id is present but empty.
    #[error("LLM provider `{0}` has an invalid empty model id")]
    InvalidLlmProviderModelId(String),
    /// LLM provider `extra_body` must be a JSON object.
    #[error("LLM provider `{0}` has invalid non-object extra_body")]
    InvalidLlmProviderExtraBody(String),
    /// Empty LLM adapter id.
    #[error("invalid empty LLM adapter id")]
    InvalidLlmAdapterId(String),
    /// Duplicate LLM adapter id.
    #[error("duplicate LLM adapter id `{0}`")]
    DuplicateLlmAdapterId(String),
    /// LLM adapter command is empty.
    #[error("LLM adapter `{0}` must configure a command")]
    InvalidLlmAdapterCommand(String),
    /// LLM adapter working directory is present but empty.
    #[error("LLM adapter `{0}` has an invalid empty working_dir")]
    InvalidLlmAdapterWorkingDir(String),
    /// LLM adapter environment contains an empty key.
    #[error("LLM adapter `{adapter_id}` has an invalid environment key `{key}`")]
    InvalidLlmAdapterEnvKey {
        /// Adapter id.
        adapter_id: String,
        /// Invalid environment key.
        key: String,
    },
    /// Candidate count is above the safety cap.
    #[error("scene `{scene_id}` asks for {candidate_count} candidates, max is 32")]
    TooManyCandidates {
        /// Scene id.
        scene_id: String,
        /// Requested candidate count.
        candidate_count: u8,
    },
}

fn default_language() -> String {
    "zh".to_owned()
}

fn default_capture_device() -> String {
    "default".to_owned()
}

fn default_asr_provider() -> String {
    "sherpa-onnx".to_owned()
}

fn default_active_scene() -> String {
    RAW_SCENE_ID.to_owned()
}

const fn default_true() -> bool {
    true
}

const fn default_input_gain() -> f32 {
    1.0
}

fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}

#[cfg(test)]
mod tests {
    use super::{AsrProviderKind, COMMAND_SCENE_ID, RAW_SCENE_ID, VinputConfig};
    use vinput_protocol::CandidateSource;

    #[test]
    fn config_file_parses_and_normalizes() {
        let path = std::env::temp_dir().join(format!(
            "vinput-config-test-{}-file.json",
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

        let config = VinputConfig::from_json_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.asr.active_provider, "p");
        config.validate().unwrap();
    }

    #[test]
    fn config_file_reports_read_errors() {
        let path = std::env::temp_dir().join(format!(
            "vinput-config-test-{}-missing.json",
            std::process::id()
        ));

        let error = VinputConfig::from_json_file(&path).unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::ReadFile { path: error_path, .. } if error_path == path
        ));
    }

    #[test]
    fn normalization_inserts_legacy_builtin_scenes_for_minimal_configs() {
        let config = VinputConfig::from_json_str(
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
    }

    #[test]
    fn normalization_preserves_existing_builtin_scene_definitions() {
        let config = VinputConfig::from_json_str(
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
        let disk_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/default-config.json");
        let disk_json = std::fs::read_to_string(&disk_path).unwrap();

        let disk_config = VinputConfig::from_json_str(&disk_json).unwrap();
        let bundled_config = VinputConfig::bundled_default().unwrap();

        assert_eq!(disk_config, bundled_config);
        disk_config.validate().unwrap();
    }

    #[test]
    fn bundled_default_parses_and_validates() {
        let config = VinputConfig::bundled_default().unwrap();
        config.validate().unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.global.default_language, "zh");
        assert_eq!(config.asr.active_provider, "sherpa-onnx");
        assert_eq!(config.asr.providers[0].kind, AsrProviderKind::Local);
        assert_eq!(config.scenes.active_scene, RAW_SCENE_ID);
        assert_eq!(config.active_scene().unwrap().id, RAW_SCENE_ID);
    }

    #[test]
    fn scene_source_policy_is_explicit() {
        let config = VinputConfig::bundled_default().unwrap();
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
        let config = VinputConfig::bundled_default().unwrap();
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
    fn validation_rejects_duplicate_registry_base_urls() {
        let mut config = VinputConfig::bundled_default().unwrap();
        let duplicate = config.registry.base_urls[0].clone();
        config.registry.base_urls.push(duplicate.clone());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::DuplicateRegistryBaseUrl(url) if url == duplicate
        ));
    }

    #[test]
    fn validation_rejects_empty_registry_base_urls() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.registry.base_urls.push("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidRegistryBaseUrl(url) if url == "  "
        ));
    }

    #[test]
    fn validation_rejects_empty_capture_device() {
        let mut c = VinputConfig::bundled_default().unwrap();
        c.global.capture_device = "  ".to_owned();
        assert!(matches!(
            c.validate().unwrap_err(),
            super::ConfigError::InvalidCaptureDevice
        ));
    }

    #[test]
    fn validation_rejects_empty_default_language() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.global.default_language = "  ".to_owned();
        let error = config.validate().unwrap_err();
        assert!(matches!(error, super::ConfigError::InvalidDefaultLanguage));
    }

    #[test]
    fn validation_rejects_empty_active_provider() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "  ".to_owned();
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidActiveAsrProviderId
        ));
    }

    #[test]
    fn validation_rejects_duplicate_scene_ids() {
        let mut config = VinputConfig::bundled_default().unwrap();
        let duplicate = config.scenes.definitions[0].clone();
        config.scenes.definitions.push(duplicate);
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::DuplicateSceneId(id) if id == RAW_SCENE_ID
        ));
    }

    #[test]
    fn validation_rejects_empty_scene_labels() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].label = "  ".to_owned();
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidSceneLabel(id) if id == RAW_SCENE_ID
        ));
    }

    #[test]
    fn validation_rejects_duplicate_asr_provider_ids() {
        let mut config = VinputConfig::bundled_default().unwrap();
        let duplicate = config.asr.providers[0].clone();
        config.asr.providers.push(duplicate);
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::DuplicateAsrProviderId(id) if id == "sherpa-onnx"
        ));
    }

    #[test]
    fn validation_rejects_missing_active_scene() {
        let mut config = VinputConfig::bundled_default().unwrap();
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
                "command": "vinput-asr-helper",
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
                "command": "vinput-adapter",
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

        let config = VinputConfig::from_json_str(input).unwrap();
        config.validate().unwrap();
        let asr = &config.asr.providers[0];
        assert_eq!(asr.command.as_deref(), Some("vinput-asr-helper"));
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
        assert_eq!(adapter.command, "vinput-adapter");
        assert_eq!(adapter.working_dir.as_deref(), Some("/tmp"));
    }

    #[test]
    fn validation_rejects_command_asr_without_command() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "cmd".to_owned();
        config.asr.providers.push(super::AsrProviderConfig {
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
            super::ConfigError::InvalidCommandAsrProviderCommand(id) if id == "cmd"
        ));
    }

    #[test]
    fn validation_rejects_empty_asr_provider_model() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers[0].model = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidAsrProviderModelId(id) if id == "sherpa-onnx"
        ));
    }

    #[test]
    fn validation_rejects_empty_asr_provider_hotwords_file() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers[0].hotwords_file = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidAsrProviderHotwordsFile(id) if id == "sherpa-onnx"
        ));
    }

    #[test]
    fn validation_rejects_empty_asr_provider_command_for_non_command_backend() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers[0].command = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidAsrProviderCommand(id) if id == "sherpa-onnx"
        ));
    }

    #[test]
    fn validation_rejects_empty_asr_provider_endpoint() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers[0].endpoint = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidAsrProviderEndpoint(id) if id == "sherpa-onnx"
        ));
    }

    #[test]
    fn validation_rejects_zero_asr_provider_timeout() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers[0].timeout_ms = Some(0);
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidAsrProviderTimeoutMs(id) if id == "sherpa-onnx"
        ));
    }

    #[test]
    fn validation_accepts_positive_asr_provider_timeout() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers[0].timeout_ms = Some(1);
        config.validate().unwrap();
    }

    #[test]
    fn validation_rejects_remote_asr_without_endpoint() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "remote".to_owned();
        config.asr.providers.push(super::AsrProviderConfig {
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
            super::ConfigError::InvalidRemoteAsrProviderEndpoint(id) if id == "remote"
        ));
    }

    #[test]
    fn validation_accepts_remote_asr_with_endpoint() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.active_provider = "remote".to_owned();
        config.asr.providers.push(super::AsrProviderConfig {
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
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.providers.push(super::LlmProviderConfig {
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
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.providers.push(super::LlmProviderConfig {
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
            super::ConfigError::InvalidLlmProviderBaseUrl(id) if id == "llm"
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.providers.push(super::LlmProviderConfig {
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
            super::ConfigError::InvalidLlmProviderModelId(id) if id == "llm"
        ));
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.providers.push(super::LlmProviderConfig {
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
            super::ConfigError::InvalidLlmProviderExtraBody(id) if id == "llm"
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(super::LlmAdapterConfig {
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
            super::ConfigError::InvalidLlmAdapterCommand(id) if id == "adapter"
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(super::LlmAdapterConfig {
            id: "adapter".to_owned(),
            command: "vinput-adapter".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: Some("  ".to_owned()),
            extra: std::collections::HashMap::default(),
        });
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidLlmAdapterWorkingDir(id) if id == "adapter"
        ));
    }

    #[test]
    fn validation_rejects_invalid_scene_postprocess_fields() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].provider_id = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidSceneProviderId(id) if id == RAW_SCENE_ID
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].model = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidSceneModelId(id) if id == RAW_SCENE_ID
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].prompt = Some("  ".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidScenePrompt(id) if id == RAW_SCENE_ID
        ));

        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].timeout_ms = Some(0);
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::InvalidSceneTimeoutMs(id) if id == RAW_SCENE_ID
        ));
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].context_lines = 33;
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::TooManyContextLines { scene_id, context_lines }
                if scene_id == RAW_SCENE_ID && context_lines == 33
        ));
    }

    #[test]
    fn validation_accepts_max_context_lines() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].context_lines = 32;
        config.validate().unwrap();
    }

    #[test]
    fn validation_accepts_positive_timeout_ms() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].timeout_ms = Some(1);
        config.validate().unwrap();
    }

    #[test]
    fn validation_rejects_unknown_scene_provider() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.scenes.definitions[0].provider_id = Some("missing-provider".to_owned());
        let error = config.validate().unwrap_err();
        assert!(matches!(
            error,
            super::ConfigError::UnknownSceneProviderId { scene_id, provider_id }
                if scene_id == RAW_SCENE_ID && provider_id == "missing-provider"
        ));
    }

    #[test]
    fn validation_rejects_scene_provider_that_only_matches_adapter() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.llm.adapters.push(super::LlmAdapterConfig {
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
            super::ConfigError::UnknownSceneProviderId { scene_id, provider_id }
                if scene_id == RAW_SCENE_ID && provider_id == "adapter-only"
        ));
    }
}
