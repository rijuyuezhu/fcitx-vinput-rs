use super::{
    AsrProviderConfig, Context, Duration, LiveRegistryI18n, LiveScriptKind, LiveScriptRegistry,
    LoadedLiveI18n, LoadedLiveScriptRegistry, Path, PathBuf, ProviderInstallOutcome,
    ProviderInstallRequest, ProviderListContext, RegistryConfig, ReqwestRegistryAssetSource,
    ReqwestRegistryTextSource, VinpstConfig, asr_provider_kind_label, config_set_write_target,
    default_config_path, default_provider_root, fetch_text_from_mirrors, fs, install_live_script,
    live_registry_urls, load_config_json, load_live_i18n, managed_script_relative_path,
    materialize_asr_provider, normalize_provider_id, validate_config_json_value,
    write_config_set_document,
};

pub(super) fn print_available_provider_list(
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let context = load_provider_list_context(config_path)?;
    let loaded = load_live_provider_registry(registry_path, &context.config.registry)?;
    let loaded_i18n = load_live_i18n(i18n_path, loaded.remote_base_url.as_deref(), locale)?;
    let configured_ids = context
        .config
        .asr
        .providers
        .iter()
        .map(|provider| provider.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let providers = loaded
        .registry
        .items
        .iter()
        .map(|provider| {
            available_live_provider_json(provider, loaded_i18n.i18n.as_ref(), &configured_ids)
        })
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "ok": true,
        "registry_source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "config_path": context.config_path.as_ref(),
        "config_source": context.source,
        "provider_count": providers.len(),
        "providers": providers,
        "next_steps": [
            "run vinpst provider install <id-or-short-id> --dry-run --json to preview installation",
            "run vinpst provider install <id-or-short-id> --in-place to install and configure it",
            "run vinpst provider use <machine-id> to select the installed provider"
        ],
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_available_provider_list_text(&loaded, &loaded_i18n, &context, &configured_ids);
    }
    Ok(())
}

fn available_live_provider_json(
    provider: &vinpst_registry::LiveScriptEntry,
    i18n: Option<&LiveRegistryI18n>,
    configured_ids: &std::collections::BTreeSet<&str>,
) -> serde_json::Value {
    let envs = provider
        .envs
        .iter()
        .map(|env| {
            serde_json::json!({
                "name": env.name,
                "required": env.required,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "id": provider.short_id.as_deref().unwrap_or(&provider.id),
        "machine_id": provider.id,
        "title": provider.resolved_title(i18n),
        "description": provider.resolved_description(i18n),
        "protocol": if provider.stream { "streaming" } else { "batch" },
        "command": provider.command,
        "readme_url": provider.readme_url,
        "envs": envs,
        "status": if configured_ids.contains(provider.id.as_str()) {
            "installed"
        } else {
            "available"
        },
    })
}

fn print_available_provider_list_text(
    loaded: &LoadedLiveScriptRegistry,
    loaded_i18n: &LoadedLiveI18n,
    _context: &ProviderListContext,
    configured_ids: &std::collections::BTreeSet<&str>,
) {
    println!("ID\tTITLE\tMODE\tSTATUS");
    for provider in &loaded.registry.items {
        println!(
            "{}\t{}\t{}\t{}",
            provider.id,
            provider.resolved_title(loaded_i18n.i18n.as_ref()),
            if provider.stream {
                "streaming"
            } else {
                "batch"
            },
            if configured_ids.contains(provider.id.as_str()) {
                "installed"
            } else {
                "available"
            },
        );
    }
}

pub(super) fn print_provider_install(request: ProviderInstallRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_install(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_install_outcome_json(&outcome))?
        );
    } else {
        print_provider_install_text(&outcome);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_provider_install(
    request: &ProviderInstallRequest<'_>,
) -> anyhow::Result<ProviderInstallOutcome> {
    let selector = normalize_provider_id(request.selector)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider install")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for provider install")?;
    config
        .validate()
        .context("validate config for provider install")?;
    let loaded_registry = load_live_provider_registry(request.registry_path, &config.registry)?;
    let entry = loaded_registry
        .registry
        .entry_by_id_or_short_id(&selector, LiveScriptKind::AsrProvider)
        .with_context(|| format!("ASR provider `{selector}` not found in live registry"))?
        .clone();
    let provider_root = request
        .provider_root
        .map(Path::to_path_buf)
        .map_or_else(default_provider_root, Ok)?;
    let script_path = provider_root.join(managed_script_relative_path(
        LiveScriptKind::AsrProvider,
        &entry.id,
    )?);
    let existing = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == entry.id);
    let materialized = materialize_asr_provider(&entry, &script_path, existing)?;
    let provider_value = serde_json::to_value(&materialized.provider)
        .context("serialize installed ASR provider config")?;
    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    if let Some(index) = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == entry.id)
    {
        providers[index] = provider_value;
    } else {
        providers.push(provider_value);
    }
    validate_config_json_value(&loaded.document, "validate installed provider config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_script = false;
    let mut wrote_config = false;
    if !request.dry_run {
        let source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(120));
        let installed =
            install_live_script(&source, LiveScriptKind::AsrProvider, &entry, &provider_root)?;
        if installed.script_path != script_path {
            anyhow::bail!(
                "installed provider script path `{}` did not match planned path `{}`",
                installed.script_path.display(),
                script_path.display()
            );
        }
        wrote_script = true;
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }
    let required_env = entry
        .envs
        .iter()
        .filter(|env| env.required)
        .map(|env| env.name.clone())
        .collect();
    let optional_env = entry
        .envs
        .iter()
        .filter(|env| !env.required)
        .map(|env| env.name.clone())
        .collect();
    Ok(ProviderInstallOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        registry_source: loaded_registry.source_json,
        provider_id: entry.id,
        short_id: entry.short_id,
        streaming: entry.stream,
        script_path,
        required_env,
        optional_env,
        replacing_managed: materialized.replacing_managed,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_script,
        wrote_config,
    })
}

