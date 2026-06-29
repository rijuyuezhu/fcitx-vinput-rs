//! Recent-input context cache paths, buffering, and JSONL maintenance.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, ErrorKind, Write},
    path::{Path, PathBuf},
};

use crate::TextError;

/// Returns the legacy default recent-input context cache path.
///
/// Legacy resolves this under `XDG_CACHE_HOME`, then `$HOME/.cache`, and falls
/// back to a relative `vinput/context.jsonl` path when neither base is set.
#[must_use]
pub fn default_context_cache_path(xdg_cache_home: Option<&Path>, home: Option<&Path>) -> PathBuf {
    let base = xdg_cache_home
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| {
            home.filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.join(".cache"))
        })
        .unwrap_or_default();
    base.join("vinput").join("context.jsonl")
}

/// Returns the default recent-input context cache path for the current process.
#[must_use]
pub fn default_context_cache_path_for_current_user() -> PathBuf {
    let xdg_cache_home = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    default_context_cache_path(xdg_cache_home.as_deref(), home.as_deref())
}

fn first_legacy_utf8_codepoint(text: &str) -> u32 {
    text.chars().next().map_or(0, u32::from)
}

fn last_legacy_utf8_codepoint(text: &str) -> u32 {
    text.chars().next_back().map_or(0, u32::from)
}

fn is_legacy_cjk_codepoint(codepoint: u32) -> bool {
    codepoint >= 0x2E80
}

fn is_legacy_sentence_ending_punctuation(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x3002 | 0xFF01 | 0xFF1F | 0x2026 | 0x2E | 0x21 | 0x3F | 0x0A
    )
}

/// Appends committed text to a recent-input context buffer.
///
/// The helper mirrors the legacy frontend buffering rule: non-CJK fragments are
/// separated by one space, CJK boundaries are kept tight, and sentence-ending
/// punctuation asks the caller to flush the buffer immediately.
pub fn append_recent_input_context_buffer(buffer: &mut String, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if !buffer.is_empty() {
        let cjk_boundary = is_legacy_cjk_codepoint(last_legacy_utf8_codepoint(buffer))
            || is_legacy_cjk_codepoint(first_legacy_utf8_codepoint(text));
        if !cjk_boundary && !buffer.ends_with(' ') {
            buffer.push(' ');
        }
    }
    buffer.push_str(text);
    is_legacy_sentence_ending_punctuation(last_legacy_utf8_codepoint(buffer))
}

/// Builds the legacy recent-input context prompt prefix from cache lines.
///
/// Empty lines are ignored, the last `max_lines` non-empty lines are kept, and
/// the returned text matches the legacy daemon's `{{context}}`/XML context
/// block content.
#[must_use]
pub fn build_recent_input_context_prefix<I, S>(lines: I, max_lines: u8) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if max_lines == 0 {
        return String::new();
    }

    let lines = lines
        .into_iter()
        .map(|line| line.as_ref().to_owned())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }

    let start = lines.len().saturating_sub(usize::from(max_lines));
    let mut result = String::from("Recent input history (use to fix ASR errors):\n");
    for line in &lines[start..] {
        result.push_str(line);
        result.push('\n');
    }
    result.push('\n');
    result
}

/// Loads the legacy recent-input context prompt prefix from a JSONL cache file.
///
/// Missing cache files are treated as empty context, matching the legacy daemon.
/// Other I/O failures are surfaced so callers can report diagnostics.
pub fn load_recent_input_context_prefix(
    path: impl AsRef<Path>,
    max_lines: u8,
) -> Result<String, TextError> {
    if max_lines == 0 {
        return Ok(String::new());
    }

    let path = path.as_ref();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => {
            return Err(TextError::ContextCacheRead(format!(
                "failed to open context cache `{}`: {error}",
                path.display()
            )));
        }
    };
    let lines = std::io::BufReader::new(file)
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            TextError::ContextCacheRead(format!(
                "failed to read context cache `{}`: {error}",
                path.display()
            ))
        })?;

    Ok(build_recent_input_context_prefix(lines, max_lines))
}

