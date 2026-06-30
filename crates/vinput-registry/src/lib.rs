//! Registry manifest models, URL resolution helpers, and staged asset boundaries.
//!
//! Asset staging can download and verify one planned asset into a caller-owned
//! staging file. Archive extraction, install root materialization, configuration
//! mutation, and user-facing install commands are intentionally still outside
//! this crate boundary.

mod archive;
mod asset;
mod cache;
mod checksum;
mod error;
mod fetch;
mod plan;
mod schema;

pub use archive::{
    ArchiveEntryKind, ArchiveSafetyError, ArchiveStagingError, StagedArchiveTree,
    checked_archive_entry_target, stage_tar_archive,
};
pub use asset::{
    AssetChecksumStatus, RegistryAssetFetchFailure, RegistryAssetSource, RegistryAssetStagingError,
    ReqwestRegistryAssetSource, StagedRegistryAsset, stage_planned_asset,
};
pub use cache::{
    RegistryCacheError, RegistryCachedFetchError, RegistryTextCache,
    fetch_registry_index_with_cache,
};
pub use checksum::{
    RegistrySha256Error, sha256_hex, verify_sha256_bytes, verify_sha256_file, verify_sha256_reader,
};
pub use error::RegistryError;
pub use fetch::{
    RegistryFetchError, RegistryFetchFailure, RegistryTextSource, ReqwestRegistryTextSource,
    fetch_registry_index_from_mirrors,
};
pub use plan::{
    AssetPlanSummary, ChecksumPolicy, InstallPlan, InstallPlanSummary, PlannedAsset,
    PlannedInstallAsset, RegistryEntryKind,
};
pub use schema::{AdapterEntry, AssetEntry, ModelEntry, RegistryIndex, RegistryIndexSummary};

#[cfg(test)]
mod tests;
