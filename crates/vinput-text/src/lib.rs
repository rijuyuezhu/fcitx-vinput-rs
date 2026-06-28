//! Deterministic text finishing helpers and adapter seams.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
};
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

/// Filesystem layout helper for supervised text adapter runtime state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRuntimePaths {
    runtime_dir: PathBuf,
}

impl AdapterRuntimePaths {
    /// Creates runtime paths rooted at `runtime_dir`.
    #[must_use]
    pub fn new(runtime_dir: impl Into<PathBuf>) -> Self {
        Self {
            runtime_dir: runtime_dir.into(),
        }
    }

    /// Returns the runtime directory.
    #[must_use]
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// Builds a path for an adapter pid file using a safe adapter id.
    pub fn pid_path(&self, adapter_id: &str) -> Result<PathBuf, TextError> {
        Ok(self.runtime_dir.join(adapter_pid_file_name(adapter_id)?))
    }

    /// Writes an adapter pid file and returns its path.
    pub fn write_pid(&self, adapter_id: &str, pid: u32) -> Result<PathBuf, TextError> {
        let path = self.pid_path(adapter_id)?;
        fs::create_dir_all(&self.runtime_dir).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to create adapter runtime directory `{}`: {error}",
                self.runtime_dir.display()
            ))
        })?;
        fs::write(&path, pid.to_string()).map_err(|error| {
            TextError::AdapterRuntimeIo(format!(
                "failed to write adapter pid file `{}`: {error}",
                path.display()
            ))
        })?;
        Ok(path)
    }

    /// Reads an adapter pid file. Missing files return `Ok(None)`.
    pub fn read_pid(&self, adapter_id: &str) -> Result<Option<u32>, TextError> {
        let path = self.pid_path(adapter_id)?;
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(TextError::AdapterRuntimeIo(format!(
                    "failed to read adapter pid file `{}`: {error}",
                    path.display()
                )));
            }
        };
        let trimmed = content.trim();
        trimmed.parse::<u32>().map(Some).map_err(|error| {
            TextError::InvalidAdapterPid(format!("invalid pid in `{}`: {error}", path.display()))
        })
    }

    /// Removes an adapter pid file. Missing files return `Ok(false)`.
    pub fn remove_pid(&self, adapter_id: &str) -> Result<bool, TextError> {
        let path = self.pid_path(adapter_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(TextError::AdapterRuntimeIo(format!(
                "failed to remove adapter pid file `{}`: {error}",
                path.display()
            ))),
        }
    }
}

fn adapter_pid_file_name(adapter_id: &str) -> Result<String, TextError> {
    if adapter_id.is_empty()
        || adapter_id == "."
        || adapter_id == ".."
        || adapter_id.contains('/')
        || adapter_id.contains('\\')
    {
        return Err(TextError::InvalidAdapterId(adapter_id.to_owned()));
    }
    Ok(format!("{adapter_id}.pid"))
}

/// Command specification for a supervised text adapter process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterProcessSpec {
    /// Stable adapter id.
    pub id: String,
    /// Executable path or program name.
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables added to the child process.
    pub env: std::collections::HashMap<String, String>,
    /// Optional child working directory.
    pub working_dir: Option<String>,
}

impl AdapterProcessSpec {
    /// Builds a process spec from typed adapter config.
    #[must_use]
    pub fn from_config(config: &LlmAdapterConfig) -> Self {
        Self {
            id: config.id.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
            working_dir: config.working_dir.clone(),
        }
    }
}

/// A started adapter child process whose pid file has been written.
#[derive(Debug)]
pub struct StartedAdapterProcess {
    /// Stable adapter id.
    pub id: String,
    /// Child process id.
    pub pid: u32,
    /// Path to the written pid file.
    pub pid_path: PathBuf,
    /// Running child process handle.
    pub child: Child,
}

/// Starts a text adapter process and writes its pid file.
pub fn start_adapter_process(
    spec: &AdapterProcessSpec,
    paths: &AdapterRuntimePaths,
) -> Result<StartedAdapterProcess, TextError> {
    let mut command = Command::new(&spec.command);
    command
        .args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(working_dir) = &spec.working_dir {
        command.current_dir(working_dir);
    }

    let mut child = command.spawn().map_err(|error| {
        TextError::AdapterFailed(format!(
            "failed to spawn text adapter `{}`: {error}",
            spec.id
        ))
    })?;
    let pid = child.id();
    let pid_path = match paths.write_pid(&spec.id, pid) {
        Ok(path) => path,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };

    Ok(StartedAdapterProcess {
        id: spec.id.clone(),
        pid,
        pid_path,
        child,
    })
}

