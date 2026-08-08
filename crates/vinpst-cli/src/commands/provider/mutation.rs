use super::catalog::load_live_provider_registry;
use super::{
    AsrProviderKind, BTreeMap, Context, LiveScriptKind, ProviderAddOutcome, ProviderAddRequest,
    ProviderRemoveOutcome, ProviderRemoveRequest, ProviderUseOutcome, ProviderUseRequest,
    VinpstConfig, config_set_write_target, default_config_path, load_config_json,
    validate_config_json_value, write_config_set_document,
};

pub(super) fn print_provider_add(request: ProviderAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_add_outcome_json(&outcome))?
        );
    } else {
        print_provider_add_text(&outcome);
    }
    Ok(())
}

fn run_provider_add(request: &ProviderAddRequest<'_>) -> anyhow::Result<ProviderAddOutcome> {
    let id = normalize_provider_id(request.id)?;
    let provider_type = normalize_provider_kind(request.kind)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider add")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for provider add")?;
    if config
        .asr
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("ASR provider `{id}` already exists");
    }
    let before_provider_count = config.asr.providers.len();
    let provider = provider_add_json_object(&id, provider_type, request)?;

    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    providers.push(serde_json::Value::Object(provider));
    validate_config_json_value(&loaded.document, "validate updated provider config")?;

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

    Ok(ProviderAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        provider_type,
        active_provider: config.asr.active_provider,
        before_provider_count,
        after_provider_count: before_provider_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn provider_add_json_object(
    id: &str,
    provider_type: &'static str,
    request: &ProviderAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "type".to_owned(),
        serde_json::Value::String(provider_type.to_owned()),
    );
    if let Some(timeout_ms) = request.timeout_ms {
        object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
    }
    insert_optional_string(&mut object, "model", request.model)?;
    insert_optional_string(&mut object, "hotwords_file", request.hotwords_file)?;
    insert_optional_string(&mut object, "command", request.command)?;
    if !request.args.is_empty() {
        object.insert("args".to_owned(), serde_json::json!(request.args));
    }
    let env = parse_provider_env(request.env)?;
    if !env.is_empty() {
        object.insert("env".to_owned(), serde_json::json!(env));
    }
    insert_optional_string(&mut object, "endpoint", request.endpoint)?;
    Ok(object)
}

fn insert_optional_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("provider field `{key}` cannot be empty");
        }
        object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(())
}

pub(super) fn parse_provider_env(entries: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for entry in entries {
        let (key, value) = entry
            .split_once('=')
            .with_context(|| format!("provider env `{entry}` is not KEY=VALUE"))?;
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("provider env `{entry}` has an empty key");
        }
        env.insert(key.to_owned(), value.to_owned());
    }
    Ok(env)
}

pub(super) fn normalize_provider_kind(kind: &str) -> anyhow::Result<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "local" => Ok("local"),
        "command" => Ok("command"),
        "remote" => Ok("remote"),
        other => {
            anyhow::bail!("unsupported ASR provider type `{other}`; use local, command, or remote")
        }
    }
}

fn provider_add_outcome_json(outcome: &ProviderAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "provider_type": outcome.provider_type,
        "active_provider": outcome.active_provider,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst provider list to verify configured ASR providers",
            "run vinpst provider use <id> to activate the new provider",
            "run vinpst asr-state to inspect provider runtime readiness"
        ],
    })
}

fn print_provider_add_text(outcome: &ProviderAddOutcome) {
    let preview = format!(
        "Would add ASR provider `{}` ({}).",
        outcome.provider_id, outcome.provider_type
    );
    let applied = format!(
        "Added ASR provider `{}` ({}).",
        outcome.provider_id, outcome.provider_type
    );
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

pub(super) fn print_provider_remove(request: ProviderRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_remove_outcome_json(&outcome))?
        );
    } else {
        print_provider_remove_text(&outcome);
    }
    Ok(())
}

