//! ASR backend contract, deterministic mock, and backend skeletons.
//!
//! This crate mirrors the original C++ daemon's recognition contract at a Rust
//! trait boundary. Real backends such as sherpa-onnx and command execution
//! should implement these traits after their contracts are covered by tests.

use std::{
    io::Write,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_audio::{PcmBuffer, PcmSpec, i16_samples_to_le_bytes};
use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};
use vinput_protocol::{AsrBackendState, CandidateSource, RecognitionPayload};

/// How audio should be delivered to an ASR session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioDeliveryMode {
    /// The backend expects all PCM samples after recording stops.
    Buffered,
    /// The backend accepts incremental PCM chunks while recording.
    Chunked,
}

/// Static backend capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BackendCapabilities {
    /// Whether this backend can emit partial recognition text.
    pub partial_results: bool,
    /// Preferred audio delivery mode.
    pub delivery_mode: AudioDeliveryMode,
}

impl BackendCapabilities {
    /// Capabilities for a simple buffered backend.
    #[must_use]
    pub const fn buffered() -> Self {
        Self {
            partial_results: false,
            delivery_mode: AudioDeliveryMode::Buffered,
        }
    }

    /// Capabilities for a streaming backend.
    #[must_use]
    pub const fn streaming() -> Self {
        Self {
            partial_results: true,
            delivery_mode: AudioDeliveryMode::Chunked,
        }
    }
}

/// Backend identity and capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BackendDescriptor {
    /// Stable provider id.
    pub provider_id: String,
    /// Stable model id.
    pub model_id: String,
    /// Human-readable backend label.
    pub label: String,
    /// Backend capability flags.
    pub capabilities: BackendCapabilities,
}

impl BackendDescriptor {
    /// Creates a descriptor with owned strings.
    #[must_use]
    pub fn new(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        label: impl Into<String>,
        capabilities: BackendCapabilities,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            label: label.into(),
            capabilities,
        }
    }
}

/// Event emitted by a recognition session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecognitionEvent {
    /// Streaming partial text.
    PartialText {
        /// Partial recognized text.
        text: String,
    },
    /// Final recognized text.
    FinalText {
        /// Final recognized text.
        text: String,
    },
    /// Backend error surfaced during recognition.
    Error {
        /// Human-readable error message.
        message: String,
    },
    /// Session has no more events.
    Completed,
}

/// Recognition context passed to concrete ASR backends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecognitionContext {
    /// Optional BCP-47-like language tag from config or scene policy.
    #[serde(default)]
    pub language: Option<String>,
    /// Scene id selected for this recognition session.
    pub scene_id: String,
    /// Whether this session is command mode.
    pub command_mode: bool,
    /// Optional selected text provided by the frontend for command mode.
    #[serde(default)]
    pub selected_text: Option<String>,
}

impl RecognitionContext {
    /// Creates a normal recognition context.
    #[must_use]
    pub fn normal(scene_id: impl Into<String>, language: Option<String>) -> Self {
        Self {
            language,
            scene_id: scene_id.into(),
            command_mode: false,
            selected_text: None,
        }
    }

    /// Creates a command-mode recognition context.
    #[must_use]
    pub fn command(
        scene_id: impl Into<String>,
        language: Option<String>,
        selected_text: impl Into<String>,
    ) -> Self {
        Self {
            language,
            scene_id: scene_id.into(),
            command_mode: true,
            selected_text: Some(selected_text.into()),
        }
    }
}

/// Mutable recognition session.
pub trait RecognitionSession: Send {
    /// Push signed 16-bit PCM samples with explicit layout metadata.
    fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        self.push_audio(pcm.samples())
    }

    /// Push raw signed 16-bit PCM samples using backend/default metadata.
    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError>;

    /// Finish audio delivery and let the backend enqueue final events.
    fn finish(&mut self) -> Result<(), AsrError>;

    /// Cancel work and enqueue no further result.
    fn cancel(&mut self) -> Result<(), AsrError>;

    /// Drain currently pending events.
    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError>;
}

/// ASR backend factory.
pub trait AsrBackend: Send + Sync {
    /// Returns backend identity and capabilities.
    fn describe(&self) -> BackendDescriptor;

    /// Creates a fresh recognition session for the given context.
    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError>;
}

/// Recognition errors.
#[derive(Debug, Error)]
pub enum AsrError {
    /// Audio was pushed after the session finished.
    #[error("recognition session is already finished")]
    AlreadyFinished,
    /// Session was cancelled.
    #[error("recognition session was cancelled")]
    Cancelled,
    /// The requested ASR provider is not present in config.
    #[error("ASR provider `{0}` is not configured")]
    UnknownProvider(String),
    /// Configured provider kind is recognized but not implemented yet.
    #[error("ASR provider `{provider_id}` of kind `{kind}` is not implemented yet")]
    UnsupportedProviderKind {
        /// Provider id.
        provider_id: String,
        /// Provider kind label.
        kind: String,
    },
    /// Backend-specific error.
    #[error("backend error: {0}")]
    Backend(String),
}

/// Deterministic ASR backend for tests and early daemon wiring.
#[derive(Debug, Clone)]
pub struct MockAsrBackend {
    descriptor: BackendDescriptor,
    final_text: String,
    partial_text: Option<String>,
    final_timing: MockFinalTiming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockFinalTiming {
    OnFinish,
    Early,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockSessionState {
    Active,
    Finished,
    Cancelled,
}

impl MockAsrBackend {
    /// Creates a buffered mock backend with fixed final text.
    #[must_use]
    pub fn buffered(final_text: impl Into<String>) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                "mock",
                "mock-buffered",
                "Mock buffered ASR",
                BackendCapabilities::buffered(),
            ),
            final_text: final_text.into(),
            partial_text: None,
            final_timing: MockFinalTiming::OnFinish,
        }
    }

    /// Creates a streaming mock backend with fixed partial and final text.
    #[must_use]
    pub fn streaming(partial_text: impl Into<String>, final_text: impl Into<String>) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                "mock",
                "mock-streaming",
                "Mock streaming ASR",
                BackendCapabilities::streaming(),
            ),
            final_text: final_text.into(),
            partial_text: Some(partial_text.into()),
            final_timing: MockFinalTiming::OnFinish,
        }
    }

    /// Creates a streaming mock backend that emits its final text before the session is closed.
    #[must_use]
    pub fn streaming_with_early_final(
        partial_text: impl Into<String>,
        final_text: impl Into<String>,
    ) -> Self {
        Self {
            final_timing: MockFinalTiming::Early,
            ..Self::streaming(partial_text, final_text)
        }
    }
}

/// Parsed external command ASR provider specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAsrSpec {
    /// Provider id from config.
    pub provider_id: String,
    /// Executable path or command name.
    pub command: String,
    /// Arguments passed to the command.
    pub args: Vec<String>,
    /// Environment variables passed to the command.
    pub env: std::collections::HashMap<String, String>,
    /// Optional model id selected for this provider.
    pub model_id: Option<String>,
    /// Optional hotwords file configured for this provider.
    pub hotwords_file: Option<String>,
    /// Optional timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl TryFrom<&AsrProviderConfig> for CommandAsrSpec {
    type Error = AsrError;

    fn try_from(provider: &AsrProviderConfig) -> Result<Self, Self::Error> {
        if provider.kind != AsrProviderKind::Command {
            return Err(AsrError::Backend(format!(
                "provider `{}` is not a command ASR provider",
                provider.id
            )));
        }
        let command = provider
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .ok_or_else(|| {
                AsrError::Backend(format!(
                    "command ASR provider `{}` must configure a command",
                    provider.id
                ))
            })?;
        Ok(Self {
            provider_id: provider.id.clone(),
            command: command.to_owned(),
            args: provider.args.clone(),
            env: provider.env.clone(),
            model_id: provider.model.clone(),
            hotwords_file: provider.hotwords_file.clone(),
            timeout_ms: provider.timeout_ms,
        })
    }
}