/// JSON request passed to command-backed text adapter helpers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTextRequest {
    /// Stable adapter id from config.
    pub adapter_id: String,
    /// Raw ASR text before post-processing.
    pub raw_text: String,
    /// Optional selected text for command-mode transforms.
    #[serde(default)]
    pub selected_text: Option<String>,
    /// Scene metadata that selected this adapter.
    pub scene: CommandTextScene,
}

impl CommandTextRequest {
    /// Builds a command-helper request from adapter id and runtime text request.
    #[must_use]
    pub fn from_text_request(adapter_id: impl Into<String>, request: &TextRequest<'_>) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            raw_text: request.raw_text.to_owned(),
            selected_text: request.selected_text.map(ToOwned::to_owned),
            scene: CommandTextScene::from_definition(request.scene),
        }
    }
}

/// Scene metadata serialized into command text adapter requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTextScene {
    /// Scene id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Optional prompt configured for the scene.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Optional provider id configured for the scene.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional model id configured for the scene.
    #[serde(default)]
    pub model: Option<String>,
    /// Number of candidates requested by the scene.
    pub candidate_count: u8,
    /// Scene timeout in milliseconds, if configured.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Previous context lines requested by the scene.
    pub context_lines: u8,
}

impl CommandTextScene {
    /// Copies command-helper scene metadata from typed config.
    #[must_use]
    pub fn from_definition(scene: &SceneDefinition) -> Self {
        Self {
            id: scene.id.clone(),
            label: scene.label.clone(),
            prompt: scene.prompt.clone(),
            provider_id: scene.provider_id.clone(),
            model: scene.model.clone(),
            candidate_count: scene.candidate_count,
            timeout_ms: scene.timeout_ms,
            context_lines: scene.context_lines,
        }
    }
}

/// JSON response returned by command-backed text adapter helpers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTextResponse {
    /// Full recognition payload returned by the helper.
    #[serde(default)]
    pub payload: Option<RecognitionPayload>,
    /// Final text after post-processing.
    #[serde(default)]
    pub text: Option<String>,
    /// Error message returned by the helper.
    #[serde(default, alias = "failure")]
    pub error: Option<String>,
}

