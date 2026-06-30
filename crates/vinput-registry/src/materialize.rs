//! Registry staged-tree materialization boundary.
//!
//! This module moves a fully prepared staging directory into a target directory
//! using same-directory renames and an explicit rollback path for replacements.
//! It does not download assets, extract archives, mutate configuration, or expose
//! user-facing install commands.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// Result of materializing a staged registry tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedRegistryTree {
    /// Source staging path that was consumed by the rename.
    pub source_path: PathBuf,
    /// Final materialized target path.
    pub target_path: PathBuf,
    /// Whether an existing target directory was replaced.
    pub replaced_existing: bool,
}

/// Registry materialization errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistryMaterializeError {
    /// Source staging tree does not exist.
    #[error("staged tree `{path}` does not exist")]
    SourceMissing {
        /// Source staging path.
        path: String,
    },
    /// Source exists but is not a directory.
    #[error("staged tree `{path}` is not a directory")]
    SourceNotDirectory {
        /// Source staging path.
        path: String,
    },
    /// Target equals source.
    #[error("materialization target `{path}` is the staged source")]
    TargetEqualsSource {
        /// Shared source/target path.
        path: String,
    },
    /// Target exists but is not a directory.
    #[error("materialization target `{path}` exists but is not a directory")]
    TargetNotDirectory {
        /// Target path.
        path: String,
    },
    /// Target parent directory could not be created.
    #[error("failed to create materialization parent for `{path}`: {message}")]
    CreateTargetParent {
        /// Target path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Existing target could not be moved aside for replacement.
    #[error("failed to move existing target `{target}` aside to `{backup}`: {message}")]
    MoveExistingTarget {
        /// Existing target path.
        target: String,
        /// Temporary backup path.
        backup: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Staged source could not be published to target.
    #[error("failed to publish staged tree `{staged}` to `{target}`: {message}")]
    Publish {
        /// Source staging path.
        staged: String,
        /// Target path.
        target: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Replacement failed and the previous target could not be restored.
    #[error(
        "failed to publish staged tree `{staged}` to `{target}`: {publish_message}; rollback from `{backup}` failed: {rollback_message}"
    )]
    RollbackFailed {
        /// Source staging path.
        staged: String,
        /// Target path.
        target: String,
        /// Temporary backup path.
        backup: String,
        /// Sanitized publish failure.
        publish_message: String,
        /// Sanitized rollback failure.
        rollback_message: String,
    },
    /// The old target backup remained after successful replacement.
    #[error("failed to remove replaced target backup `{backup}`: {message}")]
    CleanupBackup {
        /// Temporary backup path.
        backup: String,
        /// Sanitized I/O failure message.
        message: String,
    },
}

/// Moves a staged directory into a target directory.
///
/// The operation consumes `source_path` with `rename`. If the target directory
/// exists, it is first moved to a same-directory backup; publish failure attempts
/// to roll that backup back into place. This boundary is intentionally local
/// filesystem-oriented and does not copy across filesystems, edit config, or run
/// install commands.
pub fn materialize_staged_tree(
    source_path: impl AsRef<Path>,
    target_path: impl AsRef<Path>,
) -> Result<MaterializedRegistryTree, RegistryMaterializeError> {
    let source_path = source_path.as_ref();
    let target_path = target_path.as_ref();

    validate_materialize_paths(source_path, target_path)?;
    if let Some(parent) = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            RegistryMaterializeError::CreateTargetParent {
                path: display_path(target_path),
                message: sanitize_io_error(&error),
            }
        })?;
    }

    let replaced_existing = target_path.exists();
    let backup_path = materialize_backup_path(target_path);
    if replaced_existing {
        fs::rename(target_path, &backup_path).map_err(|error| {
            RegistryMaterializeError::MoveExistingTarget {
                target: display_path(target_path),
                backup: display_path(&backup_path),
                message: sanitize_io_error(&error),
            }
        })?;
    }

    if let Err(error) = fs::rename(source_path, target_path) {
        let publish_message = sanitize_io_error(&error);
        if replaced_existing {
            return rollback_existing_target(
                source_path,
                target_path,
                &backup_path,
                publish_message,
            );
        }
        return Err(RegistryMaterializeError::Publish {
            staged: display_path(source_path),
            target: display_path(target_path),
            message: publish_message,
        });
    }

    if replaced_existing {
        fs::remove_dir_all(&backup_path).map_err(|error| {
            RegistryMaterializeError::CleanupBackup {
                backup: display_path(&backup_path),
                message: sanitize_io_error(&error),
            }
        })?;
    }

    Ok(MaterializedRegistryTree {
        source_path: source_path.to_owned(),
        target_path: target_path.to_owned(),
        replaced_existing,
    })
}

fn validate_materialize_paths(
    source_path: &Path,
    target_path: &Path,
) -> Result<(), RegistryMaterializeError> {
    if source_path == target_path {
        return Err(RegistryMaterializeError::TargetEqualsSource {
            path: display_path(source_path),
        });
    }
    if !source_path.exists() {
        return Err(RegistryMaterializeError::SourceMissing {
            path: display_path(source_path),
        });
    }
    if !source_path.is_dir() {
        return Err(RegistryMaterializeError::SourceNotDirectory {
            path: display_path(source_path),
        });
    }
    if target_path.exists() && !target_path.is_dir() {
        return Err(RegistryMaterializeError::TargetNotDirectory {
            path: display_path(target_path),
        });
    }
    Ok(())
}

fn rollback_existing_target<T>(
    source_path: &Path,
    target_path: &Path,
    backup_path: &Path,
    publish_message: String,
) -> Result<T, RegistryMaterializeError> {
    match fs::rename(backup_path, target_path) {
        Ok(()) => Err(RegistryMaterializeError::Publish {
            staged: display_path(source_path),
            target: display_path(target_path),
            message: publish_message,
        }),
        Err(rollback_error) => Err(RegistryMaterializeError::RollbackFailed {
            staged: display_path(source_path),
            target: display_path(target_path),
            backup: display_path(backup_path),
            publish_message,
            rollback_message: sanitize_io_error(&rollback_error),
        }),
    }
}

fn materialize_backup_path(target_path: &Path) -> PathBuf {
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-materialized");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    target_path.with_file_name(format!(
        ".{file_name}.backup.{}.{unique}",
        std::process::id()
    ))
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
}