fn run_provider_remove(
    request: &ProviderRemoveRequest<'_>,
) -> anyhow::Result<ProviderRemoveOutcome> {
    let selector = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider remove")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for provider remove")?;
    config
        .validate()
        .context("validate config for provider remove")?;

    let provider_index = if let Some(index) = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == selector)
    {
        index
    } else if let Some(registry_path) = request.registry_path {
        let registry = load_live_provider_registry(Some(registry_path), &config.registry)?;
        let entry = registry
            .registry
            .entry_by_id_or_short_id(&selector, LiveScriptKind::AsrProvider)
            .with_context(|| format!("ASR provider selector `{selector}` not found"))?;
        config
            .asr
            .providers
            .iter()
            .position(|provider| provider.id == entry.id)
            .with_context(|| {
                format!(
                    "ASR provider `{}` resolved from `{selector}` is not installed",
                    entry.id
                )
            })?
    } else {
        anyhow::bail!(
            "ASR provider `{selector}` not found; pass --registry <providers.json> to resolve a short id"
        );
    };

    let provider = config.asr.providers[provider_index].clone();
    if provider.kind == AsrProviderKind::Local {
        anyhow::bail!("local ASR provider `{}` cannot be removed", provider.id);
    }
    let removed_provider_type = asr_provider_kind_label(&provider.kind);
    let before_provider_count = config.asr.providers.len();
    let removed_active_provider = provider.id == config.asr.active_provider;

    if removed_active_provider {
        *loaded
            .document
            .pointer_mut("/asr/active_provider")
            .with_context(|| "config pointer `/asr/active_provider` not found")? =
            serde_json::Value::String(String::new());
    }
    let providers = loaded
        .document
        .pointer_mut("/asr/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/asr/providers` not found or not an array")?;
    providers.remove(provider_index);
    validate_config_json_value(&loaded.document, "validate updated provider config")?;

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

    Ok(ProviderRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_provider_id: provider.id,
        removed_provider_type,
        active_provider: if removed_active_provider {
            String::new()
        } else {
            config.asr.active_provider
        },
        before_provider_count,
        after_provider_count: before_provider_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn provider_remove_outcome_json(outcome: &ProviderRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_provider_id": outcome.removed_provider_id,
        "removed_provider_type": outcome.removed_provider_type,
        "active_provider": outcome.active_provider,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst provider list to verify configured ASR providers",
            "run vinpst asr-state to inspect the active provider runtime readiness",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_remove_text(outcome: &ProviderRemoveOutcome) {
    let preview = format!(
        "Would remove ASR provider `{}`.",
        outcome.removed_provider_id
    );
    let applied = format!("Removed ASR provider `{}`.", outcome.removed_provider_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

pub(super) fn print_provider_use(request: ProviderUseRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_provider_use(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&provider_use_outcome_json(&outcome))?
        );
    } else {
        print_provider_use_text(&outcome);
    }
    Ok(())
}

fn run_provider_use(request: &ProviderUseRequest<'_>) -> anyhow::Result<ProviderUseOutcome> {
    let after = normalize_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for provider use")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for provider use")?;
    let provider = config
        .asr
        .providers
        .iter()
        .find(|provider| provider.id == after)
        .with_context(|| format!("ASR provider `{after}` not found"))?;
    let provider_type = asr_provider_kind_label(&provider.kind);
    let before = config.asr.active_provider;
    *loaded
        .document
        .pointer_mut("/asr/active_provider")
        .with_context(|| "config pointer `/asr/active_provider` not found")? =
        serde_json::Value::String(after.clone());
    validate_config_json_value(&loaded.document, "validate updated provider config")?;

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

    Ok(ProviderUseOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        before,
        after,
        provider_type,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

pub(super) fn normalize_provider_id(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("ASR provider id cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn provider_use_outcome_json(outcome: &ProviderUseOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "before": outcome.before,
        "after": outcome.after,
        "provider_type": outcome.provider_type,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst provider list to verify the active provider",
            "run vinpst asr-state to inspect the selected provider runtime readiness",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_provider_use_text(outcome: &ProviderUseOutcome) {
    let preview = format!("Would select ASR provider `{}`.", outcome.after);
    let applied = format!("Selected ASR provider `{}`.", outcome.after);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

pub(super) fn asr_provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}