impl CommandTextResponse {
    /// Converts a helper response into the daemon recognition payload.
    pub fn into_payload(self) -> Result<RecognitionPayload, TextError> {
        if let Some(message) = self.error.filter(|message| !message.trim().is_empty()) {
            return Err(TextError::AdapterFailed(message));
        }
        if let Some(payload) = self.payload {
            return Ok(payload.normalized());
        }
        let Some(text) = self.text.filter(|text| !text.trim().is_empty()) else {
            return Err(TextError::AdapterFailed(
                "command text response missing final text".to_owned(),
            ));
        };
        Ok(RecognitionPayload::raw(text))
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
    /// `{context_lines}`, and `{timeout_ms}`. Legacy prompt placeholders
    /// `{{asr}}`, `{{selected}}`, and `{{context}}` are also accepted; context
    /// expands to an empty string until recent-input cache wiring lands.
    /// Unknown placeholders are kept as literal text for forward compatibility.
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
            .replace("{{asr}}", context.raw_text)
            .replace("{{selected}}", context.selected_text)
            .replace("{{context}}", "")
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

/// Extracts candidate strings from the legacy OpenAI-compatible chat response shape.
///
/// The legacy post-processor asks providers to return a chat-completions response
/// whose first choice message content is itself a JSON object containing a
/// `candidates` string array. Invalid or unexpected shapes return an empty list.
#[must_use]
pub fn extract_openai_compatible_candidates(response_body: &str) -> Vec<String> {
    let Ok(response) = serde_json::from_str::<serde_json::Value>(response_body) else {
        return Vec::new();
    };
    let Some(content) = response
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
    else {
        return Vec::new();
    };
    let Ok(content) = serde_json::from_str::<serde_json::Value>(content) else {
        return Vec::new();
    };

    content
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(ToOwned::to_owned)
        .collect()
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
        adapter_id: &str,
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
        _adapter_id: &str,
        _command: &str,
        _args: &[String],
        _env: &std::collections::HashMap<String, String>,
        _working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError> {
        Err(TextError::UnsupportedAdapter(request.scene.id.clone()))
    }
}

/// Process runner for command-backed text adapter providers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessCommandTextRunner;

impl CommandTextRunner for ProcessCommandTextRunner {
    fn run(
        &self,
        adapter_id: &str,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        working_dir: Option<&str>,
        request: &TextRequest<'_>,
    ) -> Result<RecognitionPayload, TextError> {
        let mut command_process = Command::new(command);
        command_process
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(working_dir) = working_dir {
            command_process.current_dir(working_dir);
        }
        let mut child = command_process.spawn().map_err(|error| {
            TextError::AdapterFailed(format!(
                "failed to spawn text adapter `{adapter_id}`: {error}"
            ))
        })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            TextError::AdapterFailed(format!("text adapter `{adapter_id}` did not expose stdin"))
        })?;
        let helper_request = CommandTextRequest::from_text_request(adapter_id, request);
        let write_result = (|| {
            serde_json::to_writer(&mut stdin, &helper_request).map_err(|error| {
                TextError::AdapterFailed(format!(
                    "failed to encode text adapter request for `{adapter_id}`: {error}"
                ))
            })?;
            stdin.write_all(b"\n").map_err(|error| {
                TextError::AdapterFailed(format!(
                    "failed to write text adapter request for `{adapter_id}`: {error}"
                ))
            })?;
            Ok(())
        })();
        drop(stdin);

        if let Err(write_error) = write_result {
            let output = wait_for_text_adapter(adapter_id, child)?;
            if !output.status.success() {
                return text_adapter_exit_error(adapter_id, &output);
            }
            return Err(write_error);
        }

        let output = wait_for_text_adapter(adapter_id, child)?;
        if !output.status.success() {
            return text_adapter_exit_error(adapter_id, &output);
        }
        let response: CommandTextResponse =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                TextError::AdapterFailed(format!(
                    "failed to decode text adapter response for `{adapter_id}`: {error}"
                ))
            })?;
        response.into_payload()
    }
}

fn wait_for_text_adapter(
    adapter_id: &str,
    child: std::process::Child,
) -> Result<Output, TextError> {
    child.wait_with_output().map_err(|error| {
        TextError::AdapterFailed(format!(
            "failed to wait for text adapter `{adapter_id}`: {error}"
        ))
    })
}

fn text_adapter_exit_error(
    adapter_id: &str,
    output: &Output,
) -> Result<RecognitionPayload, TextError> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        return Err(TextError::AdapterFailed(format!(
            "text adapter `{adapter_id}` exited with {}",
            output.status
        )));
    }
    Err(TextError::AdapterFailed(format!(
        "text adapter `{adapter_id}` exited with {}: {stderr}",
        output.status
    )))
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
            &self.id,
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

    /// Returns the only configured command adapter when exactly one exists.
    #[must_use]
    pub fn single_command_adapter(&self) -> Option<&CommandTextAdapter> {
        if self.command_adapters.len() == 1 {
            self.command_adapters.values().next()
        } else {
            None
        }
    }
}

/// Text processor that dispatches post-processing scenes to configured command adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTextProcessor<R = UnsupportedCommandTextRunner> {
    adapters: Vec<CommandTextAdapter<R>>,
}

impl CommandTextProcessor<UnsupportedCommandTextRunner> {
    /// Builds a processor from typed command adapter config entries.
    #[must_use]
    pub fn from_configs(adapters: &[LlmAdapterConfig]) -> Self {
        Self {
            adapters: adapters
                .iter()
                .map(CommandTextAdapter::from_config)
                .collect(),
        }
    }
}

impl<R> CommandTextProcessor<R> {
    /// Builds a processor from already-constructed command adapters.
    #[must_use]
    pub fn from_adapters(adapters: Vec<CommandTextAdapter<R>>) -> Self {
        Self { adapters }
    }

