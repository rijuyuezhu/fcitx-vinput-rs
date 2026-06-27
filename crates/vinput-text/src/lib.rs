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

/// Context available while rendering a deterministic text prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptContext<'a> {
    /// Raw ASR text.
    pub raw_text: &'a str,
    /// Optional selected text used by command mode.
    pub selected_text: &'a str,
    /// Current scene id.
    pub scene_id: &'a str,
    /// Scene prompt text, if configured.
    pub scene_prompt: &'a str,
    /// Scene provider id, if configured.
    pub provider_id: &'a str,
    /// Scene model id, if configured.
    pub model: &'a str,
    /// Number of previous context lines requested by the scene.
    pub context_lines: u8,
}

impl<'a> PromptContext<'a> {
    /// Creates prompt context from a text request.
    #[must_use]
    pub fn from_request(request: &'a TextRequest<'a>) -> Self {
        Self {
            raw_text: request.raw_text,
            selected_text: request.selected_text.unwrap_or_default(),
            scene_id: &request.scene.id,
            scene_prompt: request.scene.prompt.as_deref().unwrap_or_default(),
            provider_id: request.scene.provider_id.as_deref().unwrap_or_default(),
            model: request.scene.model.as_deref().unwrap_or_default(),
            context_lines: request.scene.context_lines,
        }
    }
}

/// Tiny deterministic template renderer for command placeholders and future adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    template: String,
}

impl PromptTemplate {
    /// Creates a template with literal text and supported placeholders.
    #[must_use]
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }

    /// Renders supported placeholders using prompt context.
    #[must_use]
    pub fn render(&self, context: &PromptContext<'_>) -> String {
        self.template
            .replace("{raw_text}", context.raw_text)
            .replace("{selected_text}", context.selected_text)
            .replace("{scene_id}", context.scene_id)
            .replace("{scene_prompt}", context.scene_prompt)
            .replace("{provider_id}", context.provider_id)
            .replace("{model}", context.model)
            .replace("{context_lines}", &context.context_lines.to_string())
    }

    /// Renders supported placeholders directly from a text request.
    #[must_use]
    pub fn render_request<'a>(&self, request: &'a TextRequest<'a>) -> String {
        self.render(&PromptContext::from_request(request))
    }
}

/// Synchronous text post-processing seam used by daemon runtime and tests.
pub trait TextProcessor: Send {
    /// Finishes raw recognition text into a payload suitable for the frontend.
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError>;
}

/// Production-safe text finisher used before real LLM/adapter support lands.
///
/// It only commits scenes that do not require post-processing. Command scenes,
/// prompted scenes, provider/model-bound scenes, and candidate scenes return a
/// typed error instead of fabricating mock text.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextFinisher;

impl TextFinisher {
    /// Creates a finisher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Finishes raw recognition text into a payload.
    pub fn finish(request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        <Self as TextProcessor>::finish(&Self, request)
    }
}

impl TextProcessor for TextFinisher {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        Err(TextError::AdapterRequired(request.scene.id.clone()))
    }
}

/// Mock text processor for daemon prototypes and tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockTextProcessor;

impl MockTextProcessor {
    /// Creates a mock text processor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TextProcessor for MockTextProcessor {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        if request.scene.id == COMMAND_SCENE_ID {
            return Ok(RecognitionPayload::raw(command_placeholder_text(request)));
        }
        if request.scene.candidate_count == 0 && !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        Ok(RecognitionPayload::raw(
            PromptTemplate::new("mock postprocess result: {raw_text}").render_request(request),
        ))
    }
}

/// Errors from text finishing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TextError {
    /// A non-raw scene with candidates needs adapter support that is not ported yet.
    #[error("scene `{0}` requires adapter text finishing")]
    AdapterRequired(String),
}

fn scene_needs_postprocessing(scene: &SceneDefinition) -> bool {
    scene.id == COMMAND_SCENE_ID
        || scene.candidate_count > 0
        || scene.context_lines > 0
        || scene
            .prompt
            .as_deref()
            .is_some_and(|prompt| !prompt.trim().is_empty())
        || scene
            .provider_id
            .as_deref()
            .is_some_and(|provider_id| !provider_id.trim().is_empty())
        || scene
            .model
            .as_deref()
            .is_some_and(|model| !model.trim().is_empty())
}

