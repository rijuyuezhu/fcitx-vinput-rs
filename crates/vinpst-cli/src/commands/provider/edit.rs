use super::{
    Context, InstalledProviderResolution, LiveScriptKind, Path, PathBuf, ProviderEditOutcome,
    ProviderEditRequest, ProviderEditScriptOutcome, ProviderEditScriptRequest, VinpstConfig,
    asr_provider_kind_label, config_set_write_target, default_config_path, load_config_json,
    normalize_provider_id, prepare_provider_script_edit, validate_config_json_value,
    write_config_set_document,
};
use super::{
    catalog::{load_live_provider_registry, load_provider_list_context},
    mutation::{normalize_provider_kind, parse_provider_env},
};

pub(super) fn print_provider_edit(request: ProviderEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_edit_outcome_json(&outcome))?
        );
    } else {
        print_provider_edit_text(&outcome);
    }
    Ok(())
}

fn run_provider_edit(request: &ProviderEditRequest<'_>) -> anyhow::Result<ProviderEditOutcome> {
    let id = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider edit")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for provider edit")?;
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == id)
        .with_context(|| format!("ASR provider `{id}` not found"))?;
    let before_provider = &config.asr.providers[provider_index];
    let before_provider_type = asr_provider_kind_label(&before_provider.kind);

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    let provider_object = providers
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("ASR provider `{id}` is not a JSON object"))?;
    let changed_fields = apply_provider_edit(provider_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("provider edit requires at least one field change");
    }

    validate_config_json_value(&loaded.document, "validate updated provider config")?;
    let updated_contents =
        serde_json::to_string(&loaded.document).context("serialize updated provider config")?;
    let updated_config =
        VinpstConfig::from_json_str(&updated_contents).context("parse updated provider config")?;
    let after_provider = &updated_config.asr.providers[provider_index];
    let after_provider_type = asr_provider_kind_label(&after_provider.kind);

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;

    let mut wrote_config = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
    }

    Ok(ProviderEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        before_provider_type,
        after_provider_type,
        active_provider: config.asr.active_provider,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn provider_edit_outcome_json(outcome: &ProviderEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "before_provider_type": outcome.before_provider_type,
        "after_provider_type": outcome.after_provider_type,
        "active_provider": outcome.active_provider,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst provider list to verify configured ASR providers",
            "run vinpst asr-state to inspect provider runtime readiness",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_edit_text(outcome: &ProviderEditOutcome) {
    let preview = format!("Would update ASR provider `{}`.", outcome.provider_id);
    let applied = format!("Updated ASR provider `{}`.", outcome.provider_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

pub(super) fn print_provider_edit_script(
    request: ProviderEditScriptRequest<'_>,
) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_edit_script(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_edit_script_outcome_json(&outcome))?
        );
    } else {
        print_provider_edit_script_text(&outcome);
    }
    Ok(())
}

fn run_provider_edit_script(
    request: &ProviderEditScriptRequest<'_>,
) -> anyhow::Result<ProviderEditScriptOutcome> {
    let resolution = resolve_installed_provider_selector(
        request.selector,
        request.registry_path,
        request.config_path,
    )?;
    let plan = prepare_provider_script_edit(&resolution.provider, request.editor)?;
    let script_path = plan.script_path.clone();
    let editor_argv = plan.editor.argv().to_vec();
    let mut edited = false;
    let mut exit_status = None;
    if !request.dry_run {
        let outcome = plan.execute()?;
        exit_status = outcome.exit_status;
        edited = true;
    }
    Ok(ProviderEditScriptOutcome {
        selector: resolution.selector,
        provider_id: resolution.provider.id,
        config_path: resolution.config_path,
        source: resolution.source,
        registry_source: resolution.registry_source,
        script_path,
        editor_argv,
        dry_run: request.dry_run,
        edited,
        exit_status,
    })
}

fn resolve_installed_provider_selector(
    selector: &str,
    registry_path: Option<&Path>,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<InstalledProviderResolution> {
    let selector = normalize_provider_id(selector)?;
    let context = load_provider_list_context(config_path)?;
    let (provider, registry_source) = if let Some(provider) = context
        .config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == selector)
    {
        (provider.clone(), None)
    } else if let Some(registry_path) = registry_path {
        let registry = load_live_provider_registry(Some(registry_path), &context.config.registry)?;
        let entry = registry
            .registry
            .entry_by_id_or_short_id(&selector, LiveScriptKind::AsrProvider)
            .with_context(|| format!("ASR provider selector `{selector}` not found"))?;
        let provider = context
            .config
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == entry.id)
            .with_context(|| {
                format!(
                    "ASR provider `{}` resolved from `{selector}` is not installed",
                    entry.id
                )
            })?;
        (provider.clone(), Some(registry.source_json))
    } else {
        anyhow::bail!(
            "ASR provider `{selector}` not found; pass --registry <providers.json> to resolve a short id"
        );
    };
    Ok(InstalledProviderResolution {
        selector,
        provider,
        config_path: context.config_path,
        source: context.source,
        registry_source,
    })
}

