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
    /// Builds an install plan for all registry assets without downloading anything.
    #[must_use]
    pub fn install_plan(&self, config: &RegistryConfig, target_root: &str) -> InstallPlan {
        let assets = self.planned_assets(config);
        InstallPlan::from_assets(&assets, target_root)
    }

    /// Builds an install plan for one model id without downloading anything.
    pub fn install_model_plan(
        &self,
        model_id: &str,
        config: &RegistryConfig,
        target_root: &str,
    ) -> Result<InstallPlan, RegistryError> {
        let assets = self.planned_model_assets(model_id, config)?;
        Ok(InstallPlan::from_assets(&assets, target_root))
    }

    /// Builds an install plan for one adapter id without downloading anything.
    pub fn install_adapter_plan(
        &self,
        adapter_id: &str,
        config: &RegistryConfig,
        target_root: &str,
    ) -> Result<InstallPlan, RegistryError> {
        let assets = self.planned_adapter_assets(adapter_id, config)?;
        Ok(InstallPlan::from_assets(&assets, target_root))
    }

    /// Expands registry assets into deterministic planning rows.
    #[must_use]
    pub fn planned_assets(&self, config: &RegistryConfig) -> Vec<PlannedAsset> {
        let model_assets = self.models.iter().flat_map(|model| {
            model.assets.iter().map(|asset| PlannedAsset {
                entry_kind: RegistryEntryKind::Model,
                entry_id: model.id.clone(),
                path: asset.path.clone(),
                urls: asset.resolved_urls(config),
                sha256: asset.sha256.clone(),
                size_bytes: asset.size_bytes,
            })
        });
        let adapter_assets = self.adapters.iter().flat_map(|adapter| {
            adapter.assets.iter().map(|asset| PlannedAsset {
                entry_kind: RegistryEntryKind::Adapter,
                entry_id: adapter.id.clone(),
                path: asset.path.clone(),
                urls: asset.resolved_urls(config),
                sha256: asset.sha256.clone(),
                size_bytes: asset.size_bytes,
            })
        });
        model_assets.chain(adapter_assets).collect()
    }
    /// Expands assets for one model id into deterministic planning rows.
    pub fn planned_model_assets(
        &self,
        model_id: &str,
        config: &RegistryConfig,
    ) -> Result<Vec<PlannedAsset>, RegistryError> {
        let model = self
            .model(model_id)
            .ok_or_else(|| RegistryError::UnknownModelId(model_id.to_owned()))?;
        Ok(model
            .assets
            .iter()
            .map(|asset| PlannedAsset {
                entry_kind: RegistryEntryKind::Model,
                entry_id: model.id.clone(),
                path: asset.path.clone(),
                urls: asset.resolved_urls(config),
                sha256: asset.sha256.clone(),
                size_bytes: asset.size_bytes,
            })
            .collect())
    }

    /// Expands assets for one adapter id into deterministic planning rows.
    pub fn planned_adapter_assets(
        &self,
        adapter_id: &str,
        config: &RegistryConfig,
    ) -> Result<Vec<PlannedAsset>, RegistryError> {
        let adapter = self
            .adapter(adapter_id)
            .ok_or_else(|| RegistryError::UnknownAdapterId(adapter_id.to_owned()))?;
        Ok(adapter
            .assets
            .iter()
            .map(|asset| PlannedAsset {
                entry_kind: RegistryEntryKind::Adapter,
                entry_id: adapter.id.clone(),
                path: asset.path.clone(),
                urls: asset.resolved_urls(config),
                sha256: asset.sha256.clone(),
                size_bytes: asset.size_bytes,
            })
            .collect())
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

/// Summary for a planned registry asset set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetPlanSummary {
    /// Number of assets in the plan.
    pub asset_count: usize,
    /// Sum of known asset sizes.
    pub known_size_bytes: u64,
    /// Number of assets that do not declare a size.
    pub unknown_size_count: usize,
}

impl AssetPlanSummary {
    /// Builds a summary from planned assets.
    #[must_use]
    pub fn from_assets(assets: &[PlannedAsset]) -> Self {
        Self {
            asset_count: assets.len(),
            known_size_bytes: assets.iter().filter_map(|asset| asset.size_bytes).sum(),
            unknown_size_count: assets
                .iter()
                .filter(|asset| asset.size_bytes.is_none())
                .count(),
        }
    }
}