/// Legacy recent-input context cache entry written by the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentInputContextEntry {
    /// Committed text fragment.
    pub text: String,
    /// Source of the committed text.
    pub source: String,
    /// Unix timestamp in seconds.
    pub timestamp: u64,
}

/// Appends a legacy recent-input context cache entry.
///
/// Empty text is ignored. The entry is written as a single JSON line so the
/// daemon-side reader can preserve legacy behavior by sending raw non-empty
/// cache lines as context.
pub fn append_recent_input_context_entry(
    path: impl AsRef<Path>,
    text: &str,
    source: &str,
    timestamp: u64,
) -> Result<bool, TextError> {
    if text.is_empty() {
        return Ok(false);
    }

    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            TextError::ContextCacheWrite(format!(
                "failed to create context cache directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    let entry = RecentInputContextEntry {
        text: text.to_owned(),
        source: if source.is_empty() {
            "user".to_owned()
        } else {
            source.to_owned()
        },
        timestamp,
    };
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            TextError::ContextCacheWrite(format!(
                "failed to open context cache `{}` for append: {error}",
                path.display()
            ))
        })?;
    serde_json::to_writer(&mut file, &entry).map_err(|error| {
        TextError::ContextCacheWrite(format!(
            "failed to encode context cache entry for `{}`: {error}",
            path.display()
        ))
    })?;
    file.write_all(
        b"
",
    )
    .map_err(|error| {
        TextError::ContextCacheWrite(format!(
            "failed to write context cache `{}`: {error}",
            path.display()
        ))
    })?;
    Ok(true)
}

/// Truncates a legacy recent-input context cache to the last non-empty lines.
///
/// Missing cache files are ignored. This mirrors the frontend maintenance path,
/// while leaving scheduling policy to the caller.
pub fn truncate_recent_input_context_cache(
    path: impl AsRef<Path>,
    keep_lines: u8,
) -> Result<(), TextError> {
    if keep_lines == 0 {
        return Ok(());
    }

    let path = path.as_ref();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(TextError::ContextCacheRead(format!(
                "failed to open context cache `{}`: {error}",
                path.display()
            )));
        }
    };
    let lines = std::io::BufReader::new(file)
        .lines()
        .filter_map(|line| match line {
            Ok(line) if line.is_empty() => None,
            other => Some(other),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            TextError::ContextCacheRead(format!(
                "failed to read context cache `{}`: {error}",
                path.display()
            ))
        })?;
    let keep_lines = usize::from(keep_lines);
    if lines.len() <= keep_lines {
        return Ok(());
    }

    let mut tmp_path = path.as_os_str().to_owned();
    tmp_path.push(".tmp");
    let tmp_path = PathBuf::from(tmp_path);
    {
        let mut file = fs::File::create(&tmp_path).map_err(|error| {
            TextError::ContextCacheWrite(format!(
                "failed to create context cache temp `{}`: {error}",
                tmp_path.display()
            ))
        })?;
        for line in &lines[lines.len() - keep_lines..] {
            file.write_all(line.as_bytes()).map_err(|error| {
                TextError::ContextCacheWrite(format!(
                    "failed to write context cache temp `{}`: {error}",
                    tmp_path.display()
                ))
            })?;
            file.write_all(
                b"
",
            )
            .map_err(|error| {
                TextError::ContextCacheWrite(format!(
                    "failed to write context cache temp `{}`: {error}",
                    tmp_path.display()
                ))
            })?;
        }
    }
    fs::rename(&tmp_path, path).map_err(|error| {
        TextError::ContextCacheWrite(format!(
            "failed to replace context cache `{}` with `{}`: {error}",
            path.display(),
            tmp_path.display()
        ))
    })?;
    Ok(())
}
