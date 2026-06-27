//! Deterministic text finishing helpers and adapter seams.

use thiserror::Error;
use vinput_config::{COMMAND_SCENE_ID, LlmAdapterConfig, RAW_SCENE_ID, SceneDefinition};
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
    /// Number of candidates requested by the scene.
    pub candidate_count: u8,
    /// Number of previous context lines requested by the scene.
    pub context_lines: u8,
    /// Scene timeout in milliseconds, if configured.
    pub timeout_ms: Option<u64>,
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
            candidate_count: request.scene.candidate_count,
            context_lines: request.scene.context_lines,
            timeout_ms: request.scene.timeout_ms,
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
    ///
    /// Supported placeholders are `{raw_text}`, `{selected_text}`, `{scene_id}`,
    /// `{scene_prompt}`, `{provider_id}`, `{model}`, `{candidate_count}`,
    /// `{context_lines}`, and `{timeout_ms}`. Unknown placeholders are kept as
    /// literal text for forward compatibility.
    #[must_use]
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }

    /// Renders supported placeholders using prompt context.
    #[must_use]
    pub fn render(&self, context: &PromptContext<'_>) -> String {
        let timeout_ms = context
            .timeout_ms
            .map(|timeout_ms| timeout_ms.to_string())
            .unwrap_or_default();
        self.template
            .replace("{raw_text}", context.raw_text)
            .replace("{selected_text}", context.selected_text)
            .replace("{scene_id}", context.scene_id)
            .replace("{scene_prompt}", context.scene_prompt)
            .replace("{provider_id}", context.provider_id)
            .replace("{model}", context.model)
            .replace("{candidate_count}", &context.candidate_count.to_string())
            .replace("{context_lines}", &context.context_lines.to_string())
            .replace("{timeout_ms}", &timeout_ms)
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

/// Adapter seam for real scene post-processing backends.
pub trait TextAdapter: Send + Sync {
    /// Finishes a scene that requires post-processing.
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError>;
}

/// Text processor that delegates post-processing scenes to an adapter.
#[derive(Debug, Clone)]
pub struct LlmTextProcessor<A> {
    adapter: A,
}

impl<A> LlmTextProcessor<A> {
    /// Creates a text processor backed by one adapter implementation.
    #[must_use]
    pub const fn new(adapter: A) -> Self {
        Self { adapter }
    }

    /// Returns the configured adapter.
    #[must_use]
    pub const fn adapter(&self) -> &A {
        &self.adapter
    }
}

impl<A: TextAdapter> TextProcessor for LlmTextProcessor<A> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        self.adapter.finish(request)
    }
}

/// Runner seam for command-backed text adapters.
pub trait CommandTextRunner: Send + Sync {
    /// Executes the configured command adapter for one post-processing request.
    fn run(
        &self,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError>;
}

/// Runner placeholder used until process execution is ported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnsupportedCommandTextRunner;

impl CommandTextRunner for UnsupportedCommandTextRunner {
    fn run(
        &self,
        _command: &str,
        _args: &[String],
        _env: &std::collections::HashMap<String, String>,
        _working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError> {
        Err(TextError::UnsupportedAdapter(request.scene.id.clone()))
    }
}

/// Command-backed text adapter skeleton.
///
/// It owns the command configuration shape and delegates execution to a runner
/// seam so real process spawning can be added without making tests flaky.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTextAdapter<R = UnsupportedCommandTextRunner> {
    id: String,
    command: String,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    working_dir: Option<String>,
    runner: R,
}

impl CommandTextAdapter<UnsupportedCommandTextRunner> {
    /// Creates a command adapter skeleton from executable and arguments.
    #[must_use]
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self::with_runner(command, args, UnsupportedCommandTextRunner)
    }

    /// Creates a command adapter skeleton from typed config.
    #[must_use]
    pub fn from_config(config: &LlmAdapterConfig) -> Self {
        Self::with_adapter_config(config, UnsupportedCommandTextRunner)
    }
}

impl<R> CommandTextAdapter<R> {
    /// Creates a command adapter with an injected runner.
    #[must_use]
    pub fn with_runner(command: impl Into<String>, args: Vec<String>, runner: R) -> Self {
        Self::with_config(
            String::new(),
            command,
            args,
            std::collections::HashMap::default(),
            None,
            runner,
        )
    }