fn provider_edit_script_outcome_json(outcome: &ProviderEditScriptOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "selector": outcome.selector,
        "provider_id": outcome.provider_id,
        "config_path": outcome.config_path,
        "source": outcome.source,
        "registry_source": outcome.registry_source,
        "script_path": outcome.script_path,
        "editor": outcome.editor_argv.join(" "),
        "editor_argv": outcome.editor_argv,
        "edited": outcome.edited,
        "exit_status": outcome.exit_status,
        "next_steps": [
            "run vinpst provider list to verify the installed provider",
            "run vinpst asr-state to inspect provider runtime readiness"
        ],
    })
}

fn print_provider_edit_script_text(outcome: &ProviderEditScriptOutcome) {
    if outcome.dry_run {
        println!(
            "Would open the script for ASR provider `{}`: {}",
            outcome.provider_id,
            outcome.script_path.display()
        );
    } else {
        println!(
            "Edited the script for ASR provider `{}`: {}",
            outcome.provider_id,
            outcome.script_path.display()
        );
    }
}

fn apply_provider_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &ProviderEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(kind) = request.kind {
        provider_object.insert(
            "type".to_owned(),
            serde_json::Value::String(normalize_provider_kind(kind)?.to_owned()),
        );
        changed.push("type".to_owned());
    }
    apply_optional_provider_string_edit(
        provider_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    apply_optional_provider_string_edit(
        provider_object,
        "hotwords_file",
        "hotwords-file",
        request.hotwords_file,
        request.clear_hotwords_file,
        &mut changed,
    )?;
    apply_optional_provider_string_edit(
        provider_object,
        "command",
        "command",
        request.command,
        request.clear_command,
        &mut changed,
    )?;
    if !request.args.is_empty() && request.clear_args {
        anyhow::bail!("provider edit cannot combine --arg and --clear-args");
    }
    if !request.args.is_empty() {
        provider_object.insert("args".to_owned(), serde_json::json!(request.args));
        changed.push("args".to_owned());
    } else if request.clear_args {
        provider_object.remove("args");
        changed.push("args".to_owned());
    }
    if !request.env.is_empty() && request.clear_env {
        anyhow::bail!("provider edit cannot combine --env and --clear-env");
    }
    if !request.env.is_empty() {
        provider_object.insert(
            "env".to_owned(),
            serde_json::json!(parse_provider_env(request.env)?),
        );
        changed.push("env".to_owned());
    } else if request.clear_env {
        provider_object.remove("env");
        changed.push("env".to_owned());
    }
    apply_optional_provider_string_edit(
        provider_object,
        "endpoint",
        "endpoint",
        request.endpoint,
        request.clear_endpoint,
        &mut changed,
    )?;
    if request.timeout_ms.is_some() && request.clear_timeout {
        anyhow::bail!("provider edit cannot combine --timeout-ms and --clear-timeout");
    }
    if let Some(timeout_ms) = request.timeout_ms {
        provider_object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
        changed.push("timeout_ms".to_owned());
    } else if request.clear_timeout {
        provider_object.remove("timeout_ms");
        changed.push("timeout_ms".to_owned());
    }
    Ok(changed)
}

fn apply_optional_provider_string_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("provider edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("provider field `{key}` cannot be empty");
        }
        provider_object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
        changed.push(key.to_owned());
    } else if clear {
        provider_object.remove(key);
        changed.push(key.to_owned());
    }
    Ok(())
}
