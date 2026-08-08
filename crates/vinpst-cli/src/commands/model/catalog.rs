use super::support::{format_size_bytes, optional_str, safe_path_component};
use super::{
    Context, Duration, InstalledModelInfo, LiveModelEntry, LiveModelFamily, LiveModelRegistry,
    LiveRegistryI18n, LoadedLiveI18n, LoadedLiveModelRegistry, ModelInfoRequest,
    ModelListOwnedRequest, ModelListRequest, ModelSupport, Path, PathBuf, RegistryConfig,
    ReqwestRegistryTextSource, VinpstConfig, default_model_root, fetch_text_from_mirrors, fs,
    live_registry_urls, load_config_file, load_live_i18n, load_registry_installed_model_info,
    scan_installed_models,
};

pub(super) fn handle_model_list_command(request: &ModelListOwnedRequest) -> anyhow::Result<()> {
    print_model_list(ModelListRequest {
        available: request.available,
        installed: request.installed,
        model_root: request.model_root.as_deref(),
        registry_path: request.registry.as_deref(),
        i18n_path: request.i18n.as_deref(),
        config_path: request.config.as_ref(),
        locale: &request.locale,
        json_output: request.json_output,
    })
}

fn print_model_list(request: ModelListRequest<'_>) -> anyhow::Result<()> {
    if request.available && request.installed {
        anyhow::bail!("model list cannot combine --available and --installed");
    }
    if request.installed {
        return print_installed_model_list(request.model_root, request.json_output);
    }

    let (loaded, i18n) = load_live_model_catalog(
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
    )?;
    let models = loaded
        .registry
        .items
        .iter()
        .map(|model| live_model_list_json(model, i18n.i18n.as_ref()))
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "ok": true,
        "source": loaded.source_json,
        "i18n": i18n.source_json,
        "model_count": models.len(),
        "models": models,
    });

    if request.json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_list_text(&loaded, &i18n);
    }
    Ok(())
}

fn print_installed_model_list(model_root: Option<&Path>, json_output: bool) -> anyhow::Result<()> {
    let model_root = match model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let models = load_installed_model_list(&model_root)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&installed_model_list_json(&model_root, &models))?
        );
    } else {
        print_installed_model_list_text(&model_root, &models);
    }
    Ok(())
}

fn load_installed_model_list(model_root: &Path) -> anyhow::Result<Vec<InstalledModelInfo>> {
    scan_installed_models(model_root)
        .with_context(|| format!("scan installed model root `{}`", model_root.display()))
}

pub(super) fn print_model_info(request: ModelInfoRequest<'_>) -> anyhow::Result<()> {
    if request.installed || is_model_path_selector(request.id_or_short_id) {
        let model_dir = resolve_installed_model_info_selector(
            request.id_or_short_id,
            request.installed,
            request.model_root,
        )?;
        let info = load_installed_model_info(&model_dir)?;
        if request.json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&installed_model_info_json(&info)?)?
            );
        } else {
            print_installed_model_info_text(&info);
        }
        return Ok(());
    }

    let (loaded, i18n) = load_live_model_catalog(
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
    )?;
    let model = loaded
        .registry
        .model_by_id_or_short_id(request.id_or_short_id)
        .with_context(|| format!("unknown model id or short_id `{}`", request.id_or_short_id))?;
    let output = live_model_info_json(model, i18n.i18n.as_ref(), &loaded, &i18n)?;

    if request.json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_model_info_text(model, i18n.i18n.as_ref(), &loaded, &i18n);
    }
    Ok(())
}

fn resolve_installed_model_info_selector(
    selector: &str,
    installed: bool,
    model_root: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if is_model_path_selector(selector) {
        if installed {
            anyhow::bail!(
                "model info --installed expects a managed model directory name, not a path"
            );
        }
        return Ok(PathBuf::from(selector));
    }
    let model_root = match model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    Ok(model_root.join(safe_path_component(selector)))
}

fn is_model_path_selector(selector: &str) -> bool {
    let path = Path::new(selector);
    path.is_absolute() || selector.contains('/')
}

fn load_installed_model_info(model_dir: &Path) -> anyhow::Result<InstalledModelInfo> {
    load_registry_installed_model_info(model_dir)
        .with_context(|| format!("read installed model `{}`", model_dir.display()))
}

pub(super) fn load_live_model_catalog(
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
) -> anyhow::Result<(LoadedLiveModelRegistry, LoadedLiveI18n)> {
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinpstConfig::bundled_default().context("parse bundled config")?,
    };
    let loaded = load_live_model_registry(registry_path, &config.registry)?;
    let i18n = load_live_i18n(i18n_path, loaded.remote_base_url.as_deref(), locale)?;
    Ok((loaded, i18n))
}