fn command_placeholder_text(request: &TextRequest<'_>) -> String {
    if request.selected_text.unwrap_or_default().is_empty() {
        PromptTemplate::new("mock command result: {raw_text}").render_request(request)
    } else {
        PromptTemplate::new("mock command result for: {selected_text}").render_request(request)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MockTextProcessor, PromptContext, PromptTemplate, TextError, TextFinisher, TextProcessor,
        TextRequest,
    };
    use vinput_config::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition};

    fn scene(id: &str, candidate_count: u8) -> SceneDefinition {
        SceneDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count,
            timeout_ms: None,
            context_lines: 0,
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
    fn prompt_template_replaces_supported_fields() {
        let templated = SceneDefinition {
            prompt: Some("polish".to_owned()),
            context_lines: 3,
            ..scene("rewrite", 1)
        };
        let request = TextRequest {
            raw_text: "raw",
            scene: &templated,
            selected_text: Some("selected"),
        };
        let context = PromptContext::from_request(&request);
        let rendered = PromptTemplate::new(
            "scene={scene_id}; prompt={scene_prompt}; raw={raw_text}; selected={selected_text}; context={context_lines}",
        )
        .render(&context);
        let rendered_from_request = PromptTemplate::new(
            "scene={scene_id}; prompt={scene_prompt}; raw={raw_text}; selected={selected_text}; context={context_lines}",
        )
        .render_request(&request);
        assert_eq!(rendered_from_request, rendered);
        assert_eq!(
            rendered,
            "scene=rewrite; prompt=polish; raw=raw; selected=selected; context=3"
        );
    }

    #[test]
    fn command_scene_requires_adapter_in_production_finisher() {
        let command = scene(COMMAND_SCENE_ID, 0);
        let error = TextFinisher::finish(&TextRequest {
            raw_text: "replace it",
            scene: &command,
            selected_text: Some("selected source"),
        })
        .unwrap_err();
        assert_eq!(
            error,
            TextError::AdapterRequired(COMMAND_SCENE_ID.to_owned())
        );
    }

    #[test]
    fn mock_processor_handles_command_scene_with_selected_text() {
        let command = scene(COMMAND_SCENE_ID, 1);
        let payload = MockTextProcessor::new()
            .finish(&TextRequest {
                raw_text: "replace it",
                scene: &command,
                selected_text: Some("selected source"),
            })
            .unwrap();
        assert_eq!(
            payload.commit_text,
            "mock command result for: selected source"
        );
    }

    #[test]
    fn mock_processor_handles_command_scene_without_selected_text() {
        let command = scene(COMMAND_SCENE_ID, 1);
        let payload = MockTextProcessor::new()
            .finish(&TextRequest {
                raw_text: "replace it",
                scene: &command,
                selected_text: None,
            })
            .unwrap();
        assert_eq!(payload.commit_text, "mock command result: replace it");
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

    #[test]
    fn prompted_scene_requires_future_adapter() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let error = TextFinisher::finish(&TextRequest {
            raw_text: "hello",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();
        assert_eq!(error, TextError::AdapterRequired("polish".to_owned()));
    }

    #[test]
    fn provider_bound_scene_requires_future_adapter() {
        let provider_bound = SceneDefinition {
            provider_id: Some("openai".to_owned()),
            model: Some("gpt-test".to_owned()),
            ..scene("provider-scene", 0)
        };
        let error = TextFinisher::finish(&TextRequest {
            raw_text: "hello",
            scene: &provider_bound,
            selected_text: None,
        })
        .unwrap_err();
        assert_eq!(
            error,
            TextError::AdapterRequired("provider-scene".to_owned())
        );
    }

    #[test]
    fn context_scene_requires_future_adapter() {
        let context_scene = SceneDefinition {
            context_lines: 2,
            ..scene("context-scene", 0)
        };
        let error = TextFinisher::finish(&TextRequest {
            raw_text: "hello",
            scene: &context_scene,
            selected_text: None,
        })
        .unwrap_err();
        assert_eq!(
            error,
            TextError::AdapterRequired("context-scene".to_owned())
        );
    }

    #[test]
    fn mock_processor_handles_candidate_scene() {
        let fancy = scene("rewrite", 2);
        let payload = MockTextProcessor::new()
            .finish(&TextRequest {
                raw_text: "hello",
                scene: &fancy,
                selected_text: None,
            })
            .unwrap();
        assert_eq!(payload.commit_text, "mock postprocess result: hello");
    }
}
