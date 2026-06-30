//! Registry asset download and staging boundary.
//!
//! This module can fetch one planned asset into a temporary staging file, verify
//! its declared checksum when present, and publish the staged file only after
//! validation succeeds. It does not extract archives, install or materialize
//! files under an install root, mutate user configuration, or expose a
//! user-facing install command.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use thiserror::Error;

use crate::{PlannedInstallAsset, RegistrySha256Error, verify_sha256_file};

/// A failed asset fetch attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryAssetFetchFailure {
    /// Candidate asset URL that was attempted.
    pub url: String,
    /// Sanitized failure message from the asset source.
    pub message: String,
}

/// Explicit checksum status for a staged asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetChecksumStatus {
    /// The staged file matched the declared lowercase SHA-256 checksum.
    VerifiedSha256(String),
    /// No checksum was declared, so the staged file was accepted without being
    /// represented as verified.
    Missing,
}

/// Published output from a successful asset staging operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedRegistryAsset {
    /// Registry-relative source asset path from the install plan.
    pub source_path: String,
    /// Final staged file path published after validation.
    pub path: PathBuf,
    /// Checksum status for the published file.
    pub checksum: AssetChecksumStatus,
}

/// Registry asset staging errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistryAssetStagingError {
    /// The planned asset has no candidate URLs.
    #[error("planned asset `{source_path}` has no candidate urls")]
    NoAssetUrls {
        /// Registry-relative source asset path.
        source_path: String,
    },
    /// The staging output directory could not be created.
    #[error("failed to create staging directory for `{path}`: {message}")]
    CreateDirectory {
        /// Final staged path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
    /// Every candidate URL failed before an asset file was fetched.
    #[error("all candidate urls failed for planned asset `{source_path}`")]
    AllAssetUrlsFailed {
        /// Registry-relative source asset path.
        source_path: String,
        /// Per-URL sanitized failures.
        failures: Vec<RegistryAssetFetchFailure>,
    },
    /// The downloaded file did not pass checksum policy.
    #[error("asset checksum rejected for `{source_path}`: {error}")]
    Checksum {
        /// Registry-relative source asset path.
        source_path: String,
        /// SHA-256 verification error.
        error: RegistrySha256Error,
    },
    /// The verified temporary file could not be published to the final staged path.
    #[error("failed to publish staged asset `{path}`: {message}")]
    Publish {
        /// Final staged path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
}

/// Source boundary for fetching a registry asset into a caller-owned temp path.
///
/// Implementations must write only to `destination` and return sanitized error
/// messages. The caller owns temp-file cleanup, checksum validation, and final
/// publication.
pub trait RegistryAssetSource {
    /// Fetches one asset URL into the provided temporary destination path.
    fn fetch_asset(&self, url: &str, destination: &Path) -> Result<(), String>;
}

/// HTTP-backed registry asset source using reqwest's blocking client.
///
/// This source only downloads raw asset bytes into the temp path provided by the
/// staging boundary. It does not verify checksums, extract archives, install
/// files, mutate config, or copy response bodies/headers into diagnostics.
///
/// The reqwest blocking client is created and dropped inside a dedicated thread
/// so synchronous asset staging remains safe when called from an async runtime.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestRegistryAssetSource {
    timeout: Option<Duration>,
}

impl ReqwestRegistryAssetSource {
    /// Creates a source with reqwest's default timeout behavior.
    #[must_use]
    pub const fn new() -> Self {
        Self { timeout: None }
    }

    /// Creates a source that applies a per-request timeout.
    #[must_use]
    pub const fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
        }
    }
}

impl RegistryAssetSource for ReqwestRegistryAssetSource {
    fn fetch_asset(&self, url: &str, destination: &Path) -> Result<(), String> {
        let url = url.to_owned();
        let destination = destination.to_owned();
        let timeout = self.timeout;
        std::thread::spawn(move || fetch_asset_blocking(&url, &destination, timeout))
            .join()
            .map_err(|_| "registry asset HTTP worker thread panicked".to_owned())?
    }
}

