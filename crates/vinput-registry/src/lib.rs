//! Registry manifest models and URL resolution helpers.
//!
//! Network download and archive handling are intentionally not implemented here
//! yet.  This crate owns the pure data contract for registry indexes so later
//! code can fetch, validate, and install assets behind tested boundaries.

use std::collections::HashSet;

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
    /// Unknown model id.
    #[error("unknown model id `{0}`")]
    UnknownModelId(String),
    /// Duplicate model id.
    #[error("duplicate model id `{0}`")]
    DuplicateModelId(String),
    /// Model provider must not be empty.
    #[error("model `{0}` has an empty provider")]
    EmptyProvider(String),
    /// Unknown adapter id.
    #[error("unknown adapter id `{0}`")]
    UnknownAdapterId(String),
    /// Duplicate adapter id.
    #[error("duplicate adapter id `{0}`")]
    DuplicateAdapterId(String),
    /// Adapter kind must not be empty.
    #[error("adapter `{0}` has an empty kind")]
    EmptyAdapterKind(String),
    /// Asset path must not be empty.
    #[error("asset path must not be empty")]
    EmptyAssetPath,
    /// Duplicate asset path within one registry entry.
    #[error("duplicate asset path `{0}`")]
    DuplicateAssetPath(String),
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

#[cfg(test)]
mod tests {
    use super::{ChecksumPolicy, InstallPlan, RegistryError, RegistryIndex};
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
          "assets": [
            {
              "path": "adapters/mock-adapter.tar.zst",
              "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
              "size_bytes": 7
            }
          ]
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
    fn summarizes_planned_asset_sizes() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let plan = index.planned_assets(&RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        });
        let summary = super::AssetPlanSummary::from_assets(&plan);
        assert_eq!(summary.asset_count, 2);
        assert_eq!(summary.known_size_bytes, 49);
        assert_eq!(summary.unknown_size_count, 0);
    }

    #[test]
    fn summary_counts_registry_entries_and_assets() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let summary = index.summary();
        assert_eq!(summary.version, 1);
        assert_eq!(summary.model_count, 1);
        assert_eq!(summary.adapter_count, 1);
        assert_eq!(summary.asset_count, 2);
    }

    #[test]
    fn plans_assets_with_entry_metadata_and_urls() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let plan = index.planned_assets(&RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        });
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].entry_kind, super::RegistryEntryKind::Model);
        assert_eq!(plan[0].entry_id, "sherpa-zh-small");
        assert_eq!(plan[0].path, "models/sherpa-zh-small.tar.zst");
        assert_eq!(
            plan[0].urls,
            vec!["https://registry.invalid/root/models/sherpa-zh-small.tar.zst".to_owned()]
        );
        assert_eq!(plan[1].entry_kind, super::RegistryEntryKind::Adapter);
        assert_eq!(plan[1].entry_id, "mock-adapter");
        assert_eq!(
            plan[1].urls,
            vec!["https://registry.invalid/root/adapters/mock-adapter.tar.zst".to_owned()]
        );
    }
    #[test]
    fn install_plan_adds_targets_and_checksum_policy() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let config = RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        };
        let plan = index.install_plan(&config, "/var/lib/vinput/assets/");

        assert_eq!(plan.target_root, "/var/lib/vinput/assets");
        assert_eq!(plan.summary.asset_count, 2);
        assert_eq!(plan.summary.known_size_bytes, 49);
        assert_eq!(plan.summary.missing_checksum_count, 0);
        assert_eq!(
            plan.assets[0].target_path,
            "/var/lib/vinput/assets/models/sherpa-zh-small.tar.zst"
        );
        assert_eq!(plan.assets[0].checksum_policy, ChecksumPolicy::Sha256);
    }

    #[test]
    fn install_plan_tracks_missing_checksums() {
        let index = RegistryIndex::from_json_str(
            r#"{"version":1,"models":[{"id":"m","label":"M","provider":"p","assets":[{"path":"models/m.tar"}]}]}"#,
        )
        .unwrap();
        let assets = index.planned_assets(&RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        });
        let plan = InstallPlan::from_assets(&assets, "cache");

        assert_eq!(plan.summary.missing_checksum_count, 1);
        assert_eq!(plan.assets[0].target_path, "cache/models/m.tar");
        assert_eq!(plan.assets[0].checksum_policy, ChecksumPolicy::Missing);
    }

    #[test]
    fn install_plan_uses_relative_targets_for_empty_root() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let config = RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        };
        let plan = index.install_plan(&config, "");

        assert_eq!(plan.target_root, "");
        assert_eq!(plan.assets[0].target_path, "models/sherpa-zh-small.tar.zst");
        assert_eq!(plan.assets[1].target_path, "adapters/mock-adapter.tar.zst");
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
    fn plans_assets_for_selected_entries() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let config = RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        };
        let model_plan = index
            .planned_model_assets("sherpa-zh-small", &config)
            .unwrap();
        assert_eq!(model_plan.len(), 1);
        assert_eq!(model_plan[0].entry_kind, super::RegistryEntryKind::Model);
        assert_eq!(model_plan[0].entry_id, "sherpa-zh-small");
        let adapter_plan = index
            .planned_adapter_assets("mock-adapter", &config)
            .unwrap();
        assert_eq!(adapter_plan.len(), 1);
        assert_eq!(
            adapter_plan[0].entry_kind,
            super::RegistryEntryKind::Adapter
        );
        assert_eq!(adapter_plan[0].entry_id, "mock-adapter");
    }

    #[test]
    fn selected_asset_plans_reject_unknown_entries() {
        let index = RegistryIndex::from_json_str(SAMPLE).unwrap();
        let config = RegistryConfig {
            base_urls: vec!["https://registry.invalid/root".to_owned()],
        };
        assert_eq!(
            index
                .planned_model_assets("missing-model", &config)
                .unwrap_err(),
            RegistryError::UnknownModelId("missing-model".to_owned())
        );
        assert_eq!(
            index
                .planned_adapter_assets("missing-adapter", &config)
                .unwrap_err(),
            RegistryError::UnknownAdapterId("missing-adapter".to_owned())
        );
    }

    #[test]
    fn rejects_duplicate_model_ids() {
        let json = r#"
        {
          "version": 1,
          "models": [
            {"id":"m","label":"M","provider":"p","assets":[]},
            {"id":"m","label":"M again","provider":"p","assets":[]}
          ]
        }
        "#;
        assert_eq!(
            RegistryIndex::from_json_str(json).unwrap_err(),
            RegistryError::DuplicateModelId("m".to_owned())
        );
    }

    #[test]
    fn rejects_duplicate_adapter_ids() {
        let json = r#"
        {
          "version": 1,
          "adapters": [
            {"id":"a","label":"A","kind":"command","assets":[]},
            {"id":"a","label":"A again","kind":"command","assets":[]}
          ]
        }
        "#;
        assert_eq!(
            RegistryIndex::from_json_str(json).unwrap_err(),
            RegistryError::DuplicateAdapterId("a".to_owned())
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
    fn rejects_duplicate_asset_paths_within_entry() {
        let json = r#"
        {
          "version": 1,
          "models": [
            {
              "id":"m",
              "label":"M",
              "provider":"p",
              "assets":[{"path":"m.tar"},{"path":"m.tar"}]
            }
          ]
        }
        "#;
        assert_eq!(
            RegistryIndex::from_json_str(json).unwrap_err(),
            RegistryError::DuplicateAssetPath("m.tar".to_owned())
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
