//! Managed ASR model storage and registry operations for the GUI.

use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use vinpst_config::{AsrProviderKind, VinpstConfig};
use vinpst_registry::{
    InstalledModelInfo, LiveModelFamily, LiveModelInstallError, LiveModelInstallRequest,
    LiveModelRegistry, LiveRegistryI18n, ManagedModelRemoveRequest, RegistryOperationControl,
    RegistryOperationProgress, RegistryTextSource, ReqwestRegistryAssetSource,
    ReqwestRegistryTextSource, install_live_model_controlled, managed_model_dir_name,
    remove_managed_model, scan_installed_models,
};

use crate::{GuiLocale, model_install::ModelInstallOutcome};

/// One display-safe model entry loaded from the configured live registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryModelSummary {
    pub(crate) id: String,
    pub(crate) short_id: Option<String>,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) model_type: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) size_bytes: Option<u64>,
    pub(crate) runtime: Option<String>,
    pub(crate) supports_hotwords: bool,
    pub(crate) supported: bool,
}

impl RegistryModelSummary {
    pub(crate) fn selector(&self) -> &str {
        self.short_id.as_deref().unwrap_or(&self.id)
    }
}

/// Asynchronous live-registry catalog state shown by the Resources page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ModelCatalogState {
    #[default]
    Loading,
    Ready(Vec<RegistryModelSummary>),
    Failed(String),
}

/// Returns the managed ASR model root used by CLI and GUI workflows.
pub fn default_model_root() -> Result<PathBuf, String> {
    Ok(user_data_home()?.join("fcitx-vinpst").join("models"))
}

fn default_model_staging_root() -> Result<PathBuf, String> {
    Ok(user_cache_home()?
        .join("fcitx-vinpst")
        .join("model-install"))
}

fn user_data_home() -> Result<PathBuf, String> {
    match env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".local/share")),
    }
}

fn user_cache_home() -> Result<PathBuf, String> {
    match env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(user_home()?.join(".cache")),
    }
}

fn user_home() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is required to locate managed model storage".to_owned())
}

pub(crate) fn load_installed_models() -> Result<Vec<InstalledModelInfo>, String> {
    let root = default_model_root()?;
    scan_installed_models(&root).map_err(|error| error.to_string())
}

pub(crate) fn load_registry_model_catalog(
    config: &VinpstConfig,
    locale: GuiLocale,
) -> Result<Vec<RegistryModelSummary>, String> {
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    fetch_registry_model_catalog_from(config, locale, &source)
}

fn fetch_registry_model_catalog_from(
    config: &VinpstConfig,
    locale: GuiLocale,
    source: &impl RegistryTextSource,
) -> Result<Vec<RegistryModelSummary>, String> {
    if config.registry.base_urls.is_empty() {
        return Err("No registry mirrors are configured.".to_owned());
    }
    let mut failure_count = 0;
    for base in &config.registry.base_urls {
        let url = format!("{}/registry/models.json", base.trim_end_matches('/'));
        let Ok(text) = source.fetch_registry_text(&url) else {
            failure_count += 1;
            continue;
        };
        let Ok(registry) = LiveModelRegistry::from_json_str(&text) else {
            failure_count += 1;
            continue;
        };
        let i18n = fetch_registry_i18n(source, base, locale);
        return Ok(registry
            .items
            .iter()
            .map(|model| RegistryModelSummary {
                id: model.id.clone(),
                short_id: model.short_id.clone(),
                title: model.resolved_title(i18n.as_ref()),
                description: model.resolved_description(i18n.as_ref()),
                model_type: model
                    .vinpst_model
                    .as_ref()
                    .and_then(|metadata| metadata.model_family().map(str::to_owned)),
                language: model.language.clone(),
                size_bytes: model.size_bytes,
                runtime: model
                    .vinpst_model
                    .as_ref()
                    .and_then(|metadata| metadata.runtime.clone()),
                supports_hotwords: model.supports_hotwords(),
                supported: model_is_supported(model),
            })
            .collect());
    }
    Err(format!(
        "All {failure_count} configured registry mirrors failed."
    ))
}

pub(crate) fn fetch_registry_i18n(
    source: &impl RegistryTextSource,
    base: &str,
    locale: GuiLocale,
) -> Option<LiveRegistryI18n> {
    let base = base.trim_end_matches('/');
    let preferred = locale.code();
    let fallback = (preferred != "en_US")
        .then(|| fetch_i18n_layer(source, &format!("{base}/i18n/en_US.json")))
        .flatten();
    let preferred = fetch_i18n_layer(source, &format!("{base}/i18n/{preferred}.json"));
    let merged = LiveRegistryI18n::merge_layers([fallback, preferred].into_iter().flatten());
    (!merged.entries.is_empty()).then_some(merged)
}

