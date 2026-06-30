//! Side-effect-free registry staging path planning.
//!
//! This module connects dry-run asset actions to the later download, archive
//! staging, and materialization boundaries without executing any filesystem
//! mutation. It does not download, extract, materialize, mutate configuration, or
//! expose user-facing install commands.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ArchiveEntryKind, ArchiveFormat, ArchiveSafetyError, InstallPlan, PlannedInstallAsset,
    checked_archive_entry_target,
};

/// Side-effect-free paths for staging one archive asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveStagingPaths {
    /// Registry-relative asset source path.
    pub source_path: String,
    /// Detected archive wrapper format.
    pub archive_format: ArchiveFormat,
    /// File path where the downloaded asset should be staged.
    pub staged_asset_path: PathBuf,
    /// Directory path where the archive should be extracted before materialization.
    pub archive_extract_path: PathBuf,
    /// Existing dry-run target path for later materialization.
    pub materialize_target_path: PathBuf,
}

/// Staging path planning errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveStagingPathError {
    /// Planned source asset path is unsafe for local staging.
    #[error("unsafe planned archive path `{source_path}`: {error}")]
    UnsafeSourcePath {
        /// Registry-relative source path from the dry-run asset action.
        source_path: String,
        /// Safety policy error.
        error: ArchiveSafetyError,
    },
    /// Planned asset path uses an unsupported archive wrapper.
    #[error("unsupported archive format for planned asset `{source_path}`")]
    UnsupportedFormat {
        /// Registry-relative source path from the dry-run asset action.
        source_path: String,
    },
    /// Two planned archive assets would use the same extraction tree.
    #[error(
        "duplicate archive extraction path for planned asset `{source_path}`: `{archive_extract_path}`"
    )]
    DuplicateExtractPath {
        /// Registry-relative source path from the dry-run asset action.
        source_path: String,
        /// Extraction path that would collide with an earlier planned archive.
        archive_extract_path: PathBuf,
    },
}

/// Builds side-effect-free staging paths for one archive asset.
///
/// The staged asset file is rooted under `<staging_root>/assets`, and the archive
/// extraction tree is rooted under `<staging_root>/trees`. Both paths reuse the
/// same lexical safety policy as archive entries to reject absolute paths,
/// parent traversal, backslashes, and empty components. The returned
/// `materialize_target_path` is copied from the dry-run plan; this function does
/// not create, replace, or mutate that path.
pub fn plan_archive_staging_paths(
    asset: &PlannedInstallAsset,
    staging_root: impl AsRef<Path>,
) -> Result<ArchiveStagingPaths, ArchiveStagingPathError> {
    let format = ArchiveFormat::from_path(&asset.source_path).ok_or_else(|| {
        ArchiveStagingPathError::UnsupportedFormat {
            source_path: asset.source_path.clone(),
        }
    })?;
    let staging_root = staging_root.as_ref();
    let staged_asset_path = checked_archive_entry_target(
        staging_root.join("assets"),
        &asset.source_path,
        ArchiveEntryKind::File,
    )
    .map_err(|error| ArchiveStagingPathError::UnsafeSourcePath {
        source_path: asset.source_path.clone(),
        error,
    })?;
    let archive_extract_path = checked_archive_entry_target(
        staging_root.join("trees"),
        archive_tree_name(&asset.source_path, format),
        ArchiveEntryKind::Directory,
    )
    .map_err(|error| ArchiveStagingPathError::UnsafeSourcePath {
        source_path: asset.source_path.clone(),
        error,
    })?;

    Ok(ArchiveStagingPaths {
        source_path: asset.source_path.clone(),
        archive_format: format,
        staged_asset_path,
        archive_extract_path,
        materialize_target_path: PathBuf::from(&asset.target_path),
    })
}

/// Builds side-effect-free staging paths for every archive asset in a dry-run plan.
///
/// This is a batch helper over `plan_archive_staging_paths`. It preserves plan
/// asset order and stops on the first unsupported or unsafe archive source path.
pub fn plan_archive_staging_paths_for_plan(
    plan: &InstallPlan,
    staging_root: impl AsRef<Path>,
) -> Result<Vec<ArchiveStagingPaths>, ArchiveStagingPathError> {
    let staging_root = staging_root.as_ref();
    let mut seen_extract_paths = HashSet::new();
    let mut planned_paths = Vec::with_capacity(plan.assets.len());
    for asset in &plan.assets {
        let paths = plan_archive_staging_paths(asset, staging_root)?;
        if !seen_extract_paths.insert(paths.archive_extract_path.clone()) {
            return Err(ArchiveStagingPathError::DuplicateExtractPath {
                source_path: asset.source_path.clone(),
                archive_extract_path: paths.archive_extract_path,
            });
        }
        planned_paths.push(paths);
    }
    Ok(planned_paths)
}

fn archive_tree_name(source_path: &str, format: ArchiveFormat) -> &str {
    match format {
        ArchiveFormat::Tar => source_path.strip_suffix(".tar").unwrap_or(source_path),
        ArchiveFormat::TarZst => source_path.strip_suffix(".tar.zst").unwrap_or(source_path),
    }
}