    /// Creates a command adapter with full typed command config and runner.
    #[must_use]
    pub fn with_config(
        id: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
        env: std::collections::HashMap<String, String>,
        working_dir: Option<String>,
        runner: R,
    ) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            args,
            env,
            working_dir,
            runner,
        }
    }

    /// Creates a command adapter from typed config with a supplied runner.
    #[must_use]
    pub fn with_adapter_config(config: &LlmAdapterConfig, runner: R) -> Self {
        Self::with_config(
            config.id.clone(),
            config.command.clone(),
            config.args.clone(),
            config.env.clone(),
            config.working_dir.clone(),
            runner,
        )
    }

    /// Returns the configured adapter id, if known.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the configured command path or name.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Returns configured command arguments.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Returns configured command environment variables.
    #[must_use]
    pub fn env(&self) -> &std::collections::HashMap<String, String> {
        &self.env
    }

    /// Returns configured command working directory.
    #[must_use]
    pub fn working_dir(&self) -> Option<&str> {
        self.working_dir.as_deref()
    }

    /// Returns the configured command runner.
    #[must_use]
    pub const fn runner(&self) -> &R {
        &self.runner
    }
}

impl<R: CommandTextRunner> TextAdapter for CommandTextAdapter<R> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        self.runner.run(
            &self.command,
            &self.args,
            &self.env,
            self.working_dir.as_deref(),
            request,
        )
    }
}

/// Registry of configured text adapters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdapterRegistry {
    command_adapters: std::collections::HashMap<String, CommandTextAdapter>,
}

impl AdapterRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a registry from typed command adapter config entries.
    #[must_use]
    pub fn from_configs(adapters: &[LlmAdapterConfig]) -> Self {
        Self {
            command_adapters: adapters
                .iter()
                .map(|adapter| (adapter.id.clone(), CommandTextAdapter::from_config(adapter)))
                .collect(),
        }
    }

    /// Returns the number of registered adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.command_adapters.len()
    }

    /// Returns whether the registry has no adapters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.command_adapters.is_empty()
    }

    /// Returns whether a command adapter id is registered.
    #[must_use]
    pub fn contains_command_adapter(&self, id: &str) -> bool {
        self.command_adapters.contains_key(id)
    }

    /// Looks up a command adapter by id.
    #[must_use]
    pub fn command_adapter(&self, id: &str) -> Option<&CommandTextAdapter> {
        self.command_adapters.get(id)
    }
}

/// Adapter placeholder used until concrete local adapters are ported.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedTextAdapter;

impl UnsupportedTextAdapter {
    /// Creates an unsupported adapter placeholder.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl TextAdapter for UnsupportedTextAdapter {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        Err(TextError::UnsupportedAdapter(request.scene.id.clone()))
    }
}

/// Production-safe text finisher used before real LLM/adapter support lands.
///
/// It only commits raw/no-op scenes that do not require post-processing.
/// Command scenes, prompted scenes, provider/model-bound scenes, candidate
/// scenes, context-aware scenes, and timeout-bound scenes return a typed error
/// instead of fabricating mock text.
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
    #[error("scene `{0}` requires a text adapter/postprocess backend")]
    AdapterRequired(String),
    /// A configured adapter path exists but is not implemented yet.
    #[error("scene `{0}` requested a text adapter that is not implemented yet")]
    UnsupportedAdapter(String),
}

