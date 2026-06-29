//! OpenAI-compatible text adapter request building and processor seams.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use vinput_config::{COMMAND_SCENE_ID, LlmProviderConfig, RAW_SCENE_ID, SceneDefinition};
use vinput_protocol::{Candidate, CandidateSource, RecognitionPayload};

use crate::prompt::{
    build_constraints_suffix, render_legacy_prompt_placeholders_with_context, wrap_xml_block,
};
use crate::{
    PromptContext, has_legacy_prompt_interpolation, is_prompt_file_uri, load_prompt_file_uri,
};
use crate::{
    TextAdapter, TextError, TextProcessor, TextRequest, command_mode_payload,
    load_recent_input_context_prefix, scene_needs_postprocessing,
};

const OPENAI_COMPATIBLE_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const OPENAI_COMPATIBLE_JSON_CONTENT_TYPE_HEADER: (&str, &str) =
    ("Content-Type", "application/json");
const OPENAI_COMPATIBLE_AUTHORIZATION_HEADER: &str = "Authorization";
const OPENAI_COMPATIBLE_BEARER_PREFIX: &str = "Bearer ";

/// Builds the legacy OpenAI-compatible chat-completions endpoint URL.
///
/// Empty base URLs are not requestable. If the base URL already ends with
/// `/chat/completions`, it is preserved verbatim; otherwise trailing slashes are
/// removed before appending exactly one path separator.
#[must_use]
pub fn build_openai_compatible_chat_url(base_url: &str) -> Option<String> {
    if base_url.is_empty() {
        return None;
    }
    if base_url.ends_with(OPENAI_COMPATIBLE_CHAT_COMPLETIONS_PATH) {
        return Some(base_url.to_owned());
    }
    let mut url = base_url.to_owned();
    while url.ends_with('/') {
        url.pop();
    }
    url.push_str(OPENAI_COMPATIBLE_CHAT_COMPLETIONS_PATH);
    Some(url)
}