/// Buffered request passed to command-backed ASR runners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommandAsrRequest {
    /// Provider id selected for this request.
    pub provider_id: String,
    /// Optional model id selected for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Optional hotwords file configured for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotwords_file: Option<String>,
    /// Optional request timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Recognition context from the active scene or command mode.
    pub context: RecognitionContext,
    /// PCM layout metadata for the buffered signed 16-bit samples.
    #[serde(default)]
    pub pcm: PcmSpec,
    /// Buffered signed 16-bit PCM samples, interleaved when channel count is greater than one.
    pub samples: Vec<i16>,
}

impl CommandAsrRequest {
    /// Creates a buffered request from parsed provider metadata and runtime context.
    #[must_use]
    pub fn from_spec(
        spec: &CommandAsrSpec,
        context: RecognitionContext,
        samples: Vec<i16>,
    ) -> Self {
        Self::from_spec_with_pcm(spec, context, PcmSpec::default(), samples)
    }

    /// Creates a buffered request with explicit PCM metadata.
    #[must_use]
    pub fn from_spec_with_pcm(
        spec: &CommandAsrSpec,
        context: RecognitionContext,
        pcm: PcmSpec,
        samples: Vec<i16>,
    ) -> Self {
        Self {
            provider_id: spec.provider_id.clone(),
            model_id: spec.model_id.clone(),
            hotwords_file: spec.hotwords_file.clone(),
            timeout_ms: spec.timeout_ms,
            context,
            pcm,
            samples,
        }
    }
}

/// Response returned by a command-backed ASR helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct CommandAsrResponse {
    /// Optional streaming partial text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_text: Option<String>,
    /// Final recognized text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Backend error message produced by the helper.
    #[serde(default, alias = "failure", skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl CommandAsrResponse {
    /// Converts a helper response into recognition events.
    pub fn into_events(self) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut events = Vec::new();
        if let Some(partial_text) = self.partial_text.filter(|text| !text.trim().is_empty()) {
            events.push(RecognitionEvent::PartialText { text: partial_text });
        }
        if let Some(message) = self.error.filter(|message| !message.trim().is_empty()) {
            events.push(RecognitionEvent::Error { message });
            events.push(RecognitionEvent::Completed);
            return Ok(events);
        }
        let Some(text) = self.text.filter(|text| !text.trim().is_empty()) else {
            return Err(AsrError::Backend(
                "command ASR response missing final text".to_owned(),
            ));
        };
        events.push(RecognitionEvent::FinalText { text });
        events.push(RecognitionEvent::Completed);
        Ok(events)
    }
}

/// Runner seam for command-backed ASR providers.
pub trait CommandAsrRunner: Send + Sync {
    /// Recognizes one buffered command ASR request.
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError>;
}

/// Runner placeholder used until process execution is ported.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedCommandAsrRunner;

impl CommandAsrRunner for UnsupportedCommandAsrRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        _request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        Err(AsrError::Backend(format!(
            "command ASR provider `{}` runner is not implemented yet",
            spec.provider_id
        )))
    }
}

/// Builds a legacy command-streaming audio JSON line.
///
/// The line contains raw signed 16-bit little-endian PCM bytes encoded as
/// base64 and a `commit` flag indicating whether the chunk finalizes audio.
#[must_use]
pub fn legacy_command_streaming_audio_line(samples: &[i16], commit: bool) -> String {
    serde_json::json!({
        "type": "audio",
        "audio_base64": encode_base64(&i16_le_pcm_bytes(samples)),
        "commit": commit,
    })
    .to_string()
}

/// Builds a legacy command-streaming finish control JSON line.
#[must_use]
pub fn legacy_command_streaming_finish_line() -> String {
    serde_json::json!({"type": "finish"}).to_string()
}

fn i16_le_pcm_bytes(samples: &[i16]) -> Vec<u8> {
    i16_samples_to_le_bytes(samples)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

/// Parses one legacy command-streaming JSON line into recognition events.
///
/// Supported legacy event types are `session_started`, `partial`, `final`,
/// `final_timestamps`, `error`, and `closed`. Unknown event types are ignored.
pub fn parse_legacy_command_streaming_line(line: &str) -> Result<Vec<RecognitionEvent>, AsrError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(Vec::new());
    }
    let payload = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|error| AsrError::Backend(format!("invalid streaming provider JSON: {error}")))?;
    let event_type = payload
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match event_type {
        "partial" => Ok(payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                vec![RecognitionEvent::PartialText {
                    text: text.to_owned(),
                }]
            })
            .unwrap_or_default()),
        "final" | "final_timestamps" => Ok(payload
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| {
                vec![RecognitionEvent::FinalText {
                    text: text.to_owned(),
                }]
            })
            .unwrap_or_default()),
        "error" => Ok(vec![RecognitionEvent::Error {
            message: payload
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .unwrap_or("failed.")
                .to_owned(),
        }]),
        "closed" => Ok(vec![RecognitionEvent::Completed]),
        _ => Ok(Vec::new()),
    }
}

/// Legacy process runner for command ASR providers.
///
/// The original C++ batch command backend writes raw signed 16-bit little-endian
/// PCM bytes to stdin and treats trimmed stdout as the final recognized text.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyCommandBatchRunner;

impl CommandAsrRunner for LegacyCommandBatchRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                AsrError::Backend(format!(
                    "failed to spawn legacy command ASR provider `{}`: {error}",
                    spec.provider_id
                ))
            })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            AsrError::Backend(format!(
                "legacy command ASR provider `{}` did not expose stdin",
                spec.provider_id
            ))
        })?;
        let write_result = write_i16_le_pcm(&mut stdin, &request.samples).map_err(|error| {
            AsrError::Backend(format!(
                "failed to write legacy command ASR PCM for `{}`: {error}",
                spec.provider_id
            ))
        });
        drop(stdin);

        if let Err(write_error) = write_result {
            let output = wait_for_command_output(spec, child)?;
            if !output.status.success() {
                return command_exit_error(spec, &output);
            }
            return Err(write_error);
        }

        let output = wait_for_command_output(spec, child)?;
        if !output.status.success() {
            return command_exit_error(spec, &output);
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if text.is_empty() {
            return Err(AsrError::Backend(format!(
                "legacy command ASR provider `{}` returned no text",
                spec.provider_id
            )));
        }
        Ok(vec![
            RecognitionEvent::FinalText { text },
            RecognitionEvent::Completed,
        ])
    }
}

fn write_i16_le_pcm(mut writer: impl Write, samples: &[i16]) -> std::io::Result<()> {
    for sample in samples {
        writer.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

/// Process runner for legacy command-streaming ASR providers.
///
/// This runner sends the legacy JSON-line protocol using one committed audio
/// chunk followed by a finish control event, then parses stdout JSON event
/// lines. A fully incremental long-lived session can build on the same payload
/// and event helpers later.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyCommandStreamingRunner;

impl CommandAsrRunner for LegacyCommandStreamingRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                AsrError::Backend(format!(
                    "failed to spawn legacy command streaming ASR provider `{}`: {error}",
                    spec.provider_id
                ))
            })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            AsrError::Backend(format!(
                "legacy command streaming ASR provider `{}` did not expose stdin",
                spec.provider_id
            ))
        })?;
        let write_result = (|| {
            stdin.write_all(
                legacy_command_streaming_audio_line(&request.samples, true).as_bytes(),
            )?;
            stdin.write_all(
                b"
",
            )?;
            stdin.write_all(legacy_command_streaming_finish_line().as_bytes())?;
            stdin.write_all(
                b"
",
            )?;
            Ok::<(), std::io::Error>(())
        })()
        .map_err(|error| {
            AsrError::Backend(format!(
                "failed to write legacy command streaming events for `{}`: {error}",
                spec.provider_id
            ))
        });
        drop(stdin);

        if let Err(write_error) = write_result {
            let output = wait_for_command_output(spec, child)?;
            if !output.status.success() {
                return command_exit_error(spec, &output);
            }
            return Err(write_error);
        }

        let output = wait_for_command_output(spec, child)?;
        if !output.status.success() {
            return command_exit_error(spec, &output);
        }
        parse_legacy_command_streaming_stdout(&output.stdout)
    }
}

