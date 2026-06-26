//! Registry manifest models and URL resolution helpers.
//!
//! Network download and archive handling are intentionally not implemented here
//! yet.  This crate owns the pure data contract for registry indexes so later
//! code can fetch, validate, and install assets behind tested boundaries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_config::RegistryConfig;

/// Top-level registry index document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RegistryIndex {
    /// Schema version of the registry document.
    pub version: u32,
    /// ASR model entries.
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    /// LLM adapter entries.
    #[serde(default)]
    pub adapters: Vec<AdapterEntry>,
}

impl RegistryIndex {
    /// Parses a JSON registry index.
    pub fn from_json_str(input: &str) -> Result<Self, RegistryError> {
        let index: Self = serde_json::from_str(input)?;
        index.validate()?;
        Ok(index)
    }

    /// Validates stable registry invariants.
    pub fn validate(&self) -> Result<(), RegistryError> {
        if self.version == 0 {
            return Err(RegistryError::InvalidVersion);
        }
        for model in &self.models {
            model.validate()?;
        }
        for adapter in &self.adapters {
            adapter.validate()?;
        }
        Ok(())
    }

    /// Finds a model by id.
    #[must_use]
    pub fn model(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|model| model.id == id)
    }

    /// Finds an adapter by id.
    #[must_use]
    pub fn adapter(&self, id: &str) -> Option<&AdapterEntry> {
        self.adapters.iter().find(|adapter| adapter.id == id)
    }
}

/// ASR model entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ModelEntry {
    /// Stable model id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Provider this model belongs to.
    pub provider: String,
    /// Optional language tag.
    #[serde(default)]
    pub language: Option<String>,
    /// Downloadable assets.
    #[serde(default)]
    pub assets: Vec<AssetEntry>,
}

impl ModelEntry {
    fn validate(&self) -> Result<(), RegistryError> {
        validate_id(&self.id)?;
        if self.provider.trim().is_empty() {
            return Err(RegistryError::EmptyProvider(self.id.clone()));
        }
        for asset in &self.assets {
            asset.validate()?;
        }
        Ok(())
    }
}

/// LLM adapter entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterEntry {
    /// Stable adapter id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Adapter executable or protocol kind.
    pub kind: String,
    /// Downloadable assets.
    #[serde(default)]
    pub assets: Vec<AssetEntry>,
}

impl AdapterEntry {
    fn validate(&self) -> Result<(), RegistryError> {
        validate_id(&self.id)?;
        if self.kind.trim().is_empty() {
            return Err(RegistryError::EmptyAdapterKind(self.id.clone()));
        }
        for asset in &self.assets {
            asset.validate()?;
        }
        Ok(())
    }
}

/// Downloadable registry asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetEntry {
    /// Asset filename or relative path inside the registry.
    pub path: String,
    /// Optional sha256 checksum.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Optional size in bytes.
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

impl AssetEntry {
    fn validate(&self) -> Result<(), RegistryError> {
        if self.path.trim().is_empty() {
            return Err(RegistryError::EmptyAssetPath);
        }
        if self.path.starts_with('/') || self.path.contains("..") {
            return Err(RegistryError::UnsafeAssetPath(self.path.clone()));
        }
        if let Some(sha256) = &self.sha256 {
            validate_sha256(sha256)?;
        }
        Ok(())
    }

    /// Resolves this asset against all configured registry base URLs.
    #[must_use]
    pub fn resolved_urls(&self, config: &RegistryConfig) -> Vec<String> {
        config
            .base_urls
            .iter()
            .map(|base| join_url(base, &self.path))
            .collect()
    }
}

/// Registry errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    /// JSON parsing failed.
    #[error("invalid registry json: {0}")]
    Json(String),
    /// Version must be greater than zero.
    #[error("registry version must be greater than zero")]
    InvalidVersion,
    /// Registry ids must not be empty.
    #[error("registry id must not be empty")]
    EmptyId,
    /// Model provider must not be empty.
    #[error("model `{0}` has an empty provider")]
    EmptyProvider(String),
    /// Adapter kind must not be empty.
    #[error("adapter `{0}` has an empty kind")]
    EmptyAdapterKind(String),
    /// Asset path must not be empty.
    #[error("asset path must not be empty")]
    EmptyAssetPath,
    /// Asset path must be registry-relative and not traverse directories.
    #[error("unsafe asset path `{0}`")]
    UnsafeAssetPath(String),
    /// SHA-256 checksum must be 64 lowercase hexadecimal characters.
    #[error("invalid sha256 checksum `{0}`")]
    InvalidSha256(String),
}

impl From<serde_json::Error> for RegistryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

fn validate_id(id: &str) -> Result<(), RegistryError> {
    if id.trim().is_empty() {
        Err(RegistryError::EmptyId)
    } else {
        Ok(())
    }
}

fn validate_sha256(input: &str) -> Result<(), RegistryError> {
    let valid = input.len() == 64
        && input
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(RegistryError::InvalidSha256(input.to_owned()))
    }
}

fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::{RegistryError, RegistryIndex};
    use vinput_config::RegistryConfig;

    const SAMPLE: &str = r#"
    {
      "version": 1,
      "models": [
        {
          "id": "sherpa-zh-small",
          "label": "Sherpa zh small",
          "provider": "sherpa-onnx",
          "language": "zh",
          "assets": [
            {
              "path": "models/sherpa-zh-small.tar.zst",
              "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "size_bytes": 42
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
    "#;

    #[test]
    fn parses_and_finds_registry_entries() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        assert_eq!(index.version, 1);
        assert_eq!(
            index.model("sherpa-zh-small").unwrap().provider,
            "sherpa-onnx"
        );
        assert_eq!(index.adapter("mock-adapter").unwrap().kind, "command");
        assert!(index.model("missing").is_none());
    }

    #[test]
    fn resolves_asset_against_all_base_urls() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let asset = &index.model("sherpa-zh-small").unwrap().assets[0];
        let urls = asset.resolved_urls(&RegistryConfig {
            base_urls: vec![
                "https://example.invalid/root/".to_owned(),
                "https://mirror.invalid/root".to_owned(),
            ],
        });
        assert_eq!(
            urls,
            vec![
                "https://example.invalid/root/models/sherpa-zh-small.tar.zst".to_owned(),
                "https://mirror.invalid/root/models/sherpa-zh-small.tar.zst".to_owned(),
            ]
        );
    }

    #[test]
    fn rejects_unsafe_asset_paths() {
        let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"../secret"}]}]}"#;
        assert_eq!(
            RegistryIndex::from_json_str(json).unwrap_err(),
            RegistryError::UnsafeAssetPath("../secret".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_sha256() {
        let json = r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"m.tar","sha256":"ABC"}]}]}"#;
        assert_eq!(
            RegistryIndex::from_json_str(json).unwrap_err(),
            RegistryError::InvalidSha256("ABC".to_owned())
        );
    }
}
