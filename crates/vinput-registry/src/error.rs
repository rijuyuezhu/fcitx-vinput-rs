//! Registry error types.

use thiserror::Error;

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
