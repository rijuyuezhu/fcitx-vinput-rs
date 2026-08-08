use super::mutation::{explicit_adapter_index, llm_adapters_array_mut, normalize_adapter_id};
use super::{
    AdapterInstallOutcome, AdapterInstallRequest, AdapterListContext, Context, Duration,
    LiveRegistryI18n, LiveScriptKind, LiveScriptRegistry, LoadedLiveI18n, LoadedLiveScriptRegistry,
    Path, PathBuf, RegistryConfig, RegistryIndex, ReqwestRegistryAssetSource,
    ReqwestRegistryTextSource, VinpstConfig, config_set_write_target, default_adapter_root,
    default_config_path, fetch_text_from_mirrors, fs, install_live_script, live_registry_urls,
    load_config_file, load_config_json, load_live_i18n, managed_script_relative_path,
    materialize_llm_adapter, validate_config_json_value, write_config_set_document,
};

pub(super) fn print_adapter_install(request: AdapterInstallRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_install(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_install_outcome_json(&outcome))?
        );
    } else {
        print_adapter_install_text(&outcome);
    }
    Ok(())
}

fn run_adapter_install(
    request: &AdapterInstallRequest<'_>,
) -> anyhow::Result<AdapterInstallOutcome> {
    let selector = normalize_adapter_id(request.selector)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter install")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for adapter install")?;
    config
        .validate()
        .context("validate config for adapter install")?;
    let loaded_registry = load_live_adapter_registry(request.registry_path, &config.registry)?;
    let entry = loaded_registry
        .registry
        .entry_by_id_or_short_id(&selector, LiveScriptKind::LlmAdapter)
        .with_context(|| format!("text adapter `{selector}` not found in live registry"))?
        .clone();
    let adapter_root = request
        .adapter_root
        .map(Path::to_path_buf)
        .map_or_else(default_adapter_root, Ok)?;
    let script_path = adapter_root.join(managed_script_relative_path(
        LiveScriptKind::LlmAdapter,
        &entry.id,
    )?);
    let existing = config
        .llm
        .adapters
        .iter()
        .find(|adapter| adapter.id == entry.id);
    let materialized = materialize_llm_adapter(&entry, &script_path, existing)?;
    let adapter_value = serde_json::to_value(&materialized.adapter)
        .context("serialize installed adapter config")?;
    if existing.is_some() {
        let index = explicit_adapter_index(&loaded.document, &entry.id)?;
        llm_adapters_array_mut(&mut loaded.document)?[index] = adapter_value;
    } else {
        llm_adapters_array_mut(&mut loaded.document)?.push(adapter_value);
    }
    validate_config_json_value(&loaded.document, "validate installed adapter config")?;

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
            install_live_script(&source, LiveScriptKind::LlmAdapter, &entry, &adapter_root)?;
        if installed.script_path != script_path {
            anyhow::bail!(
                "installed adapter script path `{}` did not match planned path `{}`",
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
    Ok(AdapterInstallOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        registry_source: loaded_registry.source_json,
        adapter_id: entry.id,
        short_id: entry.short_id,
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

fn adapter_install_outcome_json(outcome: &AdapterInstallOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "registry_source": outcome.registry_source,
        "adapter_id": outcome.adapter_id,
        "short_id": outcome.short_id,
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
            "set required adapter environment values in config before starting it",
            "run vinpst adapter list to verify the installed adapter",
            "run vinpst adapter start <id> and vinpst adapter status <id>"
        ],
    })
}

fn print_adapter_install_text(outcome: &AdapterInstallOutcome) {
    let preview = format!("Would install text adapter `{}`.", outcome.adapter_id);
    let applied = format!("Installed text adapter `{}`.", outcome.adapter_id);
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

pub(super) fn load_live_adapter_registry(
    registry_path: Option<&Path>,
    registry_config: &RegistryConfig,
) -> anyhow::Result<LoadedLiveScriptRegistry> {
    let registry_urls = live_registry_urls(registry_config, "registry/adapters.json");
    if let Some(path) = registry_path {
        let input = fs::read_to_string(path)
            .with_context(|| format!("read live adapter registry `{}`", path.display()))?;
        let registry = LiveScriptRegistry::from_json_str(&input, LiveScriptKind::LlmAdapter)
            .with_context(|| format!("validate live adapter registry `{}`", path.display()))?;
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
        .context("fetch live adapter registry from configured mirrors")?;
    let registry = LiveScriptRegistry::from_json_str(&fetched.text, LiveScriptKind::LlmAdapter)
        .with_context(|| {
            format!(
                "validate live adapter registry fetched from `{}`",
                fetched.url
            )
        })?;
    let remote_base_url = fetched
        .url
        .strip_suffix("/registry/adapters.json")
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

pub(super) fn print_adapter_install_plan(
    id: &str,
    registry_path: &Path,
    target_root: &Path,
    config_path: Option<&PathBuf>,
    summary_only: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let id = normalize_adapter_id(id)?;
    let input = fs::read_to_string(registry_path)
        .with_context(|| format!("read registry index `{}`", registry_path.display()))?;
    let index = RegistryIndex::from_json_str(&input)
        .with_context(|| format!("validate registry index `{}`", registry_path.display()))?;
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinpstConfig::bundled_default().context("parse bundled config")?,
    };
    let target_root_string = target_root.to_string_lossy();
    let plan = index.install_adapter_plan(&id, &config.registry, &target_root_string)?;
    if json_output {
        let output = if summary_only {
            serde_json::json!({
                "ok": true,
                "adapter_id": id,
                "registry_path": registry_path,
                "target_root": plan.target_root,
                "asset_count": plan.summary.asset_count,
                "known_size_bytes": plan.summary.known_size_bytes,
                "missing_checksum_count": plan.summary.missing_checksum_count,
            })
        } else {
            serde_json::json!({
                "ok": true,
                "adapter_id": id,
                "registry_path": registry_path,
                "target_root": plan.target_root,
                "asset_count": plan.summary.asset_count,
                "known_size_bytes": plan.summary.known_size_bytes,
                "missing_checksum_count": plan.summary.missing_checksum_count,
                "assets": plan.assets,
            })
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_adapter_install_plan_text(&id, registry_path, &plan, summary_only);
    }
    Ok(())
}

fn print_adapter_install_plan_text(
    id: &str,
    registry_path: &Path,
    plan: &vinpst_registry::InstallPlan,
    summary_only: bool,
) {
    println!("adapter_id: {id}");
    println!("registry_path: {}", registry_path.display());
    println!("target_root: {}", plan.target_root);
    println!("asset_count: {}", plan.summary.asset_count);
    println!("known_size_bytes: {}", plan.summary.known_size_bytes);
    println!(
        "missing_checksum_count: {}",
        plan.summary.missing_checksum_count
    );
    if summary_only {
        return;
    }
    println!("source_path	target_path	urls	checksum_policy	size_bytes");
    for asset in &plan.assets {
        println!(
            "{}	{}	{}	{:?}	{}",
            asset.source_path,
            asset.target_path,
            asset.urls.len(),
            asset.checksum_policy,
            asset
                .size_bytes
                .map_or_else(|| "-".to_owned(), |size| size.to_string()),
        );
    }
}

pub(super) fn print_adapter_list(
    config_path: Option<&PathBuf>,
    available: bool,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    locale: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    if available {
        return print_available_adapter_list(
            registry_path,
            i18n_path,
            config_path,
            locale,
            json_output,
        );
    }
    let context = load_adapter_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_list_json(&context))?
        );
    } else {
        print_adapter_list_text(&context);
    }
    Ok(())
}

