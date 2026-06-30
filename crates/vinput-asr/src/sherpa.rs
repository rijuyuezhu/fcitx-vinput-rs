//! Local `sherpa-onnx` ASR backend seam.
//!
//! This module owns typed config parsing for the future local `sherpa-onnx`
//! backend. It deliberately does not link or invoke the real runtime yet.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use vinput_config::{AsrProviderConfig, AsrProviderKind};

use crate::AsrError;

/// Legacy local provider id used by bundled config and diagnostics.
pub const SHERPA_ONNX_PROVIDER_ID: &str = "sherpa-onnx";

/// Parsed local `sherpa-onnx` provider settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SherpaOnnxSpec {
    /// Provider id from config.
    pub provider_id: String,
    /// Optional model id/path from config.
    pub model: Option<String>,
    /// Optional hotwords file path from config.
    pub hotwords_file: Option<String>,
    /// Optional backend timeout from config.
    pub timeout_ms: Option<u64>,
}

/// Resolved local filesystem inputs for the future sherpa runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxModelPaths {
    /// Resolved model directory.
    pub model_dir: PathBuf,
    /// Resolved hotwords file, when configured.
    pub hotwords_file: Option<PathBuf>,
}

/// Local sherpa model path validation errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SherpaOnnxModelPathError {
    /// Provider does not configure a model.
    #[error("sherpa-onnx provider `{provider_id}` does not configure a model")]
    MissingModel {
        /// Provider id.
        provider_id: String,
    },
    /// Configured model value is empty after trimming.
    #[error("sherpa-onnx provider `{provider_id}` has an empty model path")]
    EmptyModel {
        /// Provider id.
        provider_id: String,
    },
    /// Configured path looks like a URL and is not a local filesystem path.
    #[error("sherpa-onnx provider `{provider_id}` path `{path}` must be local")]
    UrlLikePath {
        /// Provider id.
        provider_id: String,
        /// Rejected path.
        path: String,
    },
    /// Resolved model path does not exist.
    #[error("sherpa-onnx model directory `{path}` does not exist")]
    MissingModelDir {
        /// Resolved model path.
        path: String,
    },
    /// Resolved model path exists but is not a directory.
    #[error("sherpa-onnx model path `{path}` is not a directory")]
    ModelPathNotDirectory {
        /// Resolved model path.
        path: String,
    },
    /// Configured hotwords value is empty after trimming.
    #[error("sherpa-onnx provider `{provider_id}` has an empty hotwords path")]
    EmptyHotwords {
        /// Provider id.
        provider_id: String,
    },
    /// Resolved hotwords path does not exist.
    #[error("sherpa-onnx hotwords file `{path}` does not exist")]
    MissingHotwordsFile {
        /// Resolved hotwords path.
        path: String,
    },
    /// Resolved hotwords path exists but is not a regular file.
    #[error("sherpa-onnx hotwords path `{path}` is not a regular file")]
    HotwordsPathNotFile {
        /// Resolved hotwords path.
        path: String,
    },
}

impl SherpaOnnxSpec {
    /// Parses a config provider into the future local `sherpa-onnx` spec.
    pub fn from_provider(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        if provider.id != SHERPA_ONNX_PROVIDER_ID || provider.kind != AsrProviderKind::Local {
            return Err(AsrError::UnsupportedProviderKind {
                provider_id: provider.id.clone(),
                kind: crate::factory::provider_kind_label(&provider.kind).to_owned(),
            });
        }

        Ok(Self {
            provider_id: provider.id.clone(),
            model: provider.model.clone(),
            hotwords_file: provider.hotwords_file.clone(),
            timeout_ms: provider.timeout_ms,
        })
    }

    /// Resolves configured model and hotwords paths against a local model root.
    ///
    /// Relative model values are resolved under `model_root`; absolute paths are
    /// preserved. Relative hotwords paths are resolved under the resolved model
    /// directory. This validates only filesystem shape required before a future
    /// runtime is constructed; it does not load sherpa-onnx or mutate files.
    pub fn resolve_model_paths(
        &self,
        model_root: impl AsRef<Path>,
    ) -> Result<SherpaOnnxModelPaths, SherpaOnnxModelPathError> {
        let model = self
            .model
            .as_deref()
            .ok_or_else(|| SherpaOnnxModelPathError::MissingModel {
                provider_id: self.provider_id.clone(),
            })?
            .trim();
        if model.is_empty() {
            return Err(SherpaOnnxModelPathError::EmptyModel {
                provider_id: self.provider_id.clone(),
            });
        }
        reject_url_like(&self.provider_id, model)?;

        let model_dir = resolve_against(model_root.as_ref(), model);
        if !model_dir.exists() {
            return Err(SherpaOnnxModelPathError::MissingModelDir {
                path: display_path(&model_dir),
            });
        }
        if !model_dir.is_dir() {
            return Err(SherpaOnnxModelPathError::ModelPathNotDirectory {
                path: display_path(&model_dir),
            });
        }

        let hotwords_file = self
            .hotwords_file
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| {
                reject_url_like(&self.provider_id, path)?;
                let resolved = resolve_against(&model_dir, path);
                if !resolved.exists() {
                    return Err(SherpaOnnxModelPathError::MissingHotwordsFile {
                        path: display_path(&resolved),
                    });
                }
                if !resolved.is_file() {
                    return Err(SherpaOnnxModelPathError::HotwordsPathNotFile {
                        path: display_path(&resolved),
                    });
                }
                Ok(resolved)
            })
            .transpose()?;
        if self
            .hotwords_file
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            return Err(SherpaOnnxModelPathError::EmptyHotwords {
                provider_id: self.provider_id.clone(),
            });
        }

        Ok(SherpaOnnxModelPaths {
            model_dir,
            hotwords_file,
        })
    }

    /// Returns the current explicit runtime-unavailable error.
    #[must_use]
    pub fn runtime_unavailable_error(&self) -> AsrError {
        AsrError::Backend(format!(
            "sherpa-onnx runtime for provider `{}` is not implemented yet",
            self.provider_id
        ))
    }
}

fn resolve_against(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn reject_url_like(provider_id: &str, value: &str) -> Result<(), SherpaOnnxModelPathError> {
    if value.contains("://") {
        Err(SherpaOnnxModelPathError::UrlLikePath {
            provider_id: provider_id.to_owned(),
            path: value.to_owned(),
        })
    } else {
        Ok(())
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}
