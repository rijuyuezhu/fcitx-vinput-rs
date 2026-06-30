//! Archive extraction safety and staging helpers.
//!
//! This module pins validation policy and provides a narrow tar extraction
//! boundary for already-staged local archives. It extracts into a caller-owned
//! staging directory through a temporary directory and publishes the staged tree
//! only after every entry passes the safety policy. It does not install assets,
//! replace install roots, read compressed archive wrappers, or mutate
//! configuration.

use std::{
    fs, io,
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

/// Successful staged archive extraction output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArchiveTree {
    /// Source archive path that was read.
    pub archive_path: PathBuf,
    /// Published extraction root. This is a staging directory, not an install root.
    pub path: PathBuf,
    /// Number of regular file entries extracted.
    pub file_count: usize,
    /// Number of directory entries created from explicit archive entries.
    pub directory_count: usize,
}

/// Archive staging errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum ArchiveStagingError {
    /// Archive file could not be opened.
    #[error("failed to open staged archive `{path}`: {message}")]
    OpenArchive {
        /// Archive path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Output staging directory already exists.
    #[error("archive staging output `{path}` already exists")]
    OutputExists {
        /// Requested final staged output path.
        path: String,
    },
    /// Parent directory for staged output could not be created.
    #[error("failed to create archive staging directory for `{path}`: {message}")]
    CreateDirectory {
        /// Staged output path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Tar entry iteration failed.
    #[error("failed to read tar archive `{path}`: {message}")]
    ReadArchive {
        /// Archive path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// A tar entry path could not be represented safely.
    #[error("failed to read tar entry path in `{archive}`: {message}")]
    EntryPath {
        /// Archive path.
        archive: String,
        /// Sanitized failure message.
        message: String,
    },
    /// A tar entry failed the archive safety policy.
    #[error("unsafe archive entry in `{archive}`: {error}")]
    UnsafeEntry {
        /// Archive path.
        archive: String,
        /// Safety policy error.
        error: ArchiveSafetyError,
    },
    /// A directory entry could not be created under the staging root.
    #[error("failed to create archive directory `{path}`: {message}")]
    CreateEntryDirectory {
        /// Target path under the temporary staging root.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// A file entry could not be created under the staging root.
    #[error("failed to create archive file `{path}`: {message}")]
    CreateEntryFile {
        /// Target path under the temporary staging root.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// A file entry could not be copied from the archive to staging.
    #[error("failed to copy archive file `{path}`: {message}")]
    CopyEntryFile {
        /// Target path under the temporary staging root.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// The fully extracted temporary tree could not be published.
    #[error("failed to publish staged archive `{path}`: {message}")]
    Publish {
        /// Final staged output path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
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

/// Extracts an already-staged local tar archive into a staged directory.
///
/// The final `output_root` is created only after all entries have been validated
/// and copied into a same-directory temporary tree. This function rejects
/// symlinks, hardlinks, absolute paths, parent traversal, backslashes, unknown
/// entry types, and attempts to publish over an existing output path. It reads
/// plain tar only; compressed wrappers such as `.tar.zst` remain future work.
pub fn stage_tar_archive(
    archive_path: impl AsRef<Path>,
    output_root: impl AsRef<Path>,
) -> Result<StagedArchiveTree, ArchiveStagingError> {
    let archive_path = archive_path.as_ref();
    let output_root = output_root.as_ref();
    if output_root.exists() {
        return Err(ArchiveStagingError::OutputExists {
            path: display_path(output_root),
        });
    }

    if let Some(parent) = output_root
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| ArchiveStagingError::CreateDirectory {
            path: display_path(output_root),
            message: sanitize_io_error(&error),
        })?;
    }

    let temp_root = archive_temp_path(output_root);
    remove_dir_if_exists(&temp_root);
    fs::create_dir_all(&temp_root).map_err(|error| ArchiveStagingError::CreateDirectory {
        path: display_path(output_root),
        message: sanitize_io_error(&error),
    })?;

    let result = extract_tar_archive_to_temp(archive_path, output_root, &temp_root);
    match result {
        Ok((file_count, directory_count)) => {
            fs::rename(&temp_root, output_root).map_err(|error| {
                remove_dir_if_exists(&temp_root);
                ArchiveStagingError::Publish {
                    path: display_path(output_root),
                    message: sanitize_io_error(&error),
                }
            })?;
            Ok(StagedArchiveTree {
                archive_path: archive_path.to_owned(),
                path: output_root.to_owned(),
                file_count,
                directory_count,
            })
        }
        Err(error) => {
            remove_dir_if_exists(&temp_root);
            Err(error)
        }
    }
}

fn extract_tar_archive_to_temp(
    archive_path: &Path,
    output_root: &Path,
    temp_root: &Path,
) -> Result<(usize, usize), ArchiveStagingError> {
    let file = fs::File::open(archive_path).map_err(|error| ArchiveStagingError::OpenArchive {
        path: display_path(archive_path),
        message: sanitize_io_error(&error),
    })?;
    let mut archive = tar::Archive::new(file);
    let entries = archive
        .entries()
        .map_err(|error| ArchiveStagingError::ReadArchive {
            path: display_path(archive_path),
            message: sanitize_io_error(&error),
        })?;

    let mut file_count = 0;
    let mut directory_count = 0;
    for entry in entries {
        let mut entry = entry.map_err(|error| ArchiveStagingError::ReadArchive {
            path: display_path(archive_path),
            message: sanitize_io_error(&error),
        })?;
        let kind = archive_entry_kind(entry.header().entry_type());
        let entry_path = archive_entry_path(archive_path, &entry)?;
        let target =
            checked_archive_entry_target(temp_root, &entry_path, kind).map_err(|error| {
                ArchiveStagingError::UnsafeEntry {
                    archive: display_path(archive_path),
                    error,
                }
            })?;

        if !target.starts_with(temp_root) {
            return Err(ArchiveStagingError::UnsafeEntry {
                archive: display_path(archive_path),
                error: ArchiveSafetyError::RootEscape {
                    entry: entry_path,
                    root: output_root.display().to_string(),
                },
            });
        }

        match kind {
            ArchiveEntryKind::Directory => {
                fs::create_dir_all(&target).map_err(|error| {
                    ArchiveStagingError::CreateEntryDirectory {
                        path: display_path(&target),
                        message: sanitize_io_error(&error),
                    }
                })?;
                directory_count += 1;
            }
            ArchiveEntryKind::File => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        ArchiveStagingError::CreateEntryDirectory {
                            path: display_path(parent),
                            message: sanitize_io_error(&error),
                        }
                    })?;
                }
                let mut output = fs::File::create(&target).map_err(|error| {
                    ArchiveStagingError::CreateEntryFile {
                        path: display_path(&target),
                        message: sanitize_io_error(&error),
                    }
                })?;
                io::copy(&mut entry, &mut output).map_err(|error| {
                    ArchiveStagingError::CopyEntryFile {
                        path: display_path(&target),
                        message: sanitize_io_error(&error),
                    }
                })?;
                file_count += 1;
            }
            ArchiveEntryKind::Symlink | ArchiveEntryKind::Hardlink | ArchiveEntryKind::Other => {
                return Err(ArchiveStagingError::UnsafeEntry {
                    archive: display_path(archive_path),
                    error: ArchiveSafetyError::UnsupportedEntryKind(kind.as_str()),
                });
            }
        }
    }

    Ok((file_count, directory_count))
}

fn archive_entry_path(
    archive_path: &Path,
    entry: &tar::Entry<'_, fs::File>,
) -> Result<String, ArchiveStagingError> {
    let path = entry
        .path()
        .map_err(|error| ArchiveStagingError::EntryPath {
            archive: display_path(archive_path),
            message: sanitize_io_error(&error),
        })?;
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| ArchiveStagingError::EntryPath {
            archive: display_path(archive_path),
            message: "non-utf8 path".to_owned(),
        })
}

fn archive_entry_kind(entry_type: tar::EntryType) -> ArchiveEntryKind {
    if entry_type.is_dir() {
        ArchiveEntryKind::Directory
    } else if entry_type.is_file() {
        ArchiveEntryKind::File
    } else if entry_type.is_symlink() {
        ArchiveEntryKind::Symlink
    } else if entry_type.is_hard_link() {
        ArchiveEntryKind::Hardlink
    } else {
        ArchiveEntryKind::Other
    }
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

impl ArchiveEntryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::Other => "other",
        }
    }
}

fn archive_temp_path(output_root: &Path) -> PathBuf {
    let file_name = output_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-archive");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    output_root.with_file_name(format!(".{file_name}.tmp.{}.{unique}", std::process::id()))
}

fn remove_dir_if_exists(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
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