fn print_available_adapter_list(
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    let context = load_adapter_list_context(config_path)?;
    let loaded = load_live_adapter_registry(registry_path, &context.config.registry)?;
    let loaded_i18n = load_live_i18n(i18n_path, loaded.remote_base_url.as_deref(), locale)?;
    let configured_ids = context
        .config
        .llm
        .adapters
        .iter()
        .map(|adapter| adapter.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let adapters = loaded
        .registry
        .items
        .iter()
        .map(|adapter| {
            available_live_adapter_json(adapter, loaded_i18n.i18n.as_ref(), &configured_ids)
        })
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "ok": true,
        "registry_source": loaded.source_json,
        "i18n": loaded_i18n.source_json,
        "config_path": context.config_path.as_ref(),
        "config_source": context.source,
        "adapter_count": adapters.len(),
        "adapters": adapters,
        "next_steps": [
            "run vinpst adapter install <id-or-short-id> --dry-run --json to preview installation",
            "run vinpst adapter install <id-or-short-id> --in-place to install and configure it",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    });
    if json_output {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_available_adapter_list_text(&loaded, &loaded_i18n, &context, &configured_ids);
    }
    Ok(())
}

fn available_live_adapter_json(
    adapter: &vinpst_registry::LiveScriptEntry,
    i18n: Option<&LiveRegistryI18n>,
    configured_ids: &std::collections::BTreeSet<&str>,
) -> serde_json::Value {
    let envs = adapter
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
        "id": adapter.short_id.as_deref().unwrap_or(&adapter.id),
        "machine_id": adapter.id,
        "title": adapter.resolved_title(i18n),
        "description": adapter.resolved_description(i18n),
        "command": adapter.command,
        "stream": adapter.stream,
        "readme_url": adapter.readme_url,
        "envs": envs,
        "status": if configured_ids.contains(adapter.id.as_str()) {
            "installed"
        } else {
            "available"
        },
    })
}

fn print_available_adapter_list_text(
    loaded: &LoadedLiveScriptRegistry,
    loaded_i18n: &LoadedLiveI18n,
    _context: &AdapterListContext,
    configured_ids: &std::collections::BTreeSet<&str>,
) {
    println!("ID\tTITLE\tSTATUS");
    for adapter in &loaded.registry.items {
        println!(
            "{}\t{}\t{}",
            adapter.id,
            adapter.resolved_title(loaded_i18n.i18n.as_ref()),
            if configured_ids.contains(adapter.id.as_str()) {
                "installed"
            } else {
                "available"
            },
        );
    }
}

pub(super) fn load_adapter_list_context(
    config_path: Option<&PathBuf>,
) -> anyhow::Result<AdapterListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter list")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for adapter list")?;
    config
        .validate()
        .context("validate config for adapter list")?;
    Ok(AdapterListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn adapter_list_json(context: &AdapterListContext) -> serde_json::Value {
    let adapters = context
        .config
        .llm
        .adapters
        .iter()
        .map(adapter_summary_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "adapter_count": adapters.len(),
        "adapters": adapters,
        "next_steps": [
            "run vinpst scene list to inspect scenes that need adapters",
            "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn adapter_summary_json(adapter: &vinpst_config::LlmAdapterConfig) -> serde_json::Value {
    serde_json::json!({
        "id": adapter.id.as_str(),
        "command_configured": !adapter.command.trim().is_empty(),
        "args_count": adapter.args.len(),
        "env_count": adapter.env.len(),
        "working_dir_configured": adapter.working_dir.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "extra_field_count": adapter.extra.len(),
    })
}

fn print_adapter_list_text(context: &AdapterListContext) {
    println!("ID\tSTATUS");
    for adapter in &context.config.llm.adapters {
        println!("{}\tconfigured", adapter.id);
    }
}