/// Downloads, verifies, and atomically publishes one planned asset to a staging file.
///
/// Candidate URLs are attempted in order for transport/source failures. A
/// checksum mismatch stops immediately, because it indicates a fetched but
/// untrusted asset rather than temporary mirror unavailability.
pub fn stage_planned_asset(
    source: &impl RegistryAssetSource,
    asset: &PlannedInstallAsset,
    output_path: impl AsRef<Path>,
) -> Result<StagedRegistryAsset, RegistryAssetStagingError> {
    let output_path = output_path.as_ref();
    if asset.urls.is_empty() {
        return Err(RegistryAssetStagingError::NoAssetUrls {
            source_path: asset.source_path.clone(),
        });
    }

    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| RegistryAssetStagingError::CreateDirectory {
            path: display_path(output_path),
            message: sanitize_io_error(&error),
        })?;
    }

    let temp_path = staging_temp_path(output_path);
    let mut failures = Vec::new();
    for url in &asset.urls {
        remove_file_if_exists(&temp_path);
        match source.fetch_asset(url, &temp_path) {
            Ok(()) => {
                let checksum = verify_staged_checksum(asset, &temp_path)?;
                fs::rename(&temp_path, output_path).map_err(|error| {
                    remove_file_if_exists(&temp_path);
                    RegistryAssetStagingError::Publish {
                        path: display_path(output_path),
                        message: sanitize_io_error(&error),
                    }
                })?;
                return Ok(StagedRegistryAsset {
                    source_path: asset.source_path.clone(),
                    path: output_path.to_owned(),
                    checksum,
                });
            }
            Err(message) => {
                failures.push(RegistryAssetFetchFailure {
                    url: url.clone(),
                    message,
                });
                remove_file_if_exists(&temp_path);
            }
        }
    }

    Err(RegistryAssetStagingError::AllAssetUrlsFailed {
        source_path: asset.source_path.clone(),
        failures,
    })
}

fn verify_staged_checksum(
    asset: &PlannedInstallAsset,
    temp_path: &Path,
) -> Result<AssetChecksumStatus, RegistryAssetStagingError> {
    match &asset.sha256 {
        Some(expected) => verify_sha256_file(temp_path, expected)
            .map(|()| AssetChecksumStatus::VerifiedSha256(expected.clone()))
            .map_err(|error| {
                remove_file_if_exists(temp_path);
                RegistryAssetStagingError::Checksum {
                    source_path: asset.source_path.clone(),
                    error,
                }
            }),
        None => Ok(AssetChecksumStatus::Missing),
    }
}

fn fetch_asset_blocking(
    url: &str,
    destination: &Path,
    timeout: Option<Duration>,
) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let mut request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream");
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }

    let mut response = request
        .send()
        .map_err(|error| sanitize_registry_asset_http_error(&error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("registry asset HTTP mirror returned HTTP {status}"));
    }

    let mut file = fs::File::create(destination).map_err(|error| {
        format!(
            "registry asset staging write failed: {}",
            sanitize_io_error(&error)
        )
    })?;
    io::copy(&mut response, &mut file)
        .map_err(|_| "registry asset HTTP response body read failed".to_owned())?;
    Ok(())
}

fn staging_temp_path(output_path: &Path) -> PathBuf {
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("registry-asset");
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    output_path.with_file_name(format!(".{file_name}.tmp.{}.{unique}", std::process::id()))
}

fn remove_file_if_exists(path: &Path) {
    let _ = fs::remove_file(path);
}

fn sanitize_registry_asset_http_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "registry asset HTTP request timed out".to_owned()
    } else if error.is_connect() {
        "registry asset HTTP connection failed".to_owned()
    } else if error.is_body() || error.is_decode() {
        "registry asset HTTP response body read failed".to_owned()
    } else {
        "registry asset HTTP request failed".to_owned()
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
}
