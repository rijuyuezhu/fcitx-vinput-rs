//! Command-backed ASR protocol, process runners, and backend implementation.

use std::{
    io::Write,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinput_audio::{PcmBuffer, PcmSpec, i16_samples_to_le_bytes};
use vinput_config::{AsrProviderConfig, AsrProviderKind};

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};

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
    /// Creates a command ASR backend with an injected buffered runner.
    #[must_use]
    pub fn with_runner(spec: CommandAsrSpec, runner: R) -> Self {
        Self::with_runner_and_capabilities(spec, runner, BackendCapabilities::buffered())
    }

    /// Creates a command ASR backend with an injected runner and explicit capabilities.
    #[must_use]
    pub fn with_runner_and_capabilities(
        spec: CommandAsrSpec,
        runner: R,
        capabilities: BackendCapabilities,
    ) -> Self {
        let descriptor = BackendDescriptor::new(
            spec.provider_id.clone(),
            spec.model_id.clone().unwrap_or_default(),
            "Command ASR",
            capabilities,
        );
        Self {
            spec,
            descriptor,
            runner,
        }
    }

    /// Creates a command ASR backend from typed provider config with an injected runner.
    pub fn with_config(provider: &AsrProviderConfig, runner: R) -> Result<Self, AsrError> {
        Self::with_config_and_capabilities(provider, runner, BackendCapabilities::buffered())
    }

    /// Creates a command ASR backend from typed provider config with explicit capabilities.
    pub fn with_config_and_capabilities(
        provider: &AsrProviderConfig,
        runner: R,
        capabilities: BackendCapabilities,
    ) -> Result<Self, AsrError> {
        Ok(Self::with_runner_and_capabilities(
            CommandAsrSpec::try_from(provider)?,
            runner,
            capabilities,
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