fn fetch_i18n_layer(source: &impl RegistryTextSource, url: &str) -> Option<LiveRegistryI18n> {
    source
        .fetch_registry_text(url)
        .ok()
        .and_then(|text| LiveRegistryI18n::from_json_str(&text).ok())
}

fn model_is_supported(model: &vinpst_registry::LiveModelEntry) -> bool {
    let runtime = model
        .vinpst_model
        .as_ref()
        .and_then(|metadata| metadata.runtime.as_deref());
    matches!(
        (model.backend(), model.classified_model_family(), runtime),
        (
            Some("sherpa-offline"),
            Some(
                LiveModelFamily::Dolphin
                    | LiveModelFamily::Transducer
                    | LiveModelFamily::SenseVoice
                    | LiveModelFamily::Paraformer
                    | LiveModelFamily::Qwen3Asr
                    | LiveModelFamily::Moonshine
            ),
            Some("offline")
        ) | (
            Some("sherpa-streaming"),
            Some(LiveModelFamily::Transducer | LiveModelFamily::Zipformer2Ctc),
            Some("online")
        )
    )
}

pub(crate) fn install_registry_model_controlled(
    config: &VinpstConfig,
    selector: &str,
    control: &RegistryOperationControl,
    locale: GuiLocale,
) -> ModelInstallOutcome {
    control.report(RegistryOperationProgress::ResolvingRegistry);
    if control.is_cancelled() {
        return ModelInstallOutcome::Cancelled;
    }
    let registry = match fetch_live_model_registry(config) {
        Ok(registry) => registry,
        Err(_) if control.is_cancelled() => return ModelInstallOutcome::Cancelled,
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    if control.is_cancelled() {
        return ModelInstallOutcome::Cancelled;
    }
    let model = registry
        .model_by_id_or_short_id(selector)
        .ok_or_else(|| format!("Unknown registry model id or short id `{selector}`."));
    let model = match model {
        Ok(model) => model,
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    let model_name = managed_model_dir_name(model);
    let model_dir = match default_model_root() {
        Ok(root) => root.join(&model_name),
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    let staging_dir = match default_model_staging_root() {
        Ok(root) => root.join(&model_name),
        Err(error) => return ModelInstallOutcome::Failed(error),
    };
    let source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(300));
    let installed = install_live_model_controlled(
        &source,
        &LiveModelInstallRequest {
            model,
            model_dir,
            staging_dir: staging_dir.clone(),
            display: Some(model.installed_display_metadata(&config.global.default_language, None)),
        },
        control,
    );
    let installed = match installed {
        Ok(installed) => installed,
        Err(LiveModelInstallError::Cancelled { .. }) => {
            remove_staging_dir(&staging_dir);
            return ModelInstallOutcome::Cancelled;
        }
        Err(error) => {
            remove_staging_dir(&staging_dir);
            return ModelInstallOutcome::Failed(format!("Model installation failed: {error}"));
        }
    };
    ModelInstallOutcome::Installed(locale.model_installed(
        &model.resolved_title(None),
        &model_name,
        installed.checksum_verified(),
    ))
}

fn remove_staging_dir(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

fn fetch_live_model_registry(config: &VinpstConfig) -> Result<LiveModelRegistry, String> {
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    fetch_live_model_registry_from(config, &source)
}

fn fetch_live_model_registry_from(
    config: &VinpstConfig,
    source: &impl RegistryTextSource,
) -> Result<LiveModelRegistry, String> {
    let urls = config
        .registry
        .base_urls
        .iter()
        .map(|base| format!("{}/registry/models.json", base.trim_end_matches('/')))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No registry mirrors are configured.".to_owned());
    }
    let mut failure_count = 0;
    for url in &urls {
        match source.fetch_registry_text(url) {
            Ok(text) => {
                return LiveModelRegistry::from_json_str(&text)
                    .map_err(|error| format!("Registry model catalog is invalid: {error}"));
            }
            Err(_) => failure_count += 1,
        }
    }
    Err(format!(
        "All {failure_count} configured registry mirrors failed."
    ))
}

pub(crate) fn remove_installed_model(
    config: &VinpstConfig,
    target_path: &Path,
    locale: GuiLocale,
) -> Result<String, String> {
    let model_root = default_model_root()?;
    let active_model_paths = config
        .asr
        .providers
        .iter()
        .filter(|provider| provider.kind == AsrProviderKind::Local)
        .filter_map(|provider| provider.model.as_deref())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    remove_managed_model(&ManagedModelRemoveRequest {
        model_root: &model_root,
        target_path,
        active_model_paths: &active_model_paths,
    })
    .map_err(|error| error.to_string())?;
    let directory = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed model");
    Ok(locale.model_removed(directory))
}

pub(crate) fn model_is_active(config: &VinpstConfig, model_dir: &Path) -> bool {
    config.asr.providers.iter().any(|provider| {
        provider.kind == AsrProviderKind::Local
            && provider
                .model
                .as_deref()
                .is_some_and(|model| Path::new(model) == model_dir)
    })
}

pub(crate) fn model_is_selected_by_active_provider(
    config: &VinpstConfig,
    model_dir: &Path,
) -> bool {
    config.asr.providers.iter().any(|provider| {
        provider.id == config.asr.active_provider
            && provider.kind == AsrProviderKind::Local
            && provider
                .model
                .as_deref()
                .is_some_and(|model| Path::new(model) == model_dir)
    })
}

pub(crate) fn active_provider_can_use_managed_models(config: &VinpstConfig) -> bool {
    config.asr.providers.iter().any(|provider| {
        provider.id == config.asr.active_provider && provider.kind == AsrProviderKind::Local
    })
}

pub(crate) fn select_model_for_active_provider(
    config: &VinpstConfig,
    model_dir: &Path,
) -> Result<(VinpstConfig, String), String> {
    let provider_id = config.asr.active_provider.clone();
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("ASR provider `{provider_id}` not found in config"))?;
    if config.asr.providers[provider_index].kind != AsrProviderKind::Local {
        return Err(format!(
            "ASR provider `{provider_id}` is not local and cannot use a managed model"
        ));
    }

    let mut updated = config.clone();
    updated.asr.active_provider.clone_from(&provider_id);
    updated.asr.providers[provider_index].model = Some(model_dir.to_string_lossy().into_owned());
    updated
        .validate()
        .map_err(|error| format!("Validate updated configuration: {error}"))?;
    Ok((updated, provider_id))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct StubRegistryTextSource {
        responses: HashMap<String, Result<String, String>>,
    }

    impl RegistryTextSource for StubRegistryTextSource {
        fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err("missing fixture".to_owned()))
        }
    }

    #[test]
    fn registry_model_fetch_uses_mirror_fallback_without_leaking_urls() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let first = "https://user:super-secret@first.invalid".to_owned();
        let second = "https://second.invalid".to_owned();
        config.registry.base_urls = vec![first.clone(), second.clone()];
        let model_json = json!({
            "version": 1,
            "items": [{
                "id": "model.test.fixture",
                "short_id": "fixture",
                "urls": ["https://assets.invalid/fixture.tar.zst"]
            }]
        })
        .to_string();
        let source = StubRegistryTextSource {
            responses: HashMap::from([
                (
                    format!("{first}/registry/models.json"),
                    Err("connection failed".to_owned()),
                ),
                (format!("{second}/registry/models.json"), Ok(model_json)),
            ]),
        };

        let registry = fetch_live_model_registry_from(&config, &source).expect("mirror fallback");
        assert!(registry.model_by_id_or_short_id("fixture").is_some());

        let failed = StubRegistryTextSource::default();
        let error =
            fetch_live_model_registry_from(&config, &failed).expect_err("all mirrors should fail");
        assert_eq!(error, "All 2 configured registry mirrors failed.");
        assert!(!error.contains("super-secret"));
        assert!(!error.contains("first.invalid"));
    }

    #[test]
    fn registry_model_catalog_exposes_localized_browsable_metadata() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let base = "https://registry.invalid".to_owned();
        config.registry.base_urls = vec![base.clone()];
        let model_json = json!({
            "version": 1,
            "items": [{
                "id": "model.test.streaming",
                "short_id": "test-stream",
                "urls": ["https://assets.invalid/model.tar.zst"],
                "size_bytes": 21_264_113,
                "language": "zh",
                "vinput_model": {
                    "backend": "sherpa-streaming",
                    "runtime": "online",
                    "family": "zipformer2_ctc",
                    "supports_hotwords": false
                }
            }]
        })
        .to_string();
        let i18n_json = json!({
            "model.test.streaming.title": "测试流式模型",
            "model.test.streaming.description": "流式/中文"
        })
        .to_string();
        let source = StubRegistryTextSource {
            responses: HashMap::from([
                (format!("{base}/registry/models.json"), Ok(model_json)),
                (format!("{base}/i18n/zh_CN.json"), Ok(i18n_json)),
            ]),
        };

        let catalog =
            fetch_registry_model_catalog_from(&config, GuiLocale::ZhCn, &source).expect("catalog");
        assert_eq!(catalog.len(), 1);
        let model = &catalog[0];
        assert_eq!(model.selector(), "test-stream");
        assert_eq!(model.title, "测试流式模型");
        assert_eq!(model.description.as_deref(), Some("流式/中文"));
        assert_eq!(model.model_type.as_deref(), Some("zipformer2_ctc"));
        assert_eq!(model.language.as_deref(), Some("zh"));
        assert_eq!(model.size_bytes, Some(21_264_113));
        assert_eq!(model.runtime.as_deref(), Some("online"));
        assert!(!model.supports_hotwords);
        assert!(model.supported);
    }

    #[test]
    fn registry_model_catalog_skips_a_malformed_mirror() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let first = "https://first.invalid".to_owned();
        let second = "https://second.invalid".to_owned();
        config.registry.base_urls = vec![first.clone(), second.clone()];
        let valid_registry = json!({
            "version": 1,
            "items": [{
                "id": "model.test.fallback",
                "short_id": "fallback",
                "urls": ["https://assets.invalid/model.tar.zst"],
                "vinpst_model": {
                    "backend": "sherpa-offline",
                    "runtime": "offline",
                    "family": "sense_voice"
                }
            }]
        })
        .to_string();
        let source = StubRegistryTextSource {
            responses: HashMap::from([
                (
                    format!("{first}/registry/models.json"),
                    Ok("not valid registry json".to_owned()),
                ),
                (format!("{second}/registry/models.json"), Ok(valid_registry)),
            ]),
        };

        let catalog = fetch_registry_model_catalog_from(&config, GuiLocale::EnUs, &source)
            .expect("malformed mirror must fall through");
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].selector(), "fallback");
        assert!(catalog[0].supported);
    }

    #[test]
    fn active_model_detection_matches_only_local_provider_paths() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let model_dir = PathBuf::from("/managed/models/active");
        let provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.kind == AsrProviderKind::Local)
            .expect("local provider");
        provider.model = Some(model_dir.to_string_lossy().into_owned());
        assert!(model_is_active(&config, &model_dir));
        assert!(!model_is_active(
            &config,
            Path::new("/managed/models/inactive")
        ));
    }

    #[test]
    fn active_provider_selection_is_distinct_from_inactive_provider_references() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let active_model = PathBuf::from("/managed/models/active");
        let inactive_model = PathBuf::from("/managed/models/inactive-reference");
        let active_provider_id = config.asr.active_provider.clone();
        let active_provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.id == active_provider_id)
            .expect("active provider");
        active_provider.model = Some(active_model.to_string_lossy().into_owned());

        let mut inactive_provider = active_provider.clone();
        inactive_provider.id = "inactive-local".to_owned();
        inactive_provider.model = Some(inactive_model.to_string_lossy().into_owned());
        config.asr.providers.push(inactive_provider);

        assert!(model_is_active(&config, &inactive_model));
        assert!(!model_is_selected_by_active_provider(
            &config,
            &inactive_model
        ));
        assert!(model_is_selected_by_active_provider(&config, &active_model));
    }

    #[test]
    fn model_selection_targets_the_active_local_provider() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let model_dir = PathBuf::from("/managed/models/selected");
        let active_provider = config.asr.active_provider.clone();

        let (updated, provider_id) = select_model_for_active_provider(&config, &model_dir)
            .expect("active local provider should accept a managed model");

        assert_eq!(provider_id, active_provider);
        let provider = updated
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == active_provider)
            .expect("active provider");
        assert_eq!(provider.model.as_deref(), model_dir.to_str());
        assert!(model_is_active(&updated, &model_dir));
    }

    #[test]
    fn model_selection_rejects_a_non_local_active_provider() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let provider_id = config.asr.active_provider.clone();
        let provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
            .expect("active provider");
        provider.kind = AsrProviderKind::Remote;

        let error = select_model_for_active_provider(&config, Path::new("/managed/models/model"))
            .expect_err("remote provider must reject managed model selection");

        assert!(error.contains(&provider_id));
        assert!(error.contains("is not local"));
    }

    #[test]
    fn managed_model_selection_availability_tracks_the_active_provider_kind() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        assert!(active_provider_can_use_managed_models(&config));

        let active_provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.id == config.asr.active_provider)
            .expect("active provider");
        active_provider.kind = AsrProviderKind::Remote;
        assert!(!active_provider_can_use_managed_models(&config));

        config.asr.active_provider = "missing-provider".to_owned();
        assert!(!active_provider_can_use_managed_models(&config));
    }
}
