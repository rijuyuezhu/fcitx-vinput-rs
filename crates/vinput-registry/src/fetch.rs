//! Registry text fetch boundary with deterministic mirror fallback.

use thiserror::Error;

use crate::{RegistryError, RegistryIndex};

/// A failed registry mirror fetch attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryFetchFailure {
    /// Mirror URL that was attempted.
    pub url: String,
    /// Sanitized failure message from the fetch source.
    pub message: String,
}

/// Registry fetch boundary errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistryFetchError {
    /// No mirrors were provided.
    #[error("no registry mirrors configured")]
    NoMirrors,
    /// Every configured mirror failed before a registry body was fetched.
    #[error("all registry mirrors failed")]
    AllMirrorsFailed(Vec<RegistryFetchFailure>),
    /// A mirror returned text, but the text was not a valid registry index.
    #[error("registry mirror `{url}` returned an invalid index: {error}")]
    InvalidIndex {
        /// Mirror URL that returned invalid text.
        url: String,
        /// Registry parse or validation error.
        error: RegistryError,
    },
}

/// Fetch source used by registry mirror fallback.
///
/// This trait is intentionally small so production HTTP, cache fallback, and
/// tests can share the same mirror iteration and parse behavior.
pub trait RegistryTextSource {
    /// Fetches registry text from one mirror URL.
    fn fetch_registry_text(&self, url: &str) -> Result<String, String>;
}

/// Fetches and parses a registry index from ordered mirror URLs.
///
/// Mirrors are attempted in order. Transport-level failures fall through to the
/// next mirror. Once a mirror returns text, that text must parse and validate as
/// a registry index; invalid registry JSON is returned immediately instead of
/// falling back to another mirror, because it indicates a broken mirror contract
/// rather than temporary unavailability.
pub fn fetch_registry_index_from_mirrors(
    source: &impl RegistryTextSource,
    mirrors: &[String],
) -> Result<RegistryIndex, RegistryFetchError> {
    if mirrors.is_empty() {
        return Err(RegistryFetchError::NoMirrors);
    }

    let mut failures = Vec::new();
    for mirror in mirrors {
        match source.fetch_registry_text(mirror) {
            Ok(text) => {
                return RegistryIndex::from_json_str(&text).map_err(|error| {
                    RegistryFetchError::InvalidIndex {
                        url: mirror.clone(),
                        error,
                    }
                });
            }
            Err(message) => failures.push(RegistryFetchFailure {
                url: mirror.clone(),
                message,
            }),
        }
    }

    Err(RegistryFetchError::AllMirrorsFailed(failures))
}
