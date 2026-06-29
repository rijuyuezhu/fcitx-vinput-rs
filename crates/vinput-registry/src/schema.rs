//! Registry schema models, validation, URL resolution, and dry-run planning.

use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinput_config::RegistryConfig;

use crate::RegistryError;

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
        let mut model_ids = HashSet::new();
        for model in &self.models {
            model.validate()?;
            if !model_ids.insert(model.id.as_str()) {
                return Err(RegistryError::DuplicateModelId(model.id.clone()));
            }
        }
        let mut adapter_ids = HashSet::new();
        for adapter in &self.adapters {
            adapter.validate()?;
            if !adapter_ids.insert(adapter.id.as_str()) {
                return Err(RegistryError::DuplicateAdapterId(adapter.id.clone()));
            }
        }
        Ok(())
    }

    /// Builds a compact summary for CLI and diagnostics.
    #[must_use]
    pub fn summary(&self) -> RegistryIndexSummary {
        RegistryIndexSummary {
            version: self.version,
            model_count: self.models.len(),
            adapter_count: self.adapters.len(),
            asset_count: self
                .models
                .iter()
                .map(|model| model.assets.len())
                .sum::<usize>()
                + self
                    .adapters
                    .iter()
                    .map(|adapter| adapter.assets.len())
                    .sum::<usize>(),
        }
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

/// Compact registry index summary for CLI and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RegistryIndexSummary {
    /// Registry index schema version.
    pub version: u32,
    /// Number of models.
    pub model_count: usize,
    /// Number of adapters.
    pub adapter_count: usize,
    /// Total number of model and adapter assets.
    pub asset_count: usize,
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
        validate_assets(&self.assets)?;
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
        validate_assets(&self.assets)?;
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
        if self.path.starts_with('/')
            || self.path.contains("..")
            || self.path.contains("://")
            || self.path.contains('\\')
        {
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

fn validate_assets(assets: &[AssetEntry]) -> Result<(), RegistryError> {
    let mut paths = HashSet::new();
    for asset in assets {
        asset.validate()?;
        if !paths.insert(asset.path.as_str()) {
            return Err(RegistryError::DuplicateAssetPath(asset.path.clone()));
        }
    }
    Ok(())
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