fn provider_install_outcome_json(outcome: &ProviderInstallOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "registry_source": outcome.registry_source,
        "provider_id": outcome.provider_id,
        "short_id": outcome.short_id,
        "protocol": if outcome.streaming { "streaming" } else { "batch" },
        "script_path": outcome.script_path,
        "required_env": outcome.required_env,
        "optional_env": outcome.optional_env,
        "replacing_managed": outcome.replacing_managed,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_download_script": !outcome.dry_run,
        "will_write_config": !outcome.dry_run,
        "wrote_script": outcome.wrote_script,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "set required provider environment values in config before selecting it",
            "run vinpst provider use <machine-id> to select the installed provider",
            "run vinpst asr-state to inspect runtime readiness"
        ],
    })
}

fn print_provider_install_text(outcome: &ProviderInstallOutcome) {
    let mode = if outcome.streaming {
        "streaming"
    } else {
        "batch"
    };
    let preview = format!(
        "Would install ASR provider `{}` ({mode}).",
        outcome.provider_id
    );
    let applied = format!("Installed ASR provider `{}` ({mode}).", outcome.provider_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
    if !outcome.required_env.is_empty() {
        println!(
            "Required configuration: {}",
            outcome.required_env.join(", ")
        );
    }
}

pub(super) fn load_live_provider_registry(
    registry_path: Option<&Path>,
    registry_config: &RegistryConfig,
) -> anyhow::Result<LoadedLiveScriptRegistry> {
    let registry_urls = live_registry_urls(registry_config, "registry/providers.json");
    if let Some(path) = registry_path {
        let input = fs::read_to_string(path)
            .with_context(|| format!("read live provider registry `{}`", path.display()))?;
        let registry = LiveScriptRegistry::from_json_str(&input, LiveScriptKind::AsrProvider)
            .with_context(|| format!("validate live provider registry `{}`", path.display()))?;
        return Ok(LoadedLiveScriptRegistry {
            registry,
            source_json: serde_json::json!({
                "kind": "file",
                "path": path,
                "mirror_count": registry_config.base_urls.len(),
                "registry_urls": registry_urls,
            }),
            remote_base_url: None,
        });
    }
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    let fetched = fetch_text_from_mirrors(&source, &registry_urls)
        .context("fetch live provider registry from configured mirrors")?;
    let registry = LiveScriptRegistry::from_json_str(&fetched.text, LiveScriptKind::AsrProvider)
        .with_context(|| {
            format!(
                "validate live provider registry fetched from `{}`",
                fetched.url
            )
        })?;
    let remote_base_url = fetched
        .url
        .strip_suffix("/registry/providers.json")
        .map(str::to_owned);
    Ok(LoadedLiveScriptRegistry {
        registry,
        source_json: serde_json::json!({
            "kind": "http",
            "url": fetched.url,
            "mirror_count": registry_config.base_urls.len(),
            "registry_urls": registry_urls,
        }),
        remote_base_url,
    })
}

pub(super) fn print_provider_list(
    config_path: Option<&PathBuf>,
    json_output: bool,
) -> anyhow::Result<()> {
    let context = load_provider_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_list_json(&context))?
        );
    } else {
        print_provider_list_text(&context);
    }
    Ok(())
}

pub(super) fn load_provider_list_context(
    config_path: Option<&PathBuf>,
) -> anyhow::Result<ProviderListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider list")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for provider list")?;
    config
        .validate()
        .context("validate config for provider list")?;
    Ok(ProviderListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn provider_list_json(context: &ProviderListContext) -> serde_json::Value {
    let active_provider = context.config.asr.active_provider.as_str();
    let providers = context
        .config
        .asr
        .providers
        .iter()
        .map(|provider| provider_summary_json(provider, active_provider))
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "active_provider": active_provider,
        "provider_count": providers.len(),
        "providers": providers,
        "next_steps": [
            "run vinpst provider use <id> once provider mutation support is available",
            "run vinpst asr-state to inspect the selected provider runtime readiness",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn provider_summary_json(provider: &AsrProviderConfig, active_provider: &str) -> serde_json::Value {
    serde_json::json!({
        "id": provider.id.as_str(),
        "type": asr_provider_kind_label(&provider.kind),
        "active": provider.id.as_str() == active_provider,
        "model": provider.model.as_deref(),
        "hotwords_file_configured": provider.hotwords_file.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "command_configured": provider.command.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "args_count": provider.args.len(),
        "env_count": provider.env.len(),
        "endpoint_configured": provider.endpoint.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "timeout_ms": provider.timeout_ms,
    })
}

fn print_provider_list_text(context: &ProviderListContext) {
    println!("ID\tTYPE\tMODEL\tSTATUS");
    for provider in &context.config.asr.providers {
        println!(
            "{}\t{}\t{}\t{}",
            provider.id,
            asr_provider_kind_label(&provider.kind),
            provider.model.as_deref().unwrap_or("-"),
            if provider.id == context.config.asr.active_provider {
                "active"
            } else {
                ""
            },
        );
    }
}