fn load_live_model_registry(
    registry_path: Option<&Path>,
    registry_config: &RegistryConfig,
) -> anyhow::Result<LoadedLiveModelRegistry> {
    let registry_urls = live_registry_urls(registry_config, "registry/models.json");
    if let Some(path) = registry_path {
        let input = fs::read_to_string(path)
            .with_context(|| format!("read live model registry `{}`", path.display()))?;
        let registry = LiveModelRegistry::from_json_str(&input)
            .with_context(|| format!("validate live model registry `{}`", path.display()))?;
        return Ok(LoadedLiveModelRegistry {
            registry,
            source_json: serde_json::json!({
                "kind": "file",
                "path": path,
                "mirror_count": registry_config.base_urls.len(),
                "registry_urls": registry_urls,
            }),
            source_label: format!("file:{}", path.display()),
            remote_base_url: None,
        });
    }

    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    let fetched = fetch_text_from_mirrors(&source, &registry_urls)
        .context("fetch live model registry from configured mirrors")?;
    let registry = LiveModelRegistry::from_json_str(&fetched.text).with_context(|| {
        format!(
            "validate live model registry fetched from `{}`",
            fetched.url
        )
    })?;
    let remote_base_url = fetched
        .url
        .strip_suffix("/registry/models.json")
        .map(str::to_owned);
    let source_label = format!("url:{}", fetched.url);
    Ok(LoadedLiveModelRegistry {
        registry,
        source_json: serde_json::json!({
            "kind": "http",
            "url": fetched.url,
            "mirror_count": registry_config.base_urls.len(),
            "registry_urls": registry_urls,
        }),
        source_label,
        remote_base_url,
    })
}

pub(super) fn live_model_list_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
) -> serde_json::Value {
    let support = live_model_support(model);
    serde_json::json!({
        "id": model.id,
        "short_id": model.short_id,
        "title": model.resolved_title(i18n),
        "description": model.resolved_description(i18n),
        "language": model.language,
        "size_bytes": model.size_bytes,
        "backend": model.backend(),
        "family": model.model_family(),
        "runtime": model_runtime(model),
        "supports_hotwords": model.supports_hotwords(),
        "supported": support.supported,
        "support": support.reason,
        "url_count": model.urls.len(),
        "urls": model.urls,
        "sha256": model.sha256,
    })
}

fn installed_model_list_json(
    model_root: &Path,
    models: &[InstalledModelInfo],
) -> serde_json::Value {
    let models = models
        .iter()
        .map(installed_model_list_item_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "source": {
            "kind": "installed",
            "model_root": model_root,
        },
        "model_count": models.len(),
        "models": models,
    })
}

fn installed_model_list_item_json(info: &InstalledModelInfo) -> serde_json::Value {
    serde_json::json!({
        "id": info.model_id,
        "name": installed_model_dir_name(&info.model_dir),
        "model_dir": info.model_dir,
        "metadata_path": info.metadata_path,
        "backend": info.metadata.backend,
        "family": info.metadata.model_family(),
        "language": info.metadata.language,
        "runtime": info.metadata.runtime,
        "size_bytes": info.metadata.size_bytes,
        "supports_hotwords": info.metadata.supports_hotwords,
        "file_count": info.file_count,
        "files": info.files,
    })
}

fn installed_model_info_json(info: &InstalledModelInfo) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "ok": true,
        "source": {
            "kind": "installed",
            "path": info.model_dir,
            "metadata_path": info.metadata_path,
        },
        "model": {
            "id": info.model_id,
            "model_dir": info.model_dir,
            "metadata_path": info.metadata_path,
            "backend": info.metadata.backend,
            "family": info.metadata.model_family(),
            "language": info.metadata.language,
            "runtime": info.metadata.runtime,
            "size_bytes": info.metadata.size_bytes,
            "supports_hotwords": info.metadata.supports_hotwords,
            "file_count": info.file_count,
            "files": info.files,
            "vinpst_model": info.metadata.to_raw_value().context("serialize installed model metadata")?,
        },
    }))
}

fn live_model_info_json(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
) -> anyhow::Result<serde_json::Value> {
    let mut model_json = live_model_list_json(model, i18n);
    model_json["vinpst_model"] =
        model
            .vinpst_model
            .as_ref()
            .map_or(Ok(serde_json::Value::Null), |metadata| {
                metadata
                    .to_raw_value()
                    .context("serialize vinpst_model metadata")
            })?;
    Ok(serde_json::json!({
        "ok": true,
        "source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "model": model_json,
    }))
}

fn print_model_list_text(loaded: &LoadedLiveModelRegistry, i18n: &LoadedLiveI18n) {
    println!("ID\tTITLE\tLANGUAGE\tSIZE\tTYPE\tHOTWORDS\tSTATUS");
    for model in &loaded.registry.items {
        let support = live_model_support(model);
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            model.id,
            model.resolved_title(i18n.i18n.as_ref()),
            optional_str(model.language.as_deref()),
            format_size_bytes(model.size_bytes),
            optional_str(model.model_family()),
            if model.supports_hotwords() {
                "yes"
            } else {
                "no"
            },
            if support.supported {
                "available"
            } else {
                "unsupported"
            },
        );
    }
}