/// Builds the legacy OpenAI-compatible request headers.
///
/// The API key string is used as configured. Legacy CLI/GUI paths trim the value
/// while editing config, but the daemon request path only checks whether it is
/// empty before appending the `Authorization: Bearer ...` header.
#[must_use]
pub fn build_openai_compatible_headers(api_key: &str) -> Vec<(String, String)> {
    let mut headers = vec![(
        OPENAI_COMPATIBLE_JSON_CONTENT_TYPE_HEADER.0.to_owned(),
        OPENAI_COMPATIBLE_JSON_CONTENT_TYPE_HEADER.1.to_owned(),
    )];
    if !api_key.is_empty() {
        headers.push((
            OPENAI_COMPATIBLE_AUTHORIZATION_HEADER.to_owned(),
            format!("{OPENAI_COMPATIBLE_BEARER_PREFIX}{api_key}"),
        ));
    }
    headers
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

/// Converts OpenAI-compatible candidate strings into the daemon payload shape.
///
/// The first LLM candidate becomes the default committed text, matching the
/// legacy recognition payload normalization rule. Empty candidate lists return
/// `None` so callers can fall back to raw ASR/command candidates.
#[must_use]
pub fn openai_compatible_candidates_to_payload(
    candidates: impl IntoIterator<Item = String>,
) -> Option<RecognitionPayload> {
    let candidates = candidates
        .into_iter()
        .map(|candidate| Candidate::new(candidate, CandidateSource::Llm))
        .collect::<Vec<_>>();
    let commit_text = candidates.first()?.text.clone();
    Some(RecognitionPayload {
        commit_text,
        candidates,
    })
}

/// Parses an OpenAI-compatible chat response into a daemon recognition payload.
///
/// Invalid response shapes or empty candidate lists return `None` so callers can
/// fall back to raw ASR or command-mode fallback candidates.
#[must_use]
pub fn openai_compatible_response_to_payload(response_body: &str) -> Option<RecognitionPayload> {
    openai_compatible_candidates_to_payload(extract_openai_compatible_candidates(response_body))
}

const OPENAI_COMPATIBLE_PROTECTED_EXTRA_BODY_KEYS: &[&str] =
    &["messages", "stream", "response_format"];

/// Merges provider-specific OpenAI-compatible request fields into a request body.
///
/// The legacy daemon lets user config pass through provider-specific top-level
/// fields, but refuses `messages`, `stream`, and `response_format` because they
/// are required for the non-streaming JSON-candidates response contract. The
/// returned list contains ignored protected keys in input iteration order so
/// callers can log diagnostics without exposing secret values.
pub fn merge_openai_compatible_extra_body(
    request: &mut serde_json::Value,
    extra_body: &serde_json::Value,
) -> Vec<String> {
    let Some(request) = request.as_object_mut() else {
        return Vec::new();
    };
    let Some(extra_body) = extra_body.as_object() else {
        return Vec::new();
    };

    let mut ignored = Vec::new();
    for (key, value) in extra_body {
        if OPENAI_COMPATIBLE_PROTECTED_EXTRA_BODY_KEYS.contains(&key.as_str()) {
            ignored.push(key.clone());
            continue;
        }
        request.insert(key.clone(), value.clone());
    }
    ignored
}

/// OpenAI-compatible chat-completions request body built from a scene prompt.
#[derive(Clone, PartialEq)]
pub struct OpenAiCompatibleChatRequest {
    /// Fully resolved chat-completions endpoint URL.
    pub url: String,
    /// Request headers for the chat-completions request.
    pub headers: Vec<(String, String)>,
    /// JSON body for a non-streaming chat-completions request.
    pub body: serde_json::Value,
    /// Protected `extra_body` keys that were ignored while building the body.
    pub ignored_extra_body_keys: Vec<String>,
}

impl OpenAiCompatibleChatRequest {
    /// Returns request headers with secrets redacted for logs or diagnostics.
    #[must_use]
    pub fn redacted_headers(&self) -> Vec<(String, String)> {
        redact_openai_compatible_headers(&self.headers)
    }
}

impl fmt::Debug for OpenAiCompatibleChatRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleChatRequest")
            .field("url", &self.url)
            .field("headers", &self.redacted_headers())
            .field("body", &self.body)
            .field("ignored_extra_body_keys", &self.ignored_extra_body_keys)
            .finish()
    }
}

fn redact_openai_compatible_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let value = if name.eq_ignore_ascii_case(OPENAI_COMPATIBLE_AUTHORIZATION_HEADER) {
                "<redacted>".to_owned()
            } else {
                value.clone()
            };
            (name.clone(), value)
        })
        .collect()
}

/// Builds the legacy OpenAI-compatible non-streaming request body.
///
/// This helper only pins request assembly: prompt-file resolution,
/// legacy `{{asr}}`/`{{selected}}`/`{{context}}` interpolation, XML fallback,
/// candidate constraints, JSON-object response format, and protected
/// `extra_body` handling.
pub fn build_openai_compatible_chat_request(
    request: &TextRequest<'_>,
    provider: &LlmProviderConfig,
    context_prefix: &str,
) -> Result<Option<OpenAiCompatibleChatRequest>, TextError> {
    let Some(url) = build_openai_compatible_chat_url(&provider.base_url) else {
        return Ok(None);
    };
    let headers = build_openai_compatible_headers(&provider.api_key);
    let Some(prompt) = request
        .scene
        .prompt
        .as_deref()
        .filter(|prompt| !prompt.is_empty())
    else {
        return Ok(None);
    };
    let base_prompt = if is_prompt_file_uri(prompt) {
        load_prompt_file_uri(prompt)?
    } else {
        prompt.to_owned()
    };
    let prompt_context = PromptContext::from_request(request);
    let mut user_content = if has_legacy_prompt_interpolation(&base_prompt) {
        render_legacy_prompt_placeholders_with_context(
            &base_prompt,
            &prompt_context,
            context_prefix,
        )
    } else {
        let mut content = base_prompt;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push_str("\n\n");
        } else if !content.is_empty() {
            content.push('\n');
        }
        if !context_prefix.is_empty() {
            content.push_str(&wrap_xml_block("context", context_prefix));
            content.push('\n');
        }
        if !request.raw_text.is_empty() {
            content.push_str(&wrap_xml_block("asr", request.raw_text));
            content.push('\n');
        }
        if request.scene.id == COMMAND_SCENE_ID
            && let Some(selected_text) = request.selected_text.filter(|text| !text.is_empty())
        {
            content.push_str(&wrap_xml_block("selected", selected_text));
            content.push('\n');
        }
        content
    };
    user_content.push_str(&build_constraints_suffix(request.scene.candidate_count));

    let model = request
        .scene
        .model
        .as_deref()
        .or(provider.model.as_deref())
        .unwrap_or_default();
    let mut body = serde_json::json!({
        "model": model,
        "stream": false,
        "temperature": 0.2,
        "response_format": {"type": "json_object"},
        "messages": [
            {
                "role": "user",
                "content": user_content,
            }
        ],
    });
    let ignored_extra_body_keys =
        merge_openai_compatible_extra_body(&mut body, &provider.extra_body);

    Ok(Some(OpenAiCompatibleChatRequest {
        url,
        headers,
        body,
        ignored_extra_body_keys,
    }))
}