fn parse_legacy_command_streaming_stdout(stdout: &[u8]) -> Result<Vec<RecognitionEvent>, AsrError> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut events = Vec::new();
    let mut last_partial_text = String::new();
    for line in stdout.lines() {
        for event in parse_legacy_command_streaming_line(line)? {
            match &event {
                RecognitionEvent::PartialText { text } if text == &last_partial_text => {}
                RecognitionEvent::PartialText { text } => {
                    last_partial_text.clone_from(text);
                    events.push(event);
                }
                RecognitionEvent::FinalText { .. } => {
                    last_partial_text.clear();
                    events.push(event);
                }
                _ => events.push(event),
            }
        }
    }
    if events.is_empty() {
        return Err(AsrError::Backend(
            "legacy command streaming provider returned no events".to_owned(),
        ));
    }
    if !matches!(events.last(), Some(RecognitionEvent::Completed)) {
        events.push(RecognitionEvent::Completed);
    }
    Ok(events)
}

/// Process runner for command-backed ASR providers.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessCommandAsrRunner;

impl CommandAsrRunner for ProcessCommandAsrRunner {
    fn recognize(
        &self,
        spec: &CommandAsrSpec,
        request: &CommandAsrRequest,
    ) -> Result<Vec<RecognitionEvent>, AsrError> {
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .envs(&spec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                AsrError::Backend(format!(
                    "failed to spawn command ASR provider `{}`: {error}",
                    spec.provider_id
                ))
            })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            AsrError::Backend(format!(
                "command ASR provider `{}` did not expose stdin",
                spec.provider_id
            ))
        })?;
        let write_result = (|| {
            serde_json::to_writer(&mut stdin, request).map_err(|error| {
                AsrError::Backend(format!(
                    "failed to encode command ASR request for `{}`: {error}",
                    spec.provider_id
                ))
            })?;
            stdin.write_all(b"\n").map_err(|error| {
                AsrError::Backend(format!(
                    "failed to write command ASR request for `{}`: {error}",
                    spec.provider_id
                ))
            })?;
            Ok(())
        })();
        drop(stdin);

        if let Err(write_error) = write_result {
            let output = wait_for_command_output(spec, child)?;
            if !output.status.success() {
                return command_exit_error(spec, &output);
            }
            return Err(write_error);
        }

        let output = wait_for_command_output(spec, child)?;
        if !output.status.success() {
            return command_exit_error(spec, &output);
        }
        let response: CommandAsrResponse =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                AsrError::Backend(format!(
                    "failed to decode command ASR response for `{}`: {error}",
                    spec.provider_id
                ))
            })?;
        response.into_events()
    }
}

fn command_exit_error(
    spec: &CommandAsrSpec,
    output: &Output,
) -> Result<Vec<RecognitionEvent>, AsrError> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AsrError::Backend(format!(
        "command ASR provider `{}` exited with {}: {}",
        spec.provider_id,
        output.status,
        stderr.trim()
    )))
}

fn wait_for_command_output(spec: &CommandAsrSpec, mut child: Child) -> Result<Output, AsrError> {
    let Some(timeout_ms) = spec.timeout_ms else {
        return child.wait_with_output().map_err(|error| {
            AsrError::Backend(format!(
                "failed to wait for command ASR provider `{}`: {error}",
                spec.provider_id
            ))
        });
    };

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if child
            .try_wait()
            .map_err(|error| {
                AsrError::Backend(format!(
                    "failed to poll command ASR provider `{}`: {error}",
                    spec.provider_id
                ))
            })?
            .is_some()
        {
            return child.wait_with_output().map_err(|error| {
                AsrError::Backend(format!(
                    "failed to collect command ASR provider `{}` output: {error}",
                    spec.provider_id
                ))
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AsrError::Backend(format!(
                "command ASR provider `{}` timed out after {} ms",
                spec.provider_id, timeout_ms
            )));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// Command-backed ASR backend skeleton.
#[derive(Debug, Clone)]
pub struct CommandAsrBackend<R = UnsupportedCommandAsrRunner> {
    spec: CommandAsrSpec,
    descriptor: BackendDescriptor,
    runner: R,
}

impl CommandAsrBackend<UnsupportedCommandAsrRunner> {
    /// Creates a command ASR backend skeleton from a parsed spec.
    #[must_use]
    pub fn new(spec: CommandAsrSpec) -> Self {
        Self::with_runner(spec, UnsupportedCommandAsrRunner)
    }
}

impl<R> CommandAsrBackend<R> {
    /// Creates a command ASR backend with an injected runner.
    #[must_use]
    pub fn with_runner(spec: CommandAsrSpec, runner: R) -> Self {
        let descriptor = BackendDescriptor::new(
            spec.provider_id.clone(),
            spec.model_id.clone().unwrap_or_default(),
            "Command ASR",
            BackendCapabilities::buffered(),
        );
        Self {
            spec,
            descriptor,
            runner,
        }
    }

    /// Creates a command ASR backend from typed provider config with an injected runner.
    pub fn with_config(provider: &AsrProviderConfig, runner: R) -> Result<Self, AsrError> {
        Ok(Self::with_runner(
            CommandAsrSpec::try_from(provider)?,
            runner,
        ))
    }

    /// Returns the parsed command provider spec.
    #[must_use]
    pub const fn spec(&self) -> &CommandAsrSpec {
        &self.spec
    }

    /// Returns the configured command runner.
    #[must_use]
    pub const fn runner(&self) -> &R {
        &self.runner
    }
}

impl<R: CommandAsrRunner + Clone + 'static> AsrBackend for CommandAsrBackend<R> {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(CommandRecognitionSession {
            spec: self.spec.clone(),
            context,
            runner: self.runner.clone(),
            pcm: PcmSpec::default(),
            samples: Vec::new(),
            finished: false,
            cancelled: false,
            events: Vec::new(),
        }))
    }
}

/// Builds ASR backends from typed config entries.
#[derive(Debug, Clone, Copy, Default)]
pub struct AsrBackendFactory;

impl AsrBackendFactory {
    /// Creates a factory.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the active backend from ASR config.
    pub fn build_active(config: &AsrConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        let provider = active_provider(config)
            .ok_or_else(|| AsrError::UnknownProvider(config.active_provider.clone()))?;
        Self::build_provider(provider)
    }

    /// Parses an external command ASR provider into an executable spec.
    pub fn command_spec(provider: &AsrProviderConfig) -> Result<CommandAsrSpec, AsrError> {
        CommandAsrSpec::try_from(provider)
    }

    /// Builds a backend from one provider entry.
    pub fn build_provider(provider: &AsrProviderConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        if provider.id == "mock" {
            return Ok(Box::new(MockAsrBackend::streaming(
                "mock partial",
                "mock recognition result",
            )));
        }
        if provider.kind == AsrProviderKind::Command {
            if is_legacy_streaming_command_provider(&provider.id) {
                return Ok(Box::new(CommandAsrBackend::with_config(
                    provider,
                    LegacyCommandStreamingRunner,
                )?));
            }
            return Ok(Box::new(CommandAsrBackend::with_config(
                provider,
                LegacyCommandBatchRunner,
            )?));
        }
        unsupported_provider(&provider.id, &provider.kind)
    }

    /// Builds a user-facing ASR state snapshot from config and load outcome.
    #[must_use]
    pub fn state_for_config(config: &AsrConfig) -> AsrBackendState {
        let target_model_id = target_model_id(config);
        let remote_endpoints = remote_endpoints(config);
        match Self::build_active(config) {
            Ok(backend) => {
                let descriptor = backend.describe();
                let mut state = AsrBackendState::ready(descriptor.provider_id, descriptor.model_id);
                state.target_provider_id.clone_from(&config.active_provider);
                state.target_model_id = target_model_id;
                state.remote_endpoints = remote_endpoints;
                state
            }
            Err(error) => {
                let mut state = AsrBackendState::unavailable(
                    config.active_provider.clone(),
                    target_model_id,
                    error.to_string(),
                );
                state.remote_endpoints = remote_endpoints;
                state
            }
        }
    }
}