fn print_installed_model_list_text(_model_root: &Path, models: &[InstalledModelInfo]) {
    println!("ID\tLANGUAGE\tSIZE\tTYPE\tHOTWORDS\tSTATUS");
    for model in models {
        println!(
            "{}\t{}\t{}\t{}\t{}\tinstalled",
            model.model_id,
            optional_str(model.metadata.language.as_deref()),
            format_size_bytes(model.metadata.size_bytes),
            optional_str(model.metadata.model_family()),
            if model.metadata.supports_hotwords {
                "yes"
            } else {
                "no"
            },
        );
    }
}

fn installed_model_dir_name(model_dir: &Path) -> String {
    model_dir.file_name().map_or_else(
        || model_dir.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn print_installed_model_info_text(info: &InstalledModelInfo) {
    println!("source: installed");
    println!("id: {}", info.model_id);
    println!("model_dir: {}", info.model_dir.display());
    println!("metadata_path: {}", info.metadata_path.display());
    println!(
        "backend: {}",
        optional_str(info.metadata.backend.as_deref())
    );
    println!("family: {}", optional_str(info.metadata.model_family()));
    println!(
        "language: {}",
        optional_str(info.metadata.language.as_deref())
    );
    println!(
        "runtime: {}",
        optional_str(info.metadata.runtime.as_deref())
    );
    println!("size: {}", format_size_bytes(info.metadata.size_bytes));
    println!("supports_hotwords: {}", info.metadata.supports_hotwords);
    println!("files: {}", info.file_count);
    for file in &info.files {
        println!("  - {file}");
    }
}

fn print_model_info_text(
    model: &LiveModelEntry,
    i18n: Option<&LiveRegistryI18n>,
    loaded: &LoadedLiveModelRegistry,
    loaded_i18n: &LoadedLiveI18n,
) {
    let support = live_model_support(model);
    println!("registry_source: {}", loaded.source_label);
    println!("i18n_source: {}", loaded_i18n.source_label);
    println!("id: {}", model.id);
    println!("short_id: {}", optional_str(model.short_id.as_deref()));
    println!("title: {}", model.resolved_title(i18n));
    println!(
        "description: {}",
        optional_str(model.resolved_description(i18n).as_deref())
    );
    println!("language: {}", optional_str(model.language.as_deref()));
    println!("size: {}", format_size_bytes(model.size_bytes));
    println!("backend: {}", optional_str(model.backend()));
    println!("family: {}", optional_str(model.model_family()));
    println!("runtime: {}", optional_str(model_runtime(model)));
    println!("support: {}", support.reason);
    println!("supported: {}", support.supported);
    println!("supports_hotwords: {}", model.supports_hotwords());
    println!("sha256: {}", optional_str(model.sha256.as_deref()));
    println!("urls:");
    for url in &model.urls {
        println!("  - {url}");
    }
}

fn live_model_support(model: &LiveModelEntry) -> ModelSupport {
    match (
        model.backend(),
        model.classified_model_family(),
        model_runtime(model),
    ) {
        (
            Some("sherpa-offline"),
            Some(
                LiveModelFamily::Dolphin
                | LiveModelFamily::Transducer
                | LiveModelFamily::SenseVoice
                | LiveModelFamily::Paraformer
                | LiveModelFamily::Qwen3Asr
                | LiveModelFamily::Moonshine,
            ),
            Some("offline"),
        )
        | (
            Some("sherpa-streaming"),
            Some(LiveModelFamily::Transducer | LiveModelFamily::Zipformer2Ctc),
            Some("online"),
        ) => ModelSupport {
            supported: true,
            reason: "supported",
        },
        (
            Some("sherpa-offline"),
            Some(
                LiveModelFamily::Dolphin
                | LiveModelFamily::Transducer
                | LiveModelFamily::SenseVoice
                | LiveModelFamily::Paraformer
                | LiveModelFamily::Qwen3Asr
                | LiveModelFamily::Moonshine,
            ),
            _,
        )
        | (
            Some("sherpa-streaming"),
            Some(LiveModelFamily::Transducer | LiveModelFamily::Zipformer2Ctc),
            _,
        ) => ModelSupport {
            supported: false,
            reason: "unsupported-runtime",
        },
        (Some("sherpa-offline" | "sherpa-streaming"), Some(_), _) => ModelSupport {
            supported: false,
            reason: "unsupported-family",
        },
        (Some(_), _, _) => ModelSupport {
            supported: false,
            reason: "unsupported-backend",
        },
        (None, _, _) => ModelSupport {
            supported: false,
            reason: "missing-backend",
        },
    }
}

fn model_runtime(model: &LiveModelEntry) -> Option<&str> {
    model
        .vinpst_model
        .as_ref()
        .and_then(|metadata| metadata.runtime.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