/// Builds an OpenAI-compatible request using a recent-input context cache file.
///
/// The cache is read according to `request.scene.context_lines`; missing cache
/// files produce an empty context prefix. This keeps filesystem policy out of
/// HTTP transport while matching the legacy prompt assembly path.
pub fn build_openai_compatible_chat_request_from_context_cache(
    request: &TextRequest<'_>,
    provider: &LlmProviderConfig,
    context_cache_path: impl AsRef<Path>,
) -> Result<Option<OpenAiCompatibleChatRequest>, TextError> {
    let context_prefix =
        load_recent_input_context_prefix(context_cache_path, request.scene.context_lines)?;
    build_openai_compatible_chat_request(request, provider, &context_prefix)
}

/// Transport seam for OpenAI-compatible chat-completions providers.
pub trait OpenAiCompatibleChatTransport: Send + Sync {
    /// Sends a fully built request and returns the raw response body.
    fn send(
        &self,
        request: &OpenAiCompatibleChatRequest,
        timeout_ms: Option<u64>,
    ) -> Result<String, TextError>;
}

/// Blocking HTTP transport for OpenAI-compatible chat-completions providers.
///
/// The transport sends the already-built request body and headers as-is, applies
/// the optional per-scene timeout to the request, and returns the raw response
/// body for the existing candidate parser. HTTP errors are mapped to
/// `TextError::AdapterFailed` with the status code and response body included.
#[derive(Debug, Clone)]
pub struct ReqwestOpenAiCompatibleChatTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestOpenAiCompatibleChatTransport {
    /// Creates a transport with reqwest's default blocking client settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }

    /// Creates a transport with a caller-provided reqwest blocking client.
    #[must_use]
    pub const fn with_client(client: reqwest::blocking::Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestOpenAiCompatibleChatTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiCompatibleChatTransport for ReqwestOpenAiCompatibleChatTransport {
    fn send(
        &self,
        request: &OpenAiCompatibleChatRequest,
        timeout_ms: Option<u64>,
    ) -> Result<String, TextError> {
        let mut builder = self.client.post(&request.url).json(&request.body);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(timeout_ms) = timeout_ms {
            builder = builder.timeout(std::time::Duration::from_millis(timeout_ms));
        }

        let response = builder.send().map_err(|error| {
            TextError::AdapterFailed(format!("OpenAI-compatible HTTP request failed: {error}"))
        })?;
        let status = response.status();
        let body = response.text().map_err(|error| {
            TextError::AdapterFailed(format!(
                "OpenAI-compatible HTTP response body read failed: {error}"
            ))
        })?;
        if !status.is_success() {
            return Err(TextError::AdapterFailed(format!(
                "OpenAI-compatible provider returned HTTP {status}: {body}"
            )));
        }
        Ok(body)
    }
}

/// Text adapter backed by an OpenAI-compatible chat transport.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleTextAdapter<T> {
    provider: LlmProviderConfig,
    transport: T,
    context_cache_path: Option<PathBuf>,
}

impl<T> OpenAiCompatibleTextAdapter<T> {
    /// Creates an adapter without recent-input context cache wiring.
    #[must_use]
    pub fn new(provider: LlmProviderConfig, transport: T) -> Self {
        Self {
            provider,
            transport,
            context_cache_path: None,
        }
    }