/// Registry entry kind that owns a planned asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RegistryEntryKind {
    /// ASR model entry.
    Model,
    /// Text adapter entry.
    Adapter,
}

/// Planning information for one registry asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedAsset {
    /// Owning entry kind.
    pub entry_kind: RegistryEntryKind,
    /// Owning model or adapter id.
    pub entry_id: String,
    /// Registry-relative asset path.
    pub path: String,
    /// Candidate URLs resolved against configured mirrors.
    pub urls: Vec<String>,
    /// Optional sha256 checksum.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Optional size in bytes.
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

/// A dry-run install plan derived from registry assets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InstallPlan {
    /// Target root directory where assets would be installed.
    pub target_root: String,
    /// Compact install-plan summary.
    pub summary: InstallPlanSummary,
    /// Per-asset install actions.
    pub assets: Vec<PlannedInstallAsset>,
}

impl InstallPlan {
    /// Builds a deterministic dry-run install plan from planned assets.
    #[must_use]
    pub fn from_assets(assets: &[PlannedAsset], target_root: &str) -> Self {
        let planned_assets = assets
            .iter()
            .map(|asset| PlannedInstallAsset::from_asset(asset, target_root))
            .collect::<Vec<_>>();
        Self {
            target_root: normalize_install_root(target_root),
            summary: InstallPlanSummary::from_assets(&planned_assets),
            assets: planned_assets,
        }
    }
}

/// Summary for a dry-run install plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InstallPlanSummary {
    /// Number of assets in the install plan.
    pub asset_count: usize,
    /// Sum of known asset sizes.
    pub known_size_bytes: u64,
    /// Number of assets without a sha256 checksum.
    pub missing_checksum_count: usize,
}

impl InstallPlanSummary {
    /// Builds a summary from planned install assets.
    #[must_use]
    pub fn from_assets(assets: &[PlannedInstallAsset]) -> Self {
        Self {
            asset_count: assets.len(),
            known_size_bytes: assets.iter().filter_map(|asset| asset.size_bytes).sum(),
            missing_checksum_count: assets
                .iter()
                .filter(|asset| asset.checksum_policy == ChecksumPolicy::Missing)
                .count(),
        }
    }
}

/// Per-asset action in a dry-run install plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedInstallAsset {
    /// Owning entry kind.
    pub entry_kind: RegistryEntryKind,
    /// Owning model or adapter id.
    pub entry_id: String,
    /// Registry-relative source asset path.
    pub source_path: String,
    /// Target path under the install root.
    pub target_path: String,
    /// Candidate URLs resolved against configured mirrors.
    pub urls: Vec<String>,
    /// Optional sha256 checksum.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Optional size in bytes.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// Checksum handling policy for a future downloader.
    pub checksum_policy: ChecksumPolicy,
}

impl PlannedInstallAsset {
    /// Builds a dry-run install action from a planned registry asset.
    #[must_use]
    pub fn from_asset(asset: &PlannedAsset, target_root: &str) -> Self {
        Self {
            entry_kind: asset.entry_kind,
            entry_id: asset.entry_id.clone(),
            source_path: asset.path.clone(),
            target_path: join_install_path(target_root, &asset.path),
            urls: asset.urls.clone(),
            sha256: asset.sha256.clone(),
            size_bytes: asset.size_bytes,
            checksum_policy: if asset.sha256.is_some() {
                ChecksumPolicy::Sha256
            } else {
                ChecksumPolicy::Missing
            },
        }
    }
}

/// Checksum policy requested by an install plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumPolicy {
    /// Verify the declared sha256 checksum before accepting the asset.
    Sha256,
    /// No checksum is available yet; callers should treat the plan as weaker.
    Missing,
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

fn normalize_install_root(root: &str) -> String {
    root.trim_end_matches('/').to_owned()
}

fn join_install_path(root: &str, path: &str) -> String {
    let root = normalize_install_root(root);
    let path = path.trim_start_matches('/');
    if root.is_empty() {
        path.to_owned()
    } else {
        format!("{root}/{path}")
    }
}