fn is_legacy_streaming_command_provider(provider_id: &str) -> bool {
    provider_id.ends_with(".streaming")
}

fn active_provider(config: &AsrConfig) -> Option<&AsrProviderConfig> {
    config
        .providers
        .iter()
        .find(|provider| provider.id == config.active_provider)
}

fn target_model_id(config: &AsrConfig) -> String {
    active_provider(config)
        .and_then(|provider| provider.model.clone())
        .unwrap_or_default()
}

fn remote_endpoints(config: &AsrConfig) -> Vec<String> {
    active_provider(config)
        .and_then(|provider| provider.endpoint.as_deref())
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(|endpoint| vec![endpoint.to_owned()])
        .unwrap_or_default()
}

fn unsupported_provider(
    provider_id: &str,
    kind: &AsrProviderKind,
) -> Result<Box<dyn AsrBackend>, AsrError> {
    Err(AsrError::UnsupportedProviderKind {
        provider_id: provider_id.to_owned(),
        kind: provider_kind_label(kind).to_owned(),
    })
}

fn provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}
impl AsrBackend for MockAsrBackend {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(MockRecognitionSession {
            final_text: self.final_text.clone(),
            partial_text: self.partial_text.clone(),
            final_timing: self.final_timing,
            accepted_samples: 0,
            state: MockSessionState::Active,
            partial_sent: false,
            final_sent: false,
            events: Vec::new(),
        }))
    }
}

/// Converts recognition events into a legacy result payload.
pub fn events_to_payload(events: &[RecognitionEvent]) -> Result<RecognitionPayload, AsrError> {
    let final_text = events.iter().find_map(|event| match event {
        RecognitionEvent::FinalText { text } => Some(text.as_str()),
        RecognitionEvent::Error { message } => Some(message.as_str()),
        RecognitionEvent::PartialText { .. } | RecognitionEvent::Completed => None,
    });

    match final_text {
        Some(text) => Ok(RecognitionPayload {
            commit_text: text.to_owned(),
            candidates: vec![vinput_protocol::Candidate::new(text, CandidateSource::Raw)],
        }),
        None => Err(AsrError::Backend(
            "recognition completed without final text".to_owned(),
        )),
    }
}

#[derive(Debug)]
struct CommandRecognitionSession<R> {
    spec: CommandAsrSpec,
    context: RecognitionContext,
    runner: R,
    pcm: PcmSpec,
    samples: Vec<i16>,
    finished: bool,
    cancelled: bool,
    events: Vec<RecognitionEvent>,
}

impl<R: CommandAsrRunner> RecognitionSession for CommandRecognitionSession<R> {
    fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        let next_pcm = pcm.spec();
        if !self.samples.is_empty() && self.pcm != next_pcm {
            return Err(AsrError::Backend(format!(
                "command ASR PCM spec changed from {} Hz/{} channel(s) to {} Hz/{} channel(s)",
                self.pcm.sample_rate_hz,
                self.pcm.channels,
                next_pcm.sample_rate_hz,
                next_pcm.channels
            )));
        }
        self.pcm = next_pcm;
        self.samples.extend_from_slice(pcm.samples());
        Ok(())
    }

    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.finished = true;
        let request = CommandAsrRequest::from_spec_with_pcm(
            &self.spec,
            self.context.clone(),
            self.pcm,
            self.samples.clone(),
        );
        self.events = self.runner.recognize(&self.spec, &request)?;
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.cancelled = true;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

#[derive(Debug)]
struct MockRecognitionSession {
    final_text: String,
    partial_text: Option<String>,
    final_timing: MockFinalTiming,
    accepted_samples: usize,
    state: MockSessionState,
    partial_sent: bool,
    final_sent: bool,
    events: Vec<RecognitionEvent>,
}

