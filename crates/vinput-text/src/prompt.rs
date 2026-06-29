//! Prompt context, prompt-file loading, and legacy template rendering.

use std::{fs, io::Read, sync::LazyLock};

use crate::{TextError, TextRequest};

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

const PROMPT_FILE_URI_PREFIX: &str = "file:///";
const MAX_PROMPT_FILE_BYTES: usize = 256 * 1024;

static LEGACY_PROMPT_VAR_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\{\{\s*(\w+)\s*\}\}").expect("legacy prompt variable regex is valid")
});

/// Returns true when a prompt references a legacy `file:///` prompt file.
#[must_use]
pub fn is_prompt_file_uri(input: &str) -> bool {
    input.starts_with(PROMPT_FILE_URI_PREFIX)
}

/// Returns true when a prompt contains a legacy double-brace interpolation marker.
#[must_use]
pub fn has_legacy_prompt_interpolation(input: &str) -> bool {
    input.contains("{{")
}

/// Loads a legacy `file:///absolute/path` prompt file with the daemon safety cap.
///
/// Legacy C++ accepts only the literal `file:///` prefix, strips `file://`
/// while keeping the absolute path's leading slash, requires a regular file,
/// and truncates reads to 256 KiB. This helper keeps those externally visible
/// semantics instead of using generic URL parsing, because percent-decoding or
/// host handling would change accepted legacy config values.
pub fn load_prompt_file_uri(uri: &str) -> Result<String, TextError> {
    if !is_prompt_file_uri(uri) {
        return Err(TextError::PromptFileLoad("not a file:/// URI".to_owned()));
    }

    let path = &uri[PROMPT_FILE_URI_PREFIX.len() - 1..];
    if path.is_empty() || path == "/" {
        return Err(TextError::PromptFileLoad("empty path".to_owned()));
    }

    let metadata = fs::metadata(path)
        .map_err(|error| TextError::PromptFileLoad(format!("stat failed: {error}")))?;
    if !metadata.is_file() {
        return Err(TextError::PromptFileLoad("not a regular file".to_owned()));
    }

    let mut file = fs::File::open(path)
        .map_err(|error| TextError::PromptFileLoad(format!("open failed: {error}")))?;
    let mut content = Vec::with_capacity(MAX_PROMPT_FILE_BYTES + 1);
    std::io::Read::by_ref(&mut file)
        .take((MAX_PROMPT_FILE_BYTES + 1) as u64)
        .read_to_end(&mut content)
        .map_err(|error| TextError::PromptFileLoad(format!("read failed: {error}")))?;
    if content.len() > MAX_PROMPT_FILE_BYTES {
        content.truncate(MAX_PROMPT_FILE_BYTES);
    }

    Ok(String::from_utf8_lossy(&content).into_owned())
}

fn render_legacy_prompt_placeholders(template: &str, context: &PromptContext<'_>) -> String {
    render_legacy_prompt_placeholders_with_context(template, context, "")
}

pub(crate) fn render_legacy_prompt_placeholders_with_context(
    template: &str,
    context: &PromptContext<'_>,
    rendered_context: &str,
) -> String {
    LEGACY_PROMPT_VAR_RE
        .replace_all(template, |captures: &regex::Captures<'_>| {
            match &captures[1] {
                "asr" => context.raw_text.to_owned(),
                "selected" => context.selected_text.to_owned(),
                "context" => rendered_context.to_owned(),
                _ => captures[0].to_owned(),
            }
        })
        .into_owned()
}

pub(crate) fn wrap_xml_block(tag: &str, text: &str) -> String {
    let mut out = String::with_capacity(tag.len() * 2 + text.len() + 12);
    out.push('<');
    out.push_str(tag);
    out.push_str(">\n");
    out.push_str(text);
    if text.is_empty() || !text.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
    out
}

pub(crate) fn build_constraints_suffix(candidate_count: u8) -> String {
    if candidate_count == 0 {
        return String::new();
    }
    format!(
        "\n\n## Constraints\n\
         - Return only the JSON object described below.\n\
         - Each candidate must contain only the final rewritten text.\n\
         - Do not include explanations, Markdown fences, or extra keys.\n\
         \n\n## Format\n\
         Return EXACTLY {candidate_count} candidate(s) in a JSON object:\n\
         ```json\n\
         {{\"candidates\": [\"<string>\", \"<string>\"]}}\n\
         ```"
    )
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
        render_legacy_prompt_placeholders(&self.template, context)
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
