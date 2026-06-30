//! Registry manifest models and URL resolution helpers.
//!
//! Network download and archive handling are intentionally not implemented here
//! yet.  This crate owns the pure data contract for registry indexes so later
//! code can fetch, validate, and install assets behind tested boundaries.

mod archive;
mod cache;
mod checksum;
mod error;
mod fetch;
mod plan;
mod schema;

pub use archive::{ArchiveEntryKind, ArchiveSafetyError, checked_archive_entry_target};
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
