//! Configuration model and validation for vinput.
//!
//! The first implementation preserves the original `default-config.json` shape
//! and focuses on typed deserialization plus lightweight validation. Later
//! migrations can add versioned upgrades here without touching daemon code.

use std::collections::HashSet;

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
        self
    }

    /// Validates cross-field invariants that serde cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut registry_base_urls = HashSet::new();
        for base_url in &self.registry.base_urls {
            if base_url.trim().is_empty() {
                return Err(ConfigError::InvalidRegistryBaseUrl(base_url.clone()));
            }
            if !registry_base_urls.insert(base_url.as_str()) {
                return Err(ConfigError::DuplicateRegistryBaseUrl(base_url.clone()));
            }
        }

        if self.global.default_language.trim().is_empty() {
            return Err(ConfigError::InvalidDefaultLanguage);
        }
        if self.global.capture_device.trim().is_empty() {
            return Err(ConfigError::InvalidCaptureDevice);
        }

        let mut scene_ids = HashSet::new();
        for scene in &self.scenes.definitions {
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
        }

        if !scene_ids.contains(self.scenes.active_scene.as_str()) {
            return Err(ConfigError::UnknownActiveScene(
                self.scenes.active_scene.clone(),
            ));
        }

        let mut provider_ids = HashSet::new();
        for provider in &self.asr.providers {
            if provider.id.trim().is_empty() {
                return Err(ConfigError::InvalidAsrProviderId(provider.id.clone()));
            }
            if !provider_ids.insert(provider.id.as_str()) {
                return Err(ConfigError::DuplicateAsrProviderId(provider.id.clone()));
            }
        }

        if !self.asr.providers.is_empty()
            && !provider_ids.contains(self.asr.active_provider.as_str())
        {
            return Err(ConfigError::UnknownActiveAsrProvider(
                self.asr.active_provider.clone(),
            ));
        }

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
}

/// LLM provider/adapter config. Detailed typing will move here during migration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct LlmConfig {
    /// Provider entries preserved as JSON until the original shape is fully annotated.
    #[serde(default)]
    pub providers: Vec<serde_json::Value>,
    /// Adapter entries preserved as JSON until the original shape is fully annotated.
    #[serde(default)]
    pub adapters: Vec<serde_json::Value>,
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
    /// Number of result candidates to ask the post-processor for.
    #[serde(default)]
    pub candidate_count: u8,
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
    /// Empty ASR provider id.
    #[error("invalid empty ASR provider id")]
    InvalidAsrProviderId(String),
    /// Duplicate ASR provider id.
    #[error("duplicate ASR provider id `{0}`")]
    DuplicateAsrProviderId(String),
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

#[cfg(test)]
mod tests {
    use super::{AsrProviderKind, COMMAND_SCENE_ID, RAW_SCENE_ID, VinputConfig};
    use vinput_protocol::CandidateSource;

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
}
