//! Registry manifest models and URL resolution helpers.
//!
//! Network download and archive handling are intentionally not implemented here
//! yet.  This crate owns the pure data contract for registry indexes so later
//! code can fetch, validate, and install assets behind tested boundaries.

mod error;
mod fetch;
mod plan;
mod schema;

pub use error::RegistryError;
pub use fetch::{
    RegistryFetchError, RegistryFetchFailure, RegistryTextSource, fetch_registry_index_from_mirrors,
};
pub use plan::{
    AssetPlanSummary, ChecksumPolicy, InstallPlan, InstallPlanSummary, PlannedAsset,
    PlannedInstallAsset, RegistryEntryKind,
};
pub use schema::{AdapterEntry, AssetEntry, ModelEntry, RegistryIndex, RegistryIndexSummary};

#[cfg(test)]
mod tests;
