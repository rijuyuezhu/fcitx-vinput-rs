//! Archive extraction safety policy helpers.
//!
//! This module only pins validation policy for future archive extraction. It
//! does not read archive formats, extract entries, install assets, or mutate
//! configuration.

use std::{
    path::{Component, Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;

/// Minimal archive entry kind used by the extraction safety policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveEntryKind {
    /// Regular file entry.
    File,
    /// Directory entry.
    Directory,
    /// Symbolic link entry, rejected by policy.
    Symlink,
    /// Hard link entry, rejected by policy.
    Hardlink,
    /// Any archive entry type that future archive readers cannot classify.
    Other,
}

/// Archive entry safety policy errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArchiveSafetyError {
    /// Archive entry path was empty or whitespace only.
    #[error("archive entry path is empty")]
    EmptyPath,
    /// Archive entry kind is not allowed for safe extraction.
    #[error("unsupported archive entry kind `{0}`")]
    UnsupportedEntryKind(&'static str),
    /// Archive entry path was absolute.
    #[error("archive entry path `{0}` is absolute")]
    AbsolutePath(String),
    /// Archive entry path used parent traversal.
    #[error("archive entry path `{0}` contains parent traversal")]
    ParentTraversal(String),
    /// Archive entry path used a Windows separator.
    #[error("archive entry path `{0}` contains a backslash separator")]
    Backslash(String),
    /// Archive entry path had no safe normal components.
    #[error("archive entry path `{0}` has no safe components")]
    NoSafeComponents(String),
    /// Archive entry target would escape the extraction root.
    #[error("archive entry path `{entry}` escapes extraction root `{root}`")]
    RootEscape {
        /// Archive entry path.
        entry: String,
        /// Extraction root.
        root: String,
    },
}

/// Validates an archive entry and returns its target path under the root.
///
/// Only regular files and directories are accepted. Returned paths are computed
/// lexically; this helper does not touch the filesystem and does not account for
/// symlinks already present on disk. Future extraction code must combine this
/// policy with a temporary extraction root that it controls.
pub fn checked_archive_entry_target(
    extraction_root: impl AsRef<Path>,
    entry_path: &str,
    kind: ArchiveEntryKind,
) -> Result<PathBuf, ArchiveSafetyError> {
    validate_entry_kind(kind)?;
    let root = extraction_root.as_ref();
    let relative = checked_relative_archive_path(entry_path)?;
    let target = root.join(relative);
    if !target.starts_with(root) {
        return Err(ArchiveSafetyError::RootEscape {
            entry: entry_path.to_owned(),
            root: root.display().to_string(),
        });
    }
    Ok(target)
}

fn validate_entry_kind(kind: ArchiveEntryKind) -> Result<(), ArchiveSafetyError> {
    match kind {
        ArchiveEntryKind::File | ArchiveEntryKind::Directory => Ok(()),
        ArchiveEntryKind::Symlink => Err(ArchiveSafetyError::UnsupportedEntryKind("symlink")),
        ArchiveEntryKind::Hardlink => Err(ArchiveSafetyError::UnsupportedEntryKind("hardlink")),
        ArchiveEntryKind::Other => Err(ArchiveSafetyError::UnsupportedEntryKind("other")),
    }
}

fn checked_relative_archive_path(entry_path: &str) -> Result<PathBuf, ArchiveSafetyError> {
    let trimmed = entry_path.trim();
    if trimmed.is_empty() {
        return Err(ArchiveSafetyError::EmptyPath);
    }
    if trimmed.contains('\\') {
        return Err(ArchiveSafetyError::Backslash(entry_path.to_owned()));
    }

    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(ArchiveSafetyError::AbsolutePath(entry_path.to_owned()));
    }

    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(ArchiveSafetyError::ParentTraversal(entry_path.to_owned()));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveSafetyError::AbsolutePath(entry_path.to_owned()));
            }
        }
    }

    if safe.as_os_str().is_empty() {
        Err(ArchiveSafetyError::NoSafeComponents(entry_path.to_owned()))
    } else {
        Ok(safe)
    }
}

impl FromStr for ArchiveEntryKind {
    type Err = ArchiveSafetyError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "file" => Ok(Self::File),
            "directory" => Ok(Self::Directory),
            "symlink" => Ok(Self::Symlink),
            "hardlink" => Ok(Self::Hardlink),
            _ => Ok(Self::Other),
        }
    }
}
