//! Deterministic text finishing helpers before adapter integration.

use thiserror::Error;
use vinput_config::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition};
use vinput_protocol::RecognitionPayload;

/// Input to the text finishing stage.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRequest<'a> {
    /// Raw ASR text.
    pub raw_text: &'a str,
    /// Scene definition selected by the frontend/runtime.
    pub scene: &'a SceneDefinition,
    /// Optional selected text used by command mode.
    pub selected_text: Option<&'a str>,
}

/// Minimal text finisher used while adapter support is not ported yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextFinisher;

impl TextFinisher {
    /// Creates a finisher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Finishes raw recognition text into a legacy payload.
    pub fn finish(request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        if request.scene.id == COMMAND_SCENE_ID {
            return Ok(RecognitionPayload::raw(command_placeholder_text(
                request.raw_text,
                request.selected_text.unwrap_or_default(),
            )));
        }
        if request.scene.candidate_count == 0 {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        Err(TextError::AdapterRequired(request.scene.id.clone()))
    }
}

/// Errors from text finishing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextError {
    /// A non-raw scene with candidates needs adapter support that is not ported yet.
    #[error("scene `{0}` requires adapter text finishing")]
    AdapterRequired(String),
}

fn command_placeholder_text(raw_text: &str, selected_text: &str) -> String {
    if selected_text.is_empty() {
        format!("mock command result: {raw_text}")
    } else {
        format!("mock command result for: {selected_text}")
    }
}

#[cfg(test)]
mod tests {
    use super::{TextError, TextFinisher, TextRequest};
    use vinput_config::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition};

    fn scene(id: &str, candidate_count: u8) -> SceneDefinition {
        SceneDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            prompt: None,
            candidate_count,
        }
    }

    #[test]
    fn raw_scene_returns_raw_text() {
        let raw = scene(RAW_SCENE_ID, 0);
        let payload = TextFinisher::finish(&TextRequest {
            raw_text: "hello",
            scene: &raw,
            selected_text: None,
        })
        .unwrap();
        assert_eq!(payload.commit_text, "hello");
    }

    #[test]
    fn command_scene_uses_selected_text_placeholder() {
        let command = scene(COMMAND_SCENE_ID, 1);
        let payload = TextFinisher::finish(&TextRequest {
            raw_text: "replace it",
            scene: &command,
            selected_text: Some("source text"),
        })
        .unwrap();
        assert_eq!(payload.commit_text, "mock command result for: source text");
    }

    #[test]
    fn candidate_scene_requires_future_adapter() {
        let fancy = scene("rewrite", 2);
        let error = TextFinisher::finish(&TextRequest {
            raw_text: "hello",
            scene: &fancy,
            selected_text: None,
        })
        .unwrap_err();
        assert_eq!(error, TextError::AdapterRequired("rewrite".to_owned()));
    }
}