fn scene_needs_postprocessing(scene: &SceneDefinition) -> bool {
    scene.id == COMMAND_SCENE_ID
        || scene.candidate_count > 0
        || scene.context_lines > 0
        || scene.timeout_ms.is_some()
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
        CommandTextAdapter, CommandTextRunner, LlmTextProcessor, MockTextProcessor, PromptContext,
        PromptTemplate, TextError, TextFinisher, TextProcessor, TextRequest,
        UnsupportedTextAdapter,
    };
    use vinput_config::{COMMAND_SCENE_ID, LlmAdapterConfig, RAW_SCENE_ID, SceneDefinition};
    use vinput_protocol::RecognitionPayload;

    #[derive(Debug, Clone, Copy)]
    struct EchoCommandRunner;

    impl CommandTextRunner for EchoCommandRunner {
        fn run(
            &self,
            command: &str,
            args: &[String],
            env: &std::collections::HashMap<String, String>,
            working_dir: Option<&str>,
            request: &TextRequest<'_>,
        ) -> Result<RecognitionPayload, TextError> {
            Ok(RecognitionPayload::raw(format!(
                "{} {} {} {}: {}",
                command,
                args.join(" "),
                env.get("MODE").map(String::as_str).unwrap_or_default(),
                working_dir.unwrap_or_default(),
                request.raw_text
            )))
        }
    }

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
    fn prompt_context_exposes_scene_metadata() {
        let templated = SceneDefinition {
            prompt: Some("polish".to_owned()),
            provider_id: Some("p".to_owned()),
            model: Some("m".to_owned()),
            context_lines: 3,
            timeout_ms: Some(2500),
            ..scene("rewrite", 1)
        };
        let request = TextRequest {
            raw_text: "raw",
            scene: &templated,
            selected_text: Some("selected"),
        };

        let context = PromptContext::from_request(&request);
        assert_eq!(context.raw_text, "raw");
        assert_eq!(context.selected_text, "selected");
        assert_eq!(context.scene_id, "rewrite");
        assert_eq!(context.scene_prompt, "polish");
        assert_eq!(context.provider_id, "p");
        assert_eq!(context.model, "m");
        assert_eq!(context.candidate_count, 1);
        assert_eq!(context.context_lines, 3);
        assert_eq!(context.timeout_ms, Some(2500));
    }

    #[test]
    fn prompt_template_replaces_supported_fields() {
        let templated = SceneDefinition {
            prompt: Some("polish".to_owned()),
            provider_id: Some("p".to_owned()),
            model: Some("m".to_owned()),
            context_lines: 3,
            timeout_ms: Some(2500),
            ..scene("rewrite", 1)
        };
        let request = TextRequest {
            raw_text: "raw",
            scene: &templated,
            selected_text: Some("selected"),
        };
        let context = PromptContext::from_request(&request);
        let rendered = PromptTemplate::new(
            "scene={scene_id}; prompt={scene_prompt}; raw={raw_text}; selected={selected_text}; provider={provider_id}; model={model}; candidates={candidate_count}; context={context_lines}; timeout={timeout_ms}",
        )
        .render(&context);
        let rendered_from_request = PromptTemplate::new(
            "scene={scene_id}; prompt={scene_prompt}; raw={raw_text}; selected={selected_text}; provider={provider_id}; model={model}; candidates={candidate_count}; context={context_lines}; timeout={timeout_ms}",
        )
        .render_request(&request);
        assert_eq!(rendered_from_request, rendered);
        assert_eq!(
            rendered,
            "scene=rewrite; prompt=polish; raw=raw; selected=selected; provider=p; model=m; candidates=1; context=3; timeout=2500"
        );
    }

    #[test]
    fn prompt_template_renders_missing_timeout_as_empty() {
        let raw = scene("raw", 0);
        let request = TextRequest {
            raw_text: "raw",
            scene: &raw,
            selected_text: None,
        };

        let rendered = PromptTemplate::new("timeout={timeout_ms}").render_request(&request);
        assert_eq!(rendered, "timeout=");
    }

    #[test]
    fn prompt_template_keeps_unknown_placeholders() {
        let raw = scene("raw", 0);
        let request = TextRequest {
            raw_text: "raw",
            scene: &raw,
            selected_text: None,
        };

        let rendered = PromptTemplate::new("x={x}").render_request(&request);
        assert_eq!(rendered, "x={x}");
    }

    #[test]
    fn adapter_registry_indexes_command_adapters_from_config() {
        let registry = super::AdapterRegistry::from_configs(&[LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("MODE".to_owned(), "test".to_owned())]),
            working_dir: Some("/tmp/vinput".to_owned()),
            extra: std::collections::HashMap::default(),
        }]);

        assert_eq!(registry.len(), 1);
        assert!(registry.contains_command_adapter("cmd-adapter"));
        let adapter = registry
            .command_adapter("cmd-adapter")
            .expect("adapter should be indexed");
        assert_eq!(adapter.command(), "vinput-postprocess");
        assert_eq!(adapter.env().get("MODE").map(String::as_str), Some("test"));
        assert_eq!(adapter.working_dir(), Some("/tmp/vinput"));
        assert!(!registry.contains_command_adapter("missing"));
        assert!(registry.command_adapter("missing").is_none());
    }

    #[test]
    fn command_text_adapter_copies_typed_config() {
        let adapter = CommandTextAdapter::from_config(&LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("MODE".to_owned(), "test".to_owned())]),
            working_dir: Some("/tmp/vinput-text".to_owned()),
            extra: std::collections::HashMap::default(),
        });

        assert_eq!(adapter.id(), "cmd-adapter");
        assert_eq!(adapter.command(), "vinput-postprocess");
        assert_eq!(adapter.args(), ["--json"]);
        assert_eq!(adapter.env().get("MODE").map(String::as_str), Some("test"));
        assert_eq!(adapter.working_dir(), Some("/tmp/vinput-text"));
    }

    #[test]
    fn command_text_adapter_delegates_to_injected_runner() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "vinput-postprocess".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("MODE".to_owned(), "mock".to_owned())]),
            working_dir: Some("/tmp/vinput".to_owned()),
            extra: std::collections::HashMap::default(),
        };
        let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            EchoCommandRunner,
        ))
        .finish(&TextRequest {
            raw_text: "hello",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap();

        assert_eq!(
            payload.commit_text,
            "vinput-postprocess --json mock /tmp/vinput: hello"
        );
    }

    #[test]
    fn command_text_adapter_returns_unsupported_until_runner_lands() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let error = LlmTextProcessor::new(CommandTextAdapter::new(
            "vinput-postprocess",
            vec!["--json".to_owned()],
        ))
        .finish(&TextRequest {
            raw_text: "hello",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert_eq!(error, TextError::UnsupportedAdapter("polish".to_owned()));
    }

    #[test]
    fn llm_text_processor_keeps_noop_scene_raw() {
        let noop = scene("noop", 0);
        let payload = LlmTextProcessor::new(UnsupportedTextAdapter::new())
            .finish(&TextRequest {
                raw_text: "hello",
                scene: &noop,
                selected_text: None,
            })
            .unwrap();
        assert_eq!(payload.commit_text, "hello");
    }

    #[test]
    fn llm_text_processor_delegates_prompted_scene_to_adapter() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let error = LlmTextProcessor::new(UnsupportedTextAdapter::new())
            .finish(&TextRequest {
                raw_text: "hello",
                scene: &prompted,
                selected_text: None,
            })
            .unwrap_err();
        assert_eq!(error, TextError::UnsupportedAdapter("polish".to_owned()));
    }

    #[test]
    fn llm_text_processor_delegates_command_scene_to_adapter() {
        let command = scene(COMMAND_SCENE_ID, 0);
        let error = LlmTextProcessor::new(UnsupportedTextAdapter::new())
            .finish(&TextRequest {
                raw_text: "replace it",
                scene: &command,
                selected_text: Some("selected source"),
            })
            .unwrap_err();
        assert_eq!(
            error,
            TextError::UnsupportedAdapter(COMMAND_SCENE_ID.to_owned())
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
    fn timeout_scene_requires_future_adapter() {
        let timeout_scene = SceneDefinition {
            timeout_ms: Some(2500),
            ..scene("timeout-scene", 0)
        };
        let error = TextFinisher::finish(&TextRequest {
            raw_text: "hello",
            scene: &timeout_scene,
            selected_text: None,
        })
        .unwrap_err();
        assert_eq!(
            error,
            TextError::AdapterRequired("timeout-scene".to_owned())
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
    fn mock_processor_handles_timeout_scene() {
        let timeout_scene = SceneDefinition {
            timeout_ms: Some(2500),
            ..scene("timeout-scene", 0)
        };
        let payload = MockTextProcessor::new()
            .finish(&TextRequest {
                raw_text: "hello",
                scene: &timeout_scene,
                selected_text: None,
            })
            .unwrap();
        assert_eq!(payload.commit_text, "mock postprocess result: hello");
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