    /// Returns the number of configured command adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Returns whether no command adapters are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

impl<R: Clone> CommandTextProcessor<R> {
    /// Builds a processor from typed command adapter config entries and one reusable runner.
    #[must_use]
    pub fn from_configs_with_runner(adapters: &[LlmAdapterConfig], runner: R) -> Self {
        Self {
            adapters: adapters
                .iter()
                .map(|adapter| CommandTextAdapter::with_adapter_config(adapter, runner.clone()))
                .collect(),
        }
    }
}

impl<R: CommandTextRunner> TextProcessor for CommandTextProcessor<R> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        let [adapter] = self.adapters.as_slice() else {
            if self.adapters.is_empty() {
                return Err(TextError::AdapterRequired(request.scene.id.clone()));
            }
            return Err(TextError::AmbiguousAdapter(request.scene.id.clone()));
        };
        adapter.finish(request)
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
    #[error("scene `{0}` requires a text adapter backend")]
    AdapterRequired(String),
    /// A configured adapter path exists but is not implemented yet.
    #[error("scene `{0}` requested a text adapter that is not implemented yet")]
    UnsupportedAdapter(String),
    /// Adapter selection was ambiguous for a scene.
    #[error("scene `{0}` has ambiguous text adapter selection")]
    AmbiguousAdapter(String),
    /// Command adapter id is unsafe for runtime paths.
    #[error("invalid text adapter id for runtime path: {0}")]
    InvalidAdapterId(String),
    /// Adapter runtime filesystem operation failed.
    #[error("text adapter runtime I/O failed: {0}")]
    AdapterRuntimeIo(String),
    /// Adapter runtime pid file was malformed.
    #[error("text adapter runtime pid file is invalid: {0}")]
    InvalidAdapterPid(String),
    /// Command adapter helper returned an error or invalid response.
    #[error("text adapter failed: {0}")]
    AdapterFailed(String),
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
        AdapterProcessSpec, AdapterRuntimePaths, CommandTextAdapter, CommandTextProcessor,
        CommandTextRequest, CommandTextResponse, CommandTextRunner, LlmTextProcessor,
        MockTextProcessor, ProcessCommandTextRunner, PromptContext, PromptTemplate, TextError,
        TextFinisher, TextProcessor, TextRequest, UnsupportedTextAdapter,
        extract_openai_compatible_candidates, start_adapter_process,
    };
    use vinput_config::{COMMAND_SCENE_ID, LlmAdapterConfig, RAW_SCENE_ID, SceneDefinition};
    use vinput_protocol::RecognitionPayload;

    #[derive(Debug, Clone, Copy)]
    struct EchoCommandRunner;

    impl CommandTextRunner for EchoCommandRunner {
        fn run(
            &self,
            _adapter_id: &str,
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
    fn prompt_template_supports_legacy_double_brace_placeholders() {
        let command = SceneDefinition {
            prompt: Some("apply command".to_owned()),
            ..scene("__command__", 1)
        };
        let request = TextRequest {
            raw_text: "make it shorter",
            scene: &command,
            selected_text: Some("This is the selected text."),
        };

        let rendered = PromptTemplate::new(
            "prompt={scene_prompt}; asr={{asr}}; selected={{selected}}; context={{context}}; unknown={{future}}",
        )
        .render_request(&request);

        assert_eq!(
            rendered,
            "prompt=apply command; asr=make it shorter; selected=This is the selected text.; context=; unknown={{future}}"
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
    fn adapter_runtime_paths_build_safe_pid_paths() {
        let paths = AdapterRuntimePaths::new("/tmp/vinput-runtime");

        assert_eq!(
            paths.pid_path("adapter.demo").unwrap(),
            std::path::PathBuf::from("/tmp/vinput-runtime/adapter.demo.pid")
        );
        assert_eq!(
            paths.runtime_dir(),
            std::path::Path::new("/tmp/vinput-runtime")
        );
    }

    #[test]
    fn adapter_runtime_paths_roundtrip_pid_files() {
        let mut runtime_dir = std::env::temp_dir();
        runtime_dir.push(format!(
            "vinput-text-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let paths = AdapterRuntimePaths::new(&runtime_dir);

        let pid_path = paths.write_pid("adapter.demo", 12345).unwrap();
        assert_eq!(pid_path, runtime_dir.join("adapter.demo.pid"));
        assert_eq!(paths.read_pid("adapter.demo").unwrap(), Some(12345));
        assert!(paths.remove_pid("adapter.demo").unwrap());
        assert_eq!(paths.read_pid("adapter.demo").unwrap(), None);
        assert!(!paths.remove_pid("adapter.demo").unwrap());
        std::fs::remove_dir_all(runtime_dir).unwrap();
    }

    #[test]
    fn adapter_runtime_paths_reject_malformed_pid_files() {
        let mut runtime_dir = std::env::temp_dir();
        runtime_dir.push(format!(
            "vinput-text-runtime-bad-pid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::write(runtime_dir.join("adapter.demo.pid"), "not-a-pid").unwrap();
        let paths = AdapterRuntimePaths::new(&runtime_dir);

        let error = paths.read_pid("adapter.demo").unwrap_err();
        assert!(
            matches!(error, TextError::InvalidAdapterPid(message) if message.contains("not-a-pid") || message.contains("invalid digit"))
        );
        std::fs::remove_dir_all(runtime_dir).unwrap();
    }

    #[test]
    fn adapter_runtime_paths_reject_unsafe_adapter_ids() {
        let paths = AdapterRuntimePaths::new("/tmp/vinput-runtime");

        for adapter_id in ["", ".", "..", "../escape", "nested/id", r"nested\id"] {
            let error = paths.pid_path(adapter_id).unwrap_err();
            assert_eq!(error, TextError::InvalidAdapterId(adapter_id.to_owned()));
        }
    }

    #[test]
    fn adapter_process_spec_copies_typed_config() {
        let spec = AdapterProcessSpec::from_config(&LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "helper".to_owned(),
            args: vec!["--serve".to_owned()],
            env: std::collections::HashMap::from([("MODE".to_owned(), "serve".to_owned())]),
            working_dir: Some("/tmp/vinput-adapter".to_owned()),
            extra: std::collections::HashMap::default(),
        });

        assert_eq!(spec.id, "cmd-adapter");
        assert_eq!(spec.command, "helper");
        assert_eq!(spec.args, ["--serve"]);
        assert_eq!(spec.env.get("MODE").map(String::as_str), Some("serve"));
        assert_eq!(spec.working_dir.as_deref(), Some("/tmp/vinput-adapter"));
    }

    #[test]
    fn start_adapter_process_writes_pid_file_and_returns_child() {
        let mut runtime_dir = std::env::temp_dir();
        runtime_dir.push(format!(
            "vinput-text-process-runtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let paths = AdapterRuntimePaths::new(&runtime_dir);
        let spec = AdapterProcessSpec {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec!["-c".to_owned(), "sleep 30".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
        };

        let mut started = start_adapter_process(&spec, &paths).unwrap();
        assert_eq!(started.id, "cmd-adapter");
        assert_eq!(paths.read_pid("cmd-adapter").unwrap(), Some(started.pid));
        assert_eq!(started.pid_path, runtime_dir.join("cmd-adapter.pid"));
        started.child.kill().unwrap();
        let _ = started.child.wait();
        assert!(paths.remove_pid("cmd-adapter").unwrap());
        std::fs::remove_dir_all(runtime_dir).unwrap();
    }

    #[test]
    fn start_adapter_process_reports_spawn_failure_without_pid_file() {
        let mut runtime_dir = std::env::temp_dir();
        runtime_dir.push(format!(
            "vinput-text-process-runtime-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let paths = AdapterRuntimePaths::new(&runtime_dir);
        let spec = AdapterProcessSpec {
            id: "cmd-adapter".to_owned(),
            command: format!("vinput-missing-adapter-{}", std::process::id()),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: None,
        };

        let error = start_adapter_process(&spec, &paths).unwrap_err();
        assert!(matches!(
            error,
            TextError::AdapterFailed(message)
                if message.contains("failed to spawn text adapter `cmd-adapter`")
        ));
        assert_eq!(paths.read_pid("cmd-adapter").unwrap(), None);
    }

    #[test]
    fn command_text_request_serializes_scene_context() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            provider_id: Some("openai".to_owned()),
            model: Some("gpt".to_owned()),
            timeout_ms: Some(2_500),
            context_lines: 4,
            ..scene("rewrite", 2)
        };
        let request = CommandTextRequest::from_text_request(
            "cmd-adapter",
            &TextRequest {
                raw_text: "raw text",
                scene: &prompted,
                selected_text: Some("selection"),
            },
        );
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["adapter_id"], "cmd-adapter");
        assert_eq!(value["raw_text"], "raw text");
        assert_eq!(value["selected_text"], "selection");
        assert_eq!(value["scene"]["id"], "rewrite");
        assert_eq!(value["scene"]["prompt"], "polish");
        assert_eq!(value["scene"]["provider_id"], "openai");
        assert_eq!(value["scene"]["model"], "gpt");
        assert_eq!(value["scene"]["candidate_count"], 2);
        assert_eq!(value["scene"]["timeout_ms"], 2_500);
        assert_eq!(value["scene"]["context_lines"], 4);
    }

    #[test]
    fn command_text_request_preserves_missing_selected_text() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("rewrite", 1)
        };
        let request = CommandTextRequest::from_text_request(
            "cmd-adapter",
            &TextRequest {
                raw_text: "raw text",
                scene: &prompted,
                selected_text: None,
            },
        );

        assert!(request.selected_text.is_none());
        let value = serde_json::to_value(&request).unwrap();
        assert!(value["selected_text"].is_null());
    }

    #[test]
    fn openai_compatible_candidate_parser_extracts_first_choice_content_json() {
        let response = serde_json::json!({
            "choices": [{
                "message": {
                    "content": serde_json::json!({
                        "candidates": [" polished ", "", 7, "second"]
                    }).to_string()
                }
            }]
        });

        assert_eq!(
            extract_openai_compatible_candidates(&response.to_string()),
            vec!["polished".to_owned(), "second".to_owned()]
        );
    }

    #[test]
    fn openai_compatible_candidate_parser_returns_empty_for_invalid_shapes() {
        for response in [
            "not json".to_owned(),
            serde_json::json!({}).to_string(),
            serde_json::json!({"choices": []}).to_string(),
            serde_json::json!({"choices": [{"message": {"content": "not json"}}]}).to_string(),
            serde_json::json!({
                "choices": [{"message": {"content": serde_json::json!({"candidates": "no"}).to_string()}}]
            })
            .to_string(),
        ] {
            assert!(
                extract_openai_compatible_candidates(&response).is_empty(),
                "response should not yield candidates: {response}"
            );
        }
    }

    #[test]
    fn command_text_response_maps_final_text_to_payload() {
        let payload = CommandTextResponse {
            payload: None,
            text: Some("polished".to_owned()),
            error: None,
        }
        .into_payload()
        .unwrap();

        assert_eq!(payload.commit_text, "polished");
        assert_eq!(payload.candidates[0].text, "polished");
    }

    #[test]
    fn command_text_response_accepts_full_payload() {
        let response: CommandTextResponse = serde_json::from_str(
            r#"{"payload":{"commit_text":"choice","candidates":[{"text":"choice","source":"llm"}]}}"#,
        )
        .unwrap();
        let payload = response.into_payload().unwrap();

        assert_eq!(payload.commit_text, "choice");
        assert_eq!(payload.candidates[0].text, "choice");
        assert_eq!(payload.candidates[0].source.to_string(), "llm");
    }

    #[test]
    fn command_text_response_normalizes_full_payload() {
        let response: CommandTextResponse =
            serde_json::from_str(r#"{"payload":{"commit_text":"choice","candidates":[]}}"#)
                .unwrap();
        let payload = response.into_payload().unwrap();

        assert_eq!(payload.commit_text, "choice");
        assert_eq!(payload.candidates[0].text, "choice");
    }

    #[test]
    fn command_text_response_prefers_error_over_payload() {
        let response: CommandTextResponse = serde_json::from_str(
            r#"{"payload":{"commit_text":"choice","candidates":[]},"error":"adapter boom"}"#,
        )
        .unwrap();
        let error = response.into_payload().unwrap_err();

        assert_eq!(error, TextError::AdapterFailed("adapter boom".to_owned()));
    }

    #[test]
    fn command_text_response_prefers_payload_over_text() {
        let response: CommandTextResponse = serde_json::from_str(
            r#"{"payload":{"commit_text":"payload","candidates":[]},"text":"text fallback"}"#,
        )
        .unwrap();
        let payload = response.into_payload().unwrap();

        assert_eq!(payload.commit_text, "payload");
        assert_eq!(payload.candidates[0].text, "payload");
    }

    #[test]
    fn command_text_response_accepts_failure_alias() {
        let response: CommandTextResponse =
            serde_json::from_str(r#"{"failure":"adapter boom"}"#).unwrap();
        let error = response.into_payload().unwrap_err();

        assert_eq!(error, TextError::AdapterFailed("adapter boom".to_owned()));
    }

    #[test]
    fn command_text_response_rejects_blank_final_text() {
        let error = CommandTextResponse {
            payload: None,
            text: Some("   ".to_owned()),
            error: None,
        }
        .into_payload()
        .unwrap_err();

        assert!(matches!(
            error,
            TextError::AdapterFailed(message) if message.contains("missing final text")
        ));
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
        assert_eq!(
            registry
                .single_command_adapter()
                .map(CommandTextAdapter::command),
            Some("vinput-postprocess")
        );
    }

    #[test]
    fn adapter_registry_returns_no_single_adapter_for_empty_config() {
        let registry = super::AdapterRegistry::new();
        assert!(registry.single_command_adapter().is_none());
    }

    #[test]
    fn adapter_registry_returns_no_single_adapter_for_multiple_configs() {
        let registry = super::AdapterRegistry::from_configs(&[
            LlmAdapterConfig {
                id: "first".to_owned(),
                command: "first-command".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                working_dir: None,
                extra: std::collections::HashMap::default(),
            },
            LlmAdapterConfig {
                id: "second".to_owned(),
                command: "second-command".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                working_dir: None,
                extra: std::collections::HashMap::default(),
            },
        ]);
        assert!(registry.single_command_adapter().is_none());
    }

    #[test]
    fn command_text_processor_keeps_raw_scene_without_adapters() {
        let raw = scene(RAW_SCENE_ID, 0);
        let payload = CommandTextProcessor::from_configs(&[])
            .finish(&TextRequest {
                raw_text: "raw text",
                scene: &raw,
                selected_text: None,
            })
            .unwrap();

        assert_eq!(payload.commit_text, "raw text");
    }

    #[test]
    fn command_text_processor_requires_adapter_for_prompted_scene() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let error = CommandTextProcessor::from_configs(&[])
            .finish(&TextRequest {
                raw_text: "raw text",
                scene: &prompted,
                selected_text: None,
            })
            .unwrap_err();

        assert_eq!(error, TextError::AdapterRequired("polish".to_owned()));
    }

    #[test]
    fn command_text_processor_rejects_ambiguous_adapters_despite_provider_id() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            provider_id: Some("first".to_owned()),
            ..scene("polish", 0)
        };
        let processor = CommandTextProcessor::from_configs_with_runner(
            &[
                LlmAdapterConfig {
                    id: "first".to_owned(),
                    command: "first-command".to_owned(),
                    args: Vec::new(),
                    env: std::collections::HashMap::default(),
                    working_dir: None,
                    extra: std::collections::HashMap::default(),
                },
                LlmAdapterConfig {
                    id: "second".to_owned(),
                    command: "second-command".to_owned(),
                    args: Vec::new(),
                    env: std::collections::HashMap::default(),
                    working_dir: None,
                    extra: std::collections::HashMap::default(),
                },
            ],
            EchoCommandRunner,
        );
        let error = processor
            .finish(&TextRequest {
                raw_text: "raw text",
                scene: &prompted,
                selected_text: None,
            })
            .unwrap_err();

        assert_eq!(error, TextError::AmbiguousAdapter("polish".to_owned()));
    }

    #[test]
    fn command_text_processor_delegates_to_single_adapter() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let processor = CommandTextProcessor::from_configs_with_runner(
            &[LlmAdapterConfig {
                id: "cmd-adapter".to_owned(),
                command: "vinput-postprocess".to_owned(),
                args: vec!["--json".to_owned()],
                env: std::collections::HashMap::from([("MODE".to_owned(), "mock".to_owned())]),
                working_dir: Some("/tmp/vinput".to_owned()),
                extra: std::collections::HashMap::default(),
            }],
            EchoCommandRunner,
        );
        let payload = processor
            .finish(&TextRequest {
                raw_text: "raw text",
                scene: &prompted,
                selected_text: None,
            })
            .unwrap();

        assert_eq!(
            payload.commit_text,
            "vinput-postprocess --json mock /tmp/vinput: raw text"
        );
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
    fn process_command_text_runner_writes_request_and_reads_response() {
        let mut capture_path = std::env::temp_dir();
        capture_path.push(format!(
            "vinput-command-text-request-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat > "$TEXT_REQUEST"; printf '%s\n' '{"text":"polished final"}'"#.to_owned(),
            ],
            env: std::collections::HashMap::from([(
                "TEXT_REQUEST".to_owned(),
                capture_path.to_string_lossy().into_owned(),
            )]),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: Some("selection"),
        })
        .unwrap();
        assert_eq!(payload.commit_text, "polished final");

        let request: CommandTextRequest =
            serde_json::from_str(&std::fs::read_to_string(&capture_path).unwrap()).unwrap();
        std::fs::remove_file(&capture_path).unwrap();
        assert_eq!(request.adapter_id, "cmd-adapter");
        assert_eq!(request.raw_text, "raw text");
        assert_eq!(request.selected_text.as_deref(), Some("selection"));
        assert_eq!(request.scene.id, "polish");
        assert_eq!(request.scene.prompt.as_deref(), Some("polish"));
    }

    #[test]
    fn process_command_text_runner_reports_nonzero_exit() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "cat >/dev/null; echo adapter boom >&2; exit 7".to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            TextError::AdapterFailed(message)
                if message.contains("exited with") && message.contains("adapter boom")
        ));
    }

    #[test]
    fn process_command_text_runner_reports_missing_program() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: format!("vinput-missing-text-adapter-{}", std::process::id()),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            TextError::AdapterFailed(message)
                if message.contains("failed to spawn text adapter `cmd-adapter`")
        ));
    }

    #[test]
    fn process_command_text_runner_rejects_bad_json() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "cat >/dev/null; printf not-json".to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            TextError::AdapterFailed(message)
                if message.contains("failed to decode text adapter response")
        ));
    }

    #[test]
    fn process_command_text_runner_maps_helper_error_response() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s\n' '{"error":"adapter failed"}'"#.to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert_eq!(error, TextError::AdapterFailed("adapter failed".to_owned()));
    }

    #[test]
    fn process_command_text_runner_reads_payload_response() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s\n' '{"payload":{"commit_text":"payload final","candidates":[{"text":"payload final","source":"llm"}]}}'"#.to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap();

        assert_eq!(payload.commit_text, "payload final");
        assert_eq!(payload.candidates[0].text, "payload final");
        assert_eq!(payload.candidates[0].source.to_string(), "llm");
    }

    #[test]
    fn process_command_text_runner_reports_early_exit() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                "echo early adapter boom >&2; exit 9".to_owned(),
            ],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            TextError::AdapterFailed(message)
                if message.contains("exited with")
                    && message.contains("early adapter boom")
                    && !message.contains("failed to write")
        ));
    }

    #[test]
    fn process_command_text_runner_reports_empty_stderr_exit_cleanly() {
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec!["-c".to_owned(), "cat >/dev/null; exit 7".to_owned()],
            env: std::collections::HashMap::default(),
            working_dir: None,
            extra: std::collections::HashMap::default(),
        };

        let error = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            TextError::AdapterFailed(message)
                if message.contains("exited with") && !message.ends_with(':')
        ));
    }

    #[test]
    fn process_command_text_runner_uses_working_dir() {
        let mut work_dir = std::env::temp_dir();
        work_dir.push(format!(
            "vinput-command-text-workdir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir(&work_dir).unwrap();
        let mut capture_path = std::env::temp_dir();
        capture_path.push(format!(
            "vinput-command-text-cwd-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let prompted = SceneDefinition {
            prompt: Some("polish".to_owned()),
            ..scene("polish", 0)
        };
        let config = LlmAdapterConfig {
            id: "cmd-adapter".to_owned(),
            command: "sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r#"pwd > "$TEXT_CWD"; cat >/dev/null; printf '%s\n' '{"text":"cwd final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::from([(
                "TEXT_CWD".to_owned(),
                capture_path.to_string_lossy().into_owned(),
            )]),
            working_dir: Some(work_dir.to_string_lossy().into_owned()),
            extra: std::collections::HashMap::default(),
        };

        let payload = LlmTextProcessor::new(CommandTextAdapter::with_adapter_config(
            &config,
            ProcessCommandTextRunner,
        ))
        .finish(&TextRequest {
            raw_text: "raw text",
            scene: &prompted,
            selected_text: None,
        })
        .unwrap();

        assert_eq!(payload.commit_text, "cwd final");
        assert_eq!(
            std::fs::read_to_string(&capture_path).unwrap().trim(),
            work_dir.to_string_lossy()
        );
        std::fs::remove_file(&capture_path).unwrap();
        std::fs::remove_dir(&work_dir).unwrap();
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
