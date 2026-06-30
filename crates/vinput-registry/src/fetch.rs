//! Registry text fetch boundary with deterministic mirror fallback.

use std::time::Duration;

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

/// HTTP-backed registry text source using reqwest's blocking client.
///
/// This source only fetches registry index text. It does not download registry
/// assets, write cache files, extract archives, mutate config, or materialize
/// installs. Error messages are intentionally sanitized so request URLs,
/// response bodies, headers, and environment details are not copied into
/// diagnostics.
///
/// The reqwest blocking client is created and dropped inside a dedicated thread
/// so synchronous registry fetches remain safe when called from an async runtime.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestRegistryTextSource {
    timeout: Option<Duration>,
}

impl ReqwestRegistryTextSource {
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

impl RegistryTextSource for ReqwestRegistryTextSource {
    fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
        let url = url.to_owned();
        let timeout = self.timeout;
        std::thread::spawn(move || fetch_registry_text_blocking(&url, timeout))
            .join()
            .map_err(|_| "registry HTTP worker thread panicked".to_owned())?
    }
}

fn fetch_registry_text_blocking(url: &str, timeout: Option<Duration>) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let mut request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }

    let response = request
        .send()
        .map_err(|error| sanitize_registry_http_error(&error))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("registry HTTP mirror returned HTTP {status}"));
    }

    response
        .text()
        .map_err(|error| sanitize_registry_http_error(&error))
}

fn sanitize_registry_http_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "registry HTTP request timed out".to_owned()
    } else if error.is_connect() {
        "registry HTTP connection failed".to_owned()
    } else if error.is_body() || error.is_decode() {
        "registry HTTP response body read failed".to_owned()
    } else {
        "registry HTTP request failed".to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FetchedRegistryText {
    pub(crate) url: String,
    pub(crate) text: String,
}

pub(crate) fn fetch_registry_text_from_mirrors(
    source: &impl RegistryTextSource,
    mirrors: &[String],
) -> Result<FetchedRegistryText, RegistryFetchError> {
    if mirrors.is_empty() {
        return Err(RegistryFetchError::NoMirrors);
    }

    let mut failures = Vec::new();
    for mirror in mirrors {
        match source.fetch_registry_text(mirror) {
            Ok(text) => {
                return Ok(FetchedRegistryText {
                    url: mirror.clone(),
                    text,
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
    let fetched = fetch_registry_text_from_mirrors(source, mirrors)?;
    RegistryIndex::from_json_str(&fetched.text).map_err(|error| RegistryFetchError::InvalidIndex {
        url: fetched.url,
        error,
    })
}