impl RecognitionSession for MockRecognitionSession {
    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        match self.state {
            MockSessionState::Active => {}
            MockSessionState::Finished => return Err(AsrError::AlreadyFinished),
            MockSessionState::Cancelled => return Err(AsrError::Cancelled),
        }
        self.accepted_samples += samples.len();
        if !self.partial_sent
            && let Some(text) = &self.partial_text
        {
            self.events
                .push(RecognitionEvent::PartialText { text: text.clone() });
            self.partial_sent = true;
        }
        if self.final_timing == MockFinalTiming::Early && !self.final_sent {
            self.events.push(RecognitionEvent::FinalText {
                text: self.final_text.clone(),
            });
            self.final_sent = true;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        match self.state {
            MockSessionState::Active => {}
            MockSessionState::Finished => return Err(AsrError::AlreadyFinished),
            MockSessionState::Cancelled => return Err(AsrError::Cancelled),
        }
        self.state = MockSessionState::Finished;
        if !self.final_sent {
            self.events.push(RecognitionEvent::FinalText {
                text: self.final_text.clone(),
            });
            self.final_sent = true;
        }
        self.events.push(RecognitionEvent::Completed);
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.state = MockSessionState::Cancelled;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsrBackend, AsrBackendFactory, AsrError, AudioDeliveryMode, CommandAsrBackend,
        CommandAsrRequest, CommandAsrResponse, CommandAsrRunner, CommandAsrSpec,
        LegacyCommandBatchRunner, LegacyCommandStreamingRunner, MockAsrBackend,
        ProcessCommandAsrRunner, RecognitionContext, RecognitionEvent, events_to_payload,
        legacy_command_streaming_audio_line, legacy_command_streaming_finish_line,
        parse_legacy_command_streaming_line,
    };
    use vinput_audio::{PcmBuffer, PcmSpec};
    use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};

    fn write_temp_script(prefix: &str, body: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{}-{}-{}.py",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        std::fs::write(&path, body).expect("write temporary script");
        path
    }

    #[derive(Debug, Clone, Copy)]
    struct FinalTextCommandRunner;

    impl CommandAsrRunner for FinalTextCommandRunner {
        fn recognize(
            &self,
            spec: &CommandAsrSpec,
            request: &CommandAsrRequest,
        ) -> Result<Vec<RecognitionEvent>, AsrError> {
            CommandAsrResponse {
                text: Some(format!(
                    "{}:{}:{}",
                    spec.command,
                    request.context.scene_id,
                    request.samples.len()
                )),
                ..CommandAsrResponse::default()
            }
            .into_events()
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ConfigEchoCommandRunner;

    #[derive(Debug, Clone, Copy)]
    struct PcmEchoCommandRunner;

    impl CommandAsrRunner for PcmEchoCommandRunner {
        fn recognize(
            &self,
            _spec: &CommandAsrSpec,
            request: &CommandAsrRequest,
        ) -> Result<Vec<RecognitionEvent>, AsrError> {
            CommandAsrResponse {
                text: Some(format!(
                    "{}|{}|{}",
                    request.pcm.sample_rate_hz,
                    request.pcm.channels,
                    request.samples.len()
                )),
                ..CommandAsrResponse::default()
            }
            .into_events()
        }
    }

    impl CommandAsrRunner for ConfigEchoCommandRunner {
        fn recognize(
            &self,
            spec: &CommandAsrSpec,
            request: &CommandAsrRequest,
        ) -> Result<Vec<RecognitionEvent>, AsrError> {
            let scene_id = request.context.scene_id.clone();
            let language = request.context.language.clone().unwrap_or_default();
            let env_value = spec
                .env
                .get("ASR_MODE")
                .map(String::as_str)
                .unwrap_or_default();
            Ok(vec![
                RecognitionEvent::FinalText {
                    text: format!(
                        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                        request.provider_id,
                        spec.command,
                        spec.args.join(","),
                        env_value,
                        request.model_id.as_deref().unwrap_or_default(),
                        request.hotwords_file.as_deref().unwrap_or_default(),
                        request.timeout_ms.unwrap_or_default(),
                        scene_id,
                        language,
                        request.samples.len(),
                    ),
                },
                RecognitionEvent::Completed,
            ])
        }
    }

    #[test]
    fn recognition_context_marks_command_sessions() {
        let context =
            super::RecognitionContext::command("__command__", Some("zh".to_owned()), "text");
        assert!(context.command_mode);
        assert_eq!(context.scene_id, "__command__");
        assert_eq!(context.language.as_deref(), Some("zh"));
        assert_eq!(context.selected_text.as_deref(), Some("text"));
    }

    #[test]
    fn mock_buffered_backend_emits_final_text_on_finish() {
        let backend = MockAsrBackend::buffered("hello");
        let descriptor = backend.describe();
        assert_eq!(
            descriptor.capabilities.delivery_mode,
            AudioDeliveryMode::Buffered
        );

        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.push_audio(&[1, 2, 3]).unwrap();
        assert!(session.poll_events().unwrap().is_empty());
        session.finish().unwrap();
        let events = session.poll_events().unwrap();
        assert_eq!(
            events,
            vec![
                RecognitionEvent::FinalText {
                    text: "hello".to_owned()
                },
                RecognitionEvent::Completed
            ]
        );
        assert_eq!(events_to_payload(&events).unwrap().commit_text, "hello");
    }

    #[test]
    fn mock_streaming_backend_emits_partial_once() {
        let backend = MockAsrBackend::streaming("partial", "final");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.push_audio(&[1]).unwrap();
        assert_eq!(
            session.poll_events().unwrap(),
            vec![RecognitionEvent::PartialText {
                text: "partial".to_owned()
            }]
        );
        session.push_audio(&[2]).unwrap();
        assert!(session.poll_events().unwrap().is_empty());
    }

    #[test]
    fn mock_streaming_backend_can_emit_final_before_finish() {
        let backend = MockAsrBackend::streaming_with_early_final("partial", "final");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();

        session.push_audio(&[1]).unwrap();
        let events = session.poll_events().unwrap();
        assert_eq!(
            events,
            vec![
                RecognitionEvent::PartialText {
                    text: "partial".to_owned()
                },
                RecognitionEvent::FinalText {
                    text: "final".to_owned()
                }
            ]
        );
        assert_eq!(events_to_payload(&events).unwrap().commit_text, "final");

        session.finish().unwrap();
        assert_eq!(
            session.poll_events().unwrap(),
            vec![RecognitionEvent::Completed]
        );
    }

    #[test]
    fn error_event_maps_to_payload() {
        let payload = events_to_payload(&[RecognitionEvent::Error {
            message: "err".to_owned(),
        }])
        .unwrap();
        assert_eq!(payload.commit_text, "err");
    }

    #[test]
    fn events_without_final_text_return_error() {
        let error = events_to_payload(&[RecognitionEvent::Completed]).unwrap_err();
        assert!(
            matches!(error, AsrError::Backend(message) if message.contains("without final text"))
        );
    }

    #[test]
    fn session_rejects_work_after_cancel() {
        let backend = MockAsrBackend::buffered("done");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.push_audio(&[1, 2]).unwrap();
        session.cancel().unwrap();

        assert!(session.poll_events().unwrap().is_empty());
        assert!(matches!(
            session.push_audio(&[3]).unwrap_err(),
            AsrError::Cancelled
        ));
        assert!(matches!(session.finish().unwrap_err(), AsrError::Cancelled));
    }

    #[test]
    fn session_rejects_audio_after_finish() {
        let backend = MockAsrBackend::buffered("done");
        let mut session = backend
            .create_session(RecognitionContext::normal("__raw__", None))
            .unwrap();
        session.finish().unwrap();
        assert!(matches!(
            session.push_audio(&[1]).unwrap_err(),
            AsrError::AlreadyFinished
        ));
    }

    #[test]
    fn backend_factory_builds_mock_provider() {
        let config = AsrConfig {
            active_provider: "mock".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "mock".to_owned(),
                kind: AsrProviderKind::Local,
                timeout_ms: None,
                model: None,
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                endpoint: None,
            }],
            ..AsrConfig::default()
        };

        let backend = AsrBackendFactory::build_active(&config).unwrap();
        assert_eq!(backend.describe().provider_id, "mock");
    }

    #[test]
    fn backend_factory_reports_unknown_active_provider() {
        let config = AsrConfig {
            active_provider: "missing".to_owned(),
            providers: Vec::new(),
            ..AsrConfig::default()
        };

        let Err(error) = AsrBackendFactory::build_active(&config) else {
            panic!("missing provider should fail");
        };
        assert!(matches!(error, AsrError::UnknownProvider(id) if id == "missing"));
    }

    #[test]
    fn command_asr_spec_parses_provider_fields() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_500),
            model: Some("paraformer".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some(" helper ".to_owned()),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::from([("RUST_LOG".to_owned(), "info".to_owned())]),
            endpoint: None,
        };

        let spec = CommandAsrSpec::try_from(&provider).unwrap();
        assert_eq!(spec.provider_id, "cmd");
        assert_eq!(spec.command, "helper");
        assert_eq!(spec.args, ["--json"]);
        assert_eq!(spec.env.get("RUST_LOG").map(String::as_str), Some("info"));
        assert_eq!(spec.model_id.as_deref(), Some("paraformer"));
        assert_eq!(spec.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
        assert_eq!(spec.timeout_ms, Some(1_500));
    }
    #[test]
    fn command_asr_request_serializes_metadata_context_and_audio() {
        let spec = CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            model_id: Some("paraformer".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            timeout_ms: Some(1_500),
        };
        let request = CommandAsrRequest::from_spec(
            &spec,
            RecognitionContext::command("__command__", Some("zh".to_owned()), "selected"),
            vec![1, -2, 3],
        );
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["provider_id"], "cmd");
        assert_eq!(value["model_id"], "paraformer");
        assert_eq!(value["hotwords_file"], "/tmp/hotwords.txt");
        assert_eq!(value["timeout_ms"], 1_500);
        assert_eq!(value["context"]["scene_id"], "__command__");
        assert_eq!(value["context"]["command_mode"], true);
        assert_eq!(value["context"]["selected_text"], "selected");
        assert_eq!(value["pcm"]["sample_rate_hz"], 16_000);
        assert_eq!(value["pcm"]["channels"], 1);
        assert_eq!(value["samples"], serde_json::json!([1, -2, 3]));
        assert_eq!(
            serde_json::from_value::<CommandAsrRequest>(value).unwrap(),
            request
        );
    }

    #[test]
    fn command_asr_request_defaults_pcm_for_legacy_json() {
        let request: CommandAsrRequest = serde_json::from_str(
            r#"{
                "provider_id":"cmd",
                "context":{
                    "language":"zh",
                    "scene_id":"raw",
                    "command_mode":false
                },
                "samples":[1,2,3]
            }"#,
        )
        .unwrap();

        assert_eq!(request.pcm, PcmSpec::default());
        assert_eq!(request.samples, [1, 2, 3]);
    }

    #[test]
    fn command_asr_request_preserves_explicit_pcm_spec() {
        let spec = CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: None,
        };
        let pcm = PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        };
        let request = CommandAsrRequest::from_spec_with_pcm(
            &spec,
            RecognitionContext::normal("raw", None),
            pcm,
            vec![1, 2, 3, 4],
        );

        assert_eq!(request.pcm, pcm);
        assert_eq!(request.samples, [1, 2, 3, 4]);
    }

    #[test]
    fn command_asr_session_uses_pushed_pcm_metadata() {
        let backend = CommandAsrBackend::with_runner(
            CommandAsrSpec {
                provider_id: "cmd".to_owned(),
                command: "helper".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                model_id: None,
                hotwords_file: None,
                timeout_ms: None,
            },
            PcmEchoCommandRunner,
        );
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("command backend should create a buffering session");
        let pcm = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 48_000,
                channels: 2,
            },
            vec![1, 2, 3, 4],
        )
        .unwrap();

        session.push_pcm(&pcm).unwrap();
        session.finish().unwrap();

        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(payload.commit_text, "48000|2|4");
    }

    #[test]
    fn command_asr_session_rejects_mixed_pcm_metadata() {
        let backend = CommandAsrBackend::with_runner(
            CommandAsrSpec {
                provider_id: "cmd".to_owned(),
                command: "helper".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                model_id: None,
                hotwords_file: None,
                timeout_ms: None,
            },
            PcmEchoCommandRunner,
        );
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("command backend should create a buffering session");
        let first = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 48_000,
                channels: 2,
            },
            vec![1, 2],
        )
        .unwrap();
        let second = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 16_000,
                channels: 1,
            },
            vec![3],
        )
        .unwrap();

        session.push_pcm(&first).unwrap();
        let error = session.push_pcm(&second).unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message)
                if message.contains("PCM spec changed")
                    && message.contains("48000 Hz/2")
                    && message.contains("16000 Hz/1")
        ));
    }

    #[test]
    fn backend_factory_command_spec_uses_same_parser() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_500),
            model: Some("paraformer".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("helper".to_owned()),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let spec = AsrBackendFactory::command_spec(&provider).unwrap();
        assert_eq!(spec.provider_id, "cmd");
        assert_eq!(spec.command, "helper");
        assert_eq!(spec.model_id.as_deref(), Some("paraformer"));
        assert_eq!(spec.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
        assert_eq!(spec.timeout_ms, Some(1_500));
    }

    #[test]
    fn command_asr_spec_rejects_missing_command() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let error = CommandAsrSpec::try_from(&provider).unwrap_err();
        assert!(
            matches!(error, AsrError::Backend(message) if message.contains("must configure a command"))
        );
    }

    #[test]
    fn command_asr_backend_describes_configured_provider() {
        let backend = CommandAsrBackend::new(CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: vec!["--json".to_owned()],
            env: std::collections::HashMap::default(),
            model_id: Some("cmd-model".to_owned()),
            hotwords_file: None,
            timeout_ms: Some(1_000),
        });

        let descriptor = backend.describe();
        assert_eq!(descriptor.provider_id, "cmd");
        assert_eq!(descriptor.model_id, "cmd-model");
        assert_eq!(
            descriptor.capabilities.delivery_mode,
            AudioDeliveryMode::Buffered
        );
        assert_eq!(backend.spec().command, "helper");
    }

    #[test]
    fn command_asr_backend_delegates_to_injected_runner() {
        let backend = CommandAsrBackend::with_runner(
            CommandAsrSpec {
                provider_id: "cmd".to_owned(),
                command: "helper".to_owned(),
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                model_id: None,
                hotwords_file: None,
                timeout_ms: None,
            },
            FinalTextCommandRunner,
        );

        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("mock runner should create a session");
        session.push_audio(&[1, 2, 3]).unwrap();
        session.finish().unwrap();
        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(payload.commit_text, "helper:raw:3");
    }

    #[test]
    fn command_asr_backend_passes_config_to_injected_runner() {
        let backend = CommandAsrBackend::with_runner(
            CommandAsrSpec {
                provider_id: "cmd".to_owned(),
                command: "helper".to_owned(),
                args: vec!["--format".to_owned(), "json".to_owned()],
                env: std::collections::HashMap::from([("ASR_MODE".to_owned(), "fast".to_owned())]),
                model_id: Some("paraformer".to_owned()),
                hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
                timeout_ms: Some(2_500),
            },
            ConfigEchoCommandRunner,
        );

        let mut session = backend
            .create_session(RecognitionContext::normal(
                "dictation",
                Some("zh".to_owned()),
            ))
            .expect("mock runner should create a session");
        session.finish().unwrap();

        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(
            payload.commit_text,
            "cmd|helper|--format,json|fast|paraformer|/tmp/hotwords.txt|2500|dictation|zh|0"
        );
    }

    #[test]
    fn command_asr_backend_builds_from_provider_config_with_runner() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(2_500),
            model: Some("paraformer".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("helper".to_owned()),
            args: vec!["--format".to_owned(), "json".to_owned()],
            env: std::collections::HashMap::from([("ASR_MODE".to_owned(), "fast".to_owned())]),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ConfigEchoCommandRunner)
            .expect("command provider config should build");
        assert_eq!(backend.spec().provider_id, "cmd");
        assert_eq!(backend.spec().model_id.as_deref(), Some("paraformer"));
        assert_eq!(
            backend.spec().hotwords_file.as_deref(),
            Some("/tmp/hotwords.txt")
        );

        let mut session = backend
            .create_session(RecognitionContext::normal(
                "dictation",
                Some("zh".to_owned()),
            ))
            .expect("mock runner should create a session");
        session.finish().unwrap();

        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(
            payload.commit_text,
            "cmd|helper|--format,json|fast|paraformer|/tmp/hotwords.txt|2500|dictation|zh|0"
        );
    }

    #[test]
    fn legacy_command_batch_runner_writes_raw_little_endian_pcm() {
        let script_path = write_temp_script(
            "vinput-legacy-command-asr",
            r"
import struct
import sys
samples = [value[0] for value in struct.iter_unpack('<h', sys.stdin.buffer.read())]
sys.stdout.write('|'.join(str(sample) for sample in samples))
",
        );
        let spec = CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "python3".to_owned(),
            args: vec![script_path.to_string_lossy().into_owned()],
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: Some(1_000),
        };
        let request = CommandAsrRequest::from_spec(
            &spec,
            RecognitionContext::normal("raw", Some("zh".to_owned())),
            vec![1, -2, 258],
        );

        let events = LegacyCommandBatchRunner
            .recognize(&spec, &request)
            .expect("legacy runner should decode helper output");
        std::fs::remove_file(script_path).unwrap();

        assert_eq!(
            events,
            vec![
                RecognitionEvent::FinalText {
                    text: "1|-2|258".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn legacy_command_batch_runner_rejects_empty_stdout() {
        let spec = CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "sh".to_owned(),
            args: vec!["-c".to_owned(), "cat >/dev/null".to_owned()],
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: Some(1_000),
        };
        let request = CommandAsrRequest::from_spec(
            &spec,
            RecognitionContext::normal("raw", None),
            vec![1, 2, 3],
        );

        let error = LegacyCommandBatchRunner
            .recognize(&spec, &request)
            .unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message)
                if message.contains("legacy command ASR provider `cmd` returned no text")
        ));
    }

    #[test]
    fn legacy_command_streaming_audio_line_encodes_little_endian_pcm() {
        let line = legacy_command_streaming_audio_line(&[1, -2, 258], true);
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();

        assert_eq!(value["type"], "audio");
        assert_eq!(value["audio_base64"], "AQD+/wIB");
        assert_eq!(value["commit"], true);
    }

    #[test]
    fn legacy_command_streaming_finish_line_matches_control_event() {
        let value: serde_json::Value =
            serde_json::from_str(&legacy_command_streaming_finish_line()).unwrap();

        assert_eq!(value, serde_json::json!({"type": "finish"}));
    }

    #[test]
    fn legacy_command_streaming_line_parser_maps_known_events() {
        assert_eq!(
            parse_legacy_command_streaming_line(r#"{"type":"partial","text":" hello "}"#).unwrap(),
            vec![RecognitionEvent::PartialText {
                text: "hello".to_owned()
            }]
        );
        assert_eq!(
            parse_legacy_command_streaming_line(r#"{"type":"final","text":" done "}"#).unwrap(),
            vec![RecognitionEvent::FinalText {
                text: "done".to_owned()
            }]
        );
        assert_eq!(
            parse_legacy_command_streaming_line(
                r#"{"type":"final_timestamps","text":" timed final ","timestamps":[1]}"#,
            )
            .unwrap(),
            vec![RecognitionEvent::FinalText {
                text: "timed final".to_owned()
            }]
        );
        assert_eq!(
            parse_legacy_command_streaming_line(r#"{"type":"error","message":" boom "}"#).unwrap(),
            vec![RecognitionEvent::Error {
                message: "boom".to_owned()
            }]
        );
        assert_eq!(
            parse_legacy_command_streaming_line(r#"{"type":"closed"}"#).unwrap(),
            vec![RecognitionEvent::Completed]
        );
    }

    #[test]
    fn legacy_command_streaming_line_parser_ignores_noop_events() {
        for line in [
            "",
            "   ",
            r#"{"type":"session_started"}"#,
            r#"{"type":"partial","text":""}"#,
            r#"{"type":"final","text":""}"#,
            r#"{"type":"unknown","text":"ignored"}"#,
        ] {
            assert!(
                parse_legacy_command_streaming_line(line)
                    .unwrap()
                    .is_empty(),
                "line should not yield events: {line}"
            );
        }
    }

    #[test]
    fn legacy_command_streaming_line_parser_rejects_invalid_json() {
        let error = parse_legacy_command_streaming_line("not json").unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("invalid streaming provider JSON")
        ));
    }

    #[test]
    fn legacy_command_streaming_line_parser_defaults_blank_error_message() {
        assert_eq!(
            parse_legacy_command_streaming_line(r#"{"type":"error","message":""}"#).unwrap(),
            vec![RecognitionEvent::Error {
                message: "failed.".to_owned()
            }]
        );
    }

    #[test]
    fn legacy_command_streaming_runner_sends_audio_and_finish_lines() {
        let script_path = write_temp_script(
            "vinput-legacy-command-streaming-asr",
            r"
import base64
import json
import struct
import sys
lines = [json.loads(line) for line in sys.stdin if line.strip()]
audio = base64.b64decode(lines[0]['audio_base64'])
samples = [value[0] for value in struct.iter_unpack('<h', audio)]
print(json.dumps({'type':'partial','text':'partial'}))
print(json.dumps({'type':'final','text':'|'.join(str(sample) for sample in samples)}))
print(json.dumps({'type':'closed'}))
assert lines[0]['type'] == 'audio'
assert lines[0]['commit'] is True
assert lines[1]['type'] == 'finish'
",
        );
        let spec = CommandAsrSpec {
            provider_id: "cmd.streaming".to_owned(),
            command: "python3".to_owned(),
            args: vec![script_path.to_string_lossy().into_owned()],
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: Some(1_000),
        };
        let request = CommandAsrRequest::from_spec(
            &spec,
            RecognitionContext::normal("raw", Some("zh".to_owned())),
            vec![1, -2, 258],
        );

        let events = LegacyCommandStreamingRunner
            .recognize(&spec, &request)
            .expect("legacy streaming runner should parse helper events");
        std::fs::remove_file(script_path).unwrap();

        assert_eq!(
            events,
            vec![
                RecognitionEvent::PartialText {
                    text: "partial".to_owned()
                },
                RecognitionEvent::FinalText {
                    text: "1|-2|258".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn legacy_command_streaming_runner_deduplicates_repeated_partials() {
        let script_path = write_temp_script(
            "vinput-legacy-command-streaming-dedupe",
            r"
import json
import sys
for _ in sys.stdin:
    pass
print(json.dumps({'type':'partial','text':'same'}))
print(json.dumps({'type':'partial','text':'same'}))
print(json.dumps({'type':'partial','text':'next'}))
print(json.dumps({'type':'final','text':'done'}))
print(json.dumps({'type':'closed'}))
",
        );
        let spec = CommandAsrSpec {
            provider_id: "cmd.streaming".to_owned(),
            command: "python3".to_owned(),
            args: vec![script_path.to_string_lossy().into_owned()],
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: Some(1_000),
        };
        let request = CommandAsrRequest::from_spec(
            &spec,
            RecognitionContext::normal("raw", Some("zh".to_owned())),
            vec![1],
        );

        let events = LegacyCommandStreamingRunner
            .recognize(&spec, &request)
            .expect("legacy streaming runner should deduplicate repeated partials");
        std::fs::remove_file(script_path).unwrap();

        assert_eq!(
            events,
            vec![
                RecognitionEvent::PartialText {
                    text: "same".to_owned()
                },
                RecognitionEvent::PartialText {
                    text: "next".to_owned()
                },
                RecognitionEvent::FinalText {
                    text: "done".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn legacy_command_streaming_runner_rejects_empty_stdout() {
        let spec = CommandAsrSpec {
            provider_id: "cmd.streaming".to_owned(),
            command: "sh".to_owned(),
            args: vec!["-c".to_owned(), "cat >/dev/null".to_owned()],
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: Some(1_000),
        };
        let request = CommandAsrRequest::from_spec(
            &spec,
            RecognitionContext::normal("raw", None),
            vec![1, 2, 3],
        );

        let error = LegacyCommandStreamingRunner
            .recognize(&spec, &request)
            .unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message)
                if message.contains("legacy command streaming provider returned no events")
        ));
    }

    #[test]
    fn process_command_asr_runner_maps_partial_and_final_response() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"partial_text":"listening","text":"final"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        session.finish().unwrap();

        let events = session.poll_events().unwrap();
        assert_eq!(
            events,
            vec![
                RecognitionEvent::PartialText {
                    text: "listening".to_owned()
                },
                RecognitionEvent::FinalText {
                    text: "final".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
        assert_eq!(events_to_payload(&events).unwrap().commit_text, "final");
    }

    #[test]
    fn process_command_asr_runner_writes_request_and_reads_response() {
        let mut capture_path = std::env::temp_dir();
        capture_path.push(format!(
            "vinput-command-asr-request-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(2_500),
            model: Some("paraformer".to_owned()),
            hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat > "$ASR_REQUEST"; printf '%s\n' '{"text":"process final"}'"#.to_owned(),
            ],
            env: std::collections::HashMap::from([(
                "ASR_REQUEST".to_owned(),
                capture_path.to_string_lossy().into_owned(),
            )]),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::command(
                "__command__",
                Some("zh".to_owned()),
                "selected text",
            ))
            .expect("process runner should create a buffering session");
        let pcm = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 8_000,
                channels: 1,
            },
            vec![10, -20, 30],
        )
        .unwrap();
        session.push_pcm(&pcm).unwrap();
        session.finish().unwrap();
        let payload = events_to_payload(&session.poll_events().unwrap()).unwrap();
        assert_eq!(payload.commit_text, "process final");

        let request: CommandAsrRequest =
            serde_json::from_str(&std::fs::read_to_string(&capture_path).unwrap()).unwrap();
        std::fs::remove_file(&capture_path).unwrap();
        assert_eq!(request.provider_id, "cmd");
        assert_eq!(request.model_id.as_deref(), Some("paraformer"));
        assert_eq!(request.hotwords_file.as_deref(), Some("/tmp/hotwords.txt"));
        assert_eq!(request.timeout_ms, Some(2_500));
        assert_eq!(request.pcm.sample_rate_hz, 8_000);
        assert_eq!(request.pcm.channels, 1);
        assert!(request.context.command_mode);
        assert_eq!(
            request.context.selected_text.as_deref(),
            Some("selected text")
        );
        assert_eq!(request.samples, [10, -20, 30]);
    }

    #[test]
    fn process_command_asr_runner_reports_spawn_failure() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some(format!("vinput-missing-command-{}", std::process::id())),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message)
                if message.contains("failed to spawn command ASR provider `cmd`")
        ));
    }

    #[test]
    fn process_command_asr_runner_times_out_slow_helpers() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(25),
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec!["-c".to_owned(), "cat >/dev/null; sleep 1".to_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("timed out after 25 ms")
        ));
    }

    #[test]
    fn process_command_asr_runner_reports_early_nonzero_exit() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec!["-c".to_owned(), "echo early boom >&2; exit 9".to_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message)
                if message.contains("exited with")
                    && message.contains("early boom")
                    && !message.contains("failed to write")
        ));
    }

    #[test]
    fn process_command_asr_runner_reports_nonzero_exit() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                "cat >/dev/null; echo boom >&2; exit 7".to_owned(),
            ],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("exited with") && message.contains("boom")
        ));
    }

    #[test]
    fn process_command_asr_runner_rejects_invalid_json_response() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                "cat >/dev/null; printf not-json".to_owned(),
            ],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("failed to decode command ASR response")
        ));
    }

    #[test]
    fn process_command_asr_runner_rejects_missing_final_text_response() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec!["-c".to_owned(), "cat >/dev/null; printf '{}'".to_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("missing final text")
        ));
    }

    #[test]
    fn command_asr_response_accepts_failure_alias() {
        let response: CommandAsrResponse =
            serde_json::from_str(r#"{"failure":"legacy failed"}"#).unwrap();
        let events = response.into_events().unwrap();
        assert_eq!(
            events_to_payload(&events).unwrap().commit_text,
            "legacy failed"
        );
    }

    #[test]
    fn command_asr_response_rejects_missing_final_text() {
        let error = CommandAsrResponse::default().into_events().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("missing final text")
        ));
    }

    #[test]
    fn command_asr_response_ignores_empty_partial_text() {
        let events = CommandAsrResponse {
            partial_text: Some(String::new()),
            text: Some("final".to_owned()),
            error: None,
        }
        .into_events()
        .unwrap();

        assert_eq!(
            events,
            vec![
                RecognitionEvent::FinalText {
                    text: "final".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn command_asr_response_rejects_blank_final_text() {
        let error = CommandAsrResponse {
            text: Some("   	".to_owned()),
            ..CommandAsrResponse::default()
        }
        .into_events()
        .unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("missing final text")
        ));
    }

    #[test]
    fn command_asr_response_ignores_blank_partial_and_error_text() {
        let events = CommandAsrResponse {
            partial_text: Some("   ".to_owned()),
            text: Some("final".to_owned()),
            error: Some("   ".to_owned()),
        }
        .into_events()
        .unwrap();

        assert_eq!(
            events,
            vec![
                RecognitionEvent::FinalText {
                    text: "final".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn command_asr_response_error_takes_priority_over_final_text() {
        let events = CommandAsrResponse {
            partial_text: Some("listening".to_owned()),
            text: Some("final".to_owned()),
            error: Some("asr failed".to_owned()),
        }
        .into_events()
        .unwrap();

        assert_eq!(
            events,
            vec![
                RecognitionEvent::PartialText {
                    text: "listening".to_owned()
                },
                RecognitionEvent::Error {
                    message: "asr failed".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
        assert_eq!(
            events_to_payload(&events).unwrap().commit_text,
            "asr failed"
        );
    }

    #[test]
    fn process_command_asr_runner_maps_failure_response() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"error":"asr failed"}'"#
                    .to_owned(),
            ],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("process runner should create a buffering session");
        session.finish().unwrap();
        let events = session.poll_events().unwrap();
        assert_eq!(
            events_to_payload(&events).unwrap().commit_text,
            "asr failed"
        );
    }

    #[test]
    fn backend_factory_uses_legacy_streaming_protocol_for_streaming_command_provider() {
        let script_path = write_temp_script(
            "vinput-factory-legacy-streaming-asr",
            r"
import json
import sys
lines = [json.loads(line) for line in sys.stdin if line.strip()]
assert lines[0]['type'] == 'audio'
assert lines[1]['type'] == 'finish'
print(json.dumps({'type':'partial','text':'factory partial'}))
print(json.dumps({'type':'final','text':'factory final'}))
print(json.dumps({'type':'closed'}))
",
        );
        let provider = AsrProviderConfig {
            id: "cmd.streaming".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: None,
            hotwords_file: None,
            command: Some("python3".to_owned()),
            args: vec![script_path.to_string_lossy().into_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = AsrBackendFactory::build_provider(&provider).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("legacy streaming command backend should create a session");
        session.push_audio(&[1, -2, 258]).unwrap();
        session.finish().unwrap();
        std::fs::remove_file(script_path).unwrap();

        assert_eq!(
            session.poll_events().unwrap(),
            vec![
                RecognitionEvent::PartialText {
                    text: "factory partial".to_owned()
                },
                RecognitionEvent::FinalText {
                    text: "factory final".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn backend_factory_uses_legacy_batch_protocol_for_command_provider() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: None,
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec!["-c".to_owned(), "cat >/dev/null; printf final".to_owned()],
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = AsrBackendFactory::build_provider(&provider).unwrap();
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("legacy command backend should create a session");
        session.push_audio(&[1, -2, 258]).unwrap();
        session.finish().unwrap();

        assert_eq!(
            session.poll_events().unwrap(),
            vec![
                RecognitionEvent::FinalText {
                    text: "final".to_owned()
                },
                RecognitionEvent::Completed,
            ]
        );
    }

    #[test]
    fn command_asr_backend_runner_is_not_implemented_yet() {
        let backend = CommandAsrBackend::new(CommandAsrSpec {
            provider_id: "cmd".to_owned(),
            command: "helper".to_owned(),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            model_id: None,
            hotwords_file: None,
            timeout_ms: None,
        });

        let mut session = backend
            .create_session(RecognitionContext::normal("raw", None))
            .expect("command backend should create a buffering session");
        session.push_audio(&[1, 2, 3]).unwrap();
        let error = session.finish().unwrap_err();
        assert!(matches!(
            error,
            AsrError::Backend(message) if message.contains("runner is not implemented yet")
        ));
    }

    #[test]
    fn command_asr_backend_with_config_describes_provider() {
        let provider = AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("helper".to_owned()),
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let backend = CommandAsrBackend::with_config(&provider, ProcessCommandAsrRunner).unwrap();
        let descriptor = backend.describe();
        assert_eq!(descriptor.provider_id, "cmd");
        assert_eq!(descriptor.model_id, "cmd-model");
    }

    #[test]
    fn backend_factory_reports_unimplemented_provider_kind() {
        let provider = AsrProviderConfig {
            id: "sherpa-onnx".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::default(),
            endpoint: None,
        };

        let Err(error) = AsrBackendFactory::build_provider(&provider) else {
            panic!("unsupported provider should fail");
        };
        assert!(matches!(
            error,
            AsrError::UnsupportedProviderKind { provider_id, kind }
                if provider_id == "sherpa-onnx" && kind == "local"
        ));
    }

    #[test]
    fn backend_factory_state_reports_unavailable_provider() {
        let config = AsrConfig {
            active_provider: "sherpa-onnx".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "sherpa-onnx".to_owned(),
                kind: AsrProviderKind::Local,
                timeout_ms: None,
                model: Some("paraformer".to_owned()),
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                endpoint: None,
            }],
            ..AsrConfig::default()
        };

        let state = AsrBackendFactory::state_for_config(&config);
        assert_eq!(state.target_provider_id, "sherpa-onnx");
        assert_eq!(state.target_model_id, "paraformer");
        assert!(!state.has_effective_backend);
        assert!(state.last_error.contains("not implemented"));
    }

    #[test]
    fn backend_factory_state_preserves_remote_endpoint() {
        let config = AsrConfig {
            active_provider: "remote".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "remote".to_owned(),
                kind: AsrProviderKind::Remote,
                timeout_ms: None,
                model: Some("cloud-model".to_owned()),
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: std::collections::HashMap::default(),
                endpoint: Some("https://asr.example.test".to_owned()),
            }],
            ..AsrConfig::default()
        };

        let state = AsrBackendFactory::state_for_config(&config);
        assert_eq!(state.target_provider_id, "remote");
        assert_eq!(state.target_model_id, "cloud-model");
        assert!(!state.has_effective_backend);
        assert_eq!(state.remote_endpoints, ["https://asr.example.test"]);
    }
}
