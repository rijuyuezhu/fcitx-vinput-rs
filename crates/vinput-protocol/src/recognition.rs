//! Recognition result JSON payload shared between daemon and frontend.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use strum::EnumString;
use thiserror::Error;

/// Source of a recognition candidate.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, EnumString,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum CandidateSource {
    /// Raw ASR text before post-processing.
    Raw,
    /// Text produced by an LLM post-processor.
    Llm,
    /// Direct ASR output.
    Asr,
    /// User-visible cancellation sentinel.
    Cancel,
}

impl CandidateSource {
    /// Returns the legacy JSON string for this source.
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Llm => "llm",
            Self::Asr => "asr",
            Self::Cancel => "cancel",
        }
    }

    /// Parses a legacy source string.
    pub fn parse_wire(input: &str) -> Result<Self, strum::ParseError> {
        Self::from_str(input)
    }
}

impl fmt::Display for CandidateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

/// A single result menu candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Candidate {
    /// Text shown to the user and committed when selected.
    pub text: String,
    /// Candidate provenance.
    pub source: CandidateSource,
}

impl Candidate {
    /// Creates a candidate with owned text.
    #[must_use]
    pub fn new(text: impl Into<String>, source: CandidateSource) -> Self {
        Self {
            text: text.into(),
            source,
        }
    }
}

/// Final recognition payload emitted by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecognitionPayload {
    /// Text that should be committed immediately when no menu is needed.
    pub commit_text: String,
    /// Ordered candidates for the result menu.
    #[serde(default)]
    pub candidates: Vec<Candidate>,
}

impl RecognitionPayload {
    /// Creates a raw one-candidate payload.
    #[must_use]
    pub fn raw(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            commit_text: text.clone(),
            candidates: vec![Candidate::new(text, CandidateSource::Raw)],
        }
    }

    /// Creates a cancellation payload matching the original wire shape.
    #[must_use]
    pub fn cancelled() -> Self {
        Self {
            commit_text: String::new(),
            candidates: vec![Candidate::new(String::new(), CandidateSource::Cancel)],
        }
    }

    /// Parses legacy JSON and applies compatibility fallback rules from the C++ implementation.
    pub fn from_json_str(input: &str) -> Result<Self, RecognitionProtocolError> {
        let payload: Self = serde_json::from_str(input)?;
        Ok(payload.normalized())
    }

    /// Serializes to the JSON string carried by the D-Bus `RecognitionResult` signal.
    pub fn to_json_string(&self) -> Result<String, RecognitionProtocolError> {
        Ok(serde_json::to_string(self)?)
    }

    /// Applies compatibility fallback rules:
    ///
    /// - empty `commit_text` plus non-empty candidates commits the first candidate;
    /// - empty candidates plus non-empty `commit_text` creates a raw candidate.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.commit_text.is_empty() {
            if let Some(first) = self.candidates.first() {
                self.commit_text = first.text.clone();
            }
        } else if self.candidates.is_empty() {
            self.candidates.push(Candidate::new(
                self.commit_text.clone(),
                CandidateSource::Raw,
            ));
        }
        self
    }

    /// Returns the candidate that should be committed by default.
    #[must_use]
    pub fn default_candidate(&self) -> Option<&Candidate> {
        self.candidates
            .iter()
            .find(|candidate| candidate.text == self.commit_text)
            .or_else(|| self.candidates.first())
    }
}

/// Errors while parsing or serializing protocol payloads.
#[derive(Debug, Error)]
pub enum RecognitionProtocolError {
    /// JSON shape was invalid.
    #[error("invalid recognition JSON payload: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::{Candidate, CandidateSource, RecognitionPayload};

    #[test]
    fn raw_payload_matches_legacy_json_shape() {
        let payload = RecognitionPayload::raw("hello");
        let json = payload.to_json_string().unwrap();
        assert_eq!(
            json,
            r#"{"commit_text":"hello","candidates":[{"text":"hello","source":"raw"}]}"#
        );
        assert_eq!(RecognitionPayload::from_json_str(&json).unwrap(), payload);
    }

    #[test]
    fn parser_fills_commit_text_from_first_candidate() {
        let payload = RecognitionPayload::from_json_str(
            r#"{"commit_text":"","candidates":[{"text":"fallback","source":"asr"}]}"#,
        )
        .unwrap();
        assert_eq!(payload.commit_text, "fallback");
        assert_eq!(
            payload.default_candidate().unwrap().source,
            CandidateSource::Asr
        );
    }

    #[test]
    fn parser_creates_raw_candidate_from_commit_text() {
        let payload =
            RecognitionPayload::from_json_str(r#"{"commit_text":"only","candidates":[]}"#).unwrap();
        assert_eq!(
            payload.candidates,
            vec![Candidate::new("only", CandidateSource::Raw)]
        );
    }
    #[test]
    fn default_candidate_falls_back_to_first_candidate() {
        let payload = RecognitionPayload {
            commit_text: "missing".to_owned(),
            candidates: vec![
                Candidate::new("first", CandidateSource::Asr),
                Candidate::new("second", CandidateSource::Llm),
            ],
        };

        let candidate = payload.default_candidate().unwrap();
        assert_eq!(candidate.text, "first");
        assert_eq!(candidate.source, CandidateSource::Asr);
    }
}