    /// Adds a recent-input context cache path used by scenes with context lines.
    #[must_use]
    pub fn with_context_cache_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.context_cache_path = Some(path.into());
        self
    }
}

impl<T: OpenAiCompatibleChatTransport> TextAdapter for OpenAiCompatibleTextAdapter<T> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        let built = if let Some(context_cache_path) = &self.context_cache_path {
            build_openai_compatible_chat_request_from_context_cache(
                request,
                &self.provider,
                context_cache_path,
            )?
        } else {
            build_openai_compatible_chat_request(request, &self.provider, "")?
        }
        .ok_or_else(|| TextError::UnsupportedAdapter(request.scene.id.clone()))?;

        let response_body = self.transport.send(&built, request.scene.timeout_ms)?;
        let candidates = extract_openai_compatible_candidates(&response_body);
        if request.scene.id == COMMAND_SCENE_ID {
            return Ok(command_mode_payload(
                request.selected_text.unwrap_or_default(),
                request.raw_text,
                candidates,
            ));
        }
        openai_compatible_candidates_to_payload(candidates).ok_or_else(|| {
            TextError::AdapterFailed(format!(
                "OpenAI-compatible provider `{}` response did not contain candidates",
                self.provider.id
            ))
        })
    }
}

fn select_openai_compatible_provider<'a>(
    providers: &'a [LlmProviderConfig],
    scene: &SceneDefinition,
) -> Result<Option<&'a LlmProviderConfig>, TextError> {
    if let Some(provider_id) = scene.provider_id.as_deref() {
        return providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map(Some)
            .ok_or_else(|| TextError::UnknownProvider {
                scene_id: scene.id.clone(),
                provider_id: provider_id.to_owned(),
            });
    }

    match providers {
        [] => Ok(None),
        [provider] => Ok(Some(provider)),
        _ => Err(TextError::AmbiguousProvider(scene.id.clone())),
    }
}

/// Text processor that selects an OpenAI-compatible provider per scene.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleTextProcessor<T> {
    providers: Vec<LlmProviderConfig>,
    transport: T,
    context_cache_path: Option<PathBuf>,
}

impl<T> OpenAiCompatibleTextProcessor<T> {
    /// Creates a processor from OpenAI-compatible provider config entries.
    #[must_use]
    pub fn new(providers: Vec<LlmProviderConfig>, transport: T) -> Self {
        Self {
            providers,
            transport,
            context_cache_path: None,
        }
    }

    /// Adds a recent-input context cache path used by scenes with context lines.
    #[must_use]
    pub fn with_context_cache_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.context_cache_path = Some(path.into());
        self
    }

    /// Returns configured OpenAI-compatible providers.
    #[must_use]
    pub fn providers(&self) -> &[LlmProviderConfig] {
        &self.providers
    }
}

impl<T> TextProcessor for OpenAiCompatibleTextProcessor<T>
where
    T: OpenAiCompatibleChatTransport + Clone,
{
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        let provider = select_openai_compatible_provider(&self.providers, request.scene)?
            .ok_or_else(|| TextError::AdapterRequired(request.scene.id.clone()))?;
        let mut adapter =
            OpenAiCompatibleTextAdapter::new(provider.clone(), self.transport.clone());
        if let Some(context_cache_path) = &self.context_cache_path {
            adapter = adapter.with_context_cache_path(context_cache_path.clone());
        }
        adapter.finish(request)
    }
}
