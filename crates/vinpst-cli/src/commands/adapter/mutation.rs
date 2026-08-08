use super::catalog::load_live_adapter_registry;
use super::{
    AdapterAddOutcome, AdapterAddRequest, AdapterEditOutcome, AdapterEditRequest,
    AdapterRemoveOutcome, AdapterRemoveRequest, BTreeMap, Context, LiveScriptKind, Path, PathBuf,
    VinpstConfig, config_set_write_target, default_adapter_root, default_config_path, fs,
    load_config_json, managed_script_relative_path, validate_config_json_value,
    write_config_set_document,
};

pub(super) fn print_adapter_edit(request: AdapterEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_edit_outcome_json(&outcome))?
        );
    } else {
        print_adapter_edit_text(&outcome);
    }
    Ok(())
}

fn run_adapter_edit(request: &AdapterEditRequest<'_>) -> anyhow::Result<AdapterEditOutcome> {
    let id = normalize_adapter_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter edit")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for adapter edit")?;
    config
        .validate()
        .context("validate config for adapter edit")?;
    if !config.llm.adapters.iter().any(|adapter| adapter.id == id) {
        anyhow::bail!("text adapter `{id}` not found");
    }
    let adapter_index = explicit_adapter_index(&loaded.document, &id)?;
    let adapter_object = llm_adapters_array_mut(&mut loaded.document)?
        .get_mut(adapter_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("text adapter `{id}` is not a JSON object"))?;
    let changed_fields = apply_adapter_edit(adapter_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("text adapter edit requires at least one field change");
    }
    validate_config_json_value(&loaded.document, "validate updated adapter config")?;

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
    Ok(AdapterEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        adapter_id: id,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn apply_adapter_edit(
    adapter_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &AdapterEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(command) = request.command {
        adapter_object.insert(
            "command".to_owned(),
            serde_json::Value::String(normalize_adapter_command(command)?),
        );
        changed.push("command".to_owned());
    }
    if !request.args.is_empty() && request.clear_args {
        anyhow::bail!("text adapter edit cannot combine --arg and --clear-args");
    }
    if !request.args.is_empty() {
        adapter_object.insert("args".to_owned(), serde_json::json!(request.args));
        changed.push("args".to_owned());
    } else if request.clear_args {
        adapter_object.remove("args");
        changed.push("args".to_owned());
    }
    if !request.env.is_empty() && request.clear_env {
        anyhow::bail!("text adapter edit cannot combine --env and --clear-env");
    }
    if !request.env.is_empty() {
        adapter_object.insert(
            "env".to_owned(),
            serde_json::json!(parse_adapter_env(request.env)?),
        );
        changed.push("env".to_owned());
    } else if request.clear_env {
        adapter_object.remove("env");
        changed.push("env".to_owned());
    }
    if request.working_dir.is_some() && request.clear_working_dir {
        anyhow::bail!("text adapter edit cannot combine --working-dir and --clear-working-dir");
    }
    if let Some(working_dir) = request.working_dir {
        let working_dir = working_dir.trim();
        if working_dir.is_empty() {
            anyhow::bail!("text adapter field `working_dir` cannot be empty");
        }
        adapter_object.insert(
            "working_dir".to_owned(),
            serde_json::Value::String(working_dir.to_owned()),
        );
        changed.push("working_dir".to_owned());
    } else if request.clear_working_dir {
        adapter_object.remove("working_dir");
        changed.push("working_dir".to_owned());
    }
    Ok(changed)
}

fn adapter_edit_outcome_json(outcome: &AdapterEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "adapter_id": outcome.adapter_id,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst adapter list to verify configured text adapters",
            "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_adapter_edit_text(outcome: &AdapterEditOutcome) {
    let preview = format!("Would update text adapter `{}`.", outcome.adapter_id);
    let applied = format!("Updated text adapter `{}`.", outcome.adapter_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

pub(super) fn print_adapter_add(request: AdapterAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_add_outcome_json(&outcome))?
        );
    } else {
        print_adapter_add_text(&outcome);
    }
    Ok(())
}

fn run_adapter_add(request: &AdapterAddRequest<'_>) -> anyhow::Result<AdapterAddOutcome> {
    let id = normalize_adapter_id(request.id)?;
    let command = normalize_adapter_command(request.command)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter add")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for adapter add")?;
    config
        .validate()
        .context("validate config for adapter add")?;
    if config.llm.adapters.iter().any(|adapter| adapter.id == id) {
        anyhow::bail!("text adapter `{id}` already exists");
    }
    let before_adapter_count = config.llm.adapters.len();
    let adapter = adapter_add_json_object(&id, &command, request)?;
    llm_adapters_array_mut(&mut loaded.document)?.push(serde_json::Value::Object(adapter));
    validate_config_json_value(&loaded.document, "validate updated adapter config")?;

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
    Ok(AdapterAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        adapter_id: id,
        before_adapter_count,
        after_adapter_count: before_adapter_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

pub(super) fn print_adapter_remove(request: AdapterRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_adapter_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&adapter_remove_outcome_json(&outcome))?
        );
    } else {
        print_adapter_remove_text(&outcome);
    }
    Ok(())
}

fn run_adapter_remove(request: &AdapterRemoveRequest<'_>) -> anyhow::Result<AdapterRemoveOutcome> {
    let selector = normalize_adapter_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for adapter remove")?;
    let config =
        VinpstConfig::from_json_str(&contents).context("parse config for adapter remove")?;
    config
        .validate()
        .context("validate config for adapter remove")?;

    let (adapter_index, registry_source) = if let Some(index) = config
        .llm
        .adapters
        .iter()
        .position(|adapter| adapter.id == selector)
    {
        (index, None)
    } else if let Some(registry_path) = request.registry_path {
        let registry = load_live_adapter_registry(Some(registry_path), &config.registry)?;
        let entry = registry
            .registry
            .entry_by_id_or_short_id(&selector, LiveScriptKind::LlmAdapter)
            .with_context(|| format!("text adapter selector `{selector}` not found"))?;
        let index = config
            .llm
            .adapters
            .iter()
            .position(|adapter| adapter.id == entry.id)
            .with_context(|| {
                format!(
                    "text adapter `{}` resolved from `{selector}` is not installed",
                    entry.id
                )
            })?;
        (index, Some(registry.source_json))
    } else {
        anyhow::bail!(
            "text adapter `{selector}` not found; pass --registry <adapters.json> to resolve a short id"
        );
    };

    let adapter = config.llm.adapters[adapter_index].clone();
    let adapter_root = request
        .adapter_root
        .map(Path::to_path_buf)
        .map_or_else(default_adapter_root, Ok)?;
    let script_path = managed_adapter_script_path(&adapter, &adapter_root);
    let script_existed = script_path
        .as_deref()
        .map(managed_script_exists)
        .transpose()?
        .unwrap_or(false);
    let before_adapter_count = config.llm.adapters.len();
    llm_adapters_array_mut(&mut loaded.document)?.remove(adapter_index);
    validate_config_json_value(&loaded.document, "validate updated adapter config")?;

    let write_target = config_set_write_target(
        request.output_path,
        request.in_place,
        request.dry_run,
        loaded.path.as_ref(),
        &default_path,
    )?;
    let mut wrote_config = false;
    let mut removed_script = false;
    if !request.dry_run {
        write_config_set_document(&loaded.document, &write_target)?;
        wrote_config = true;
        if write_target.in_place()
            && script_existed
            && let Some(script_path) = &script_path
        {
            fs::remove_file(script_path).with_context(|| {
                format!("remove managed adapter script `{}`", script_path.display())
            })?;
            removed_script = true;
        }
    }
    Ok(AdapterRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        registry_source,
        removed_adapter_id: adapter.id,
        managed_script: script_path.is_some(),
        script_path,
        script_existed,
        removed_script,
        before_adapter_count,
        after_adapter_count: before_adapter_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn managed_adapter_script_path(
    adapter: &vinpst_config::LlmAdapterConfig,
    adapter_root: &Path,
) -> Option<PathBuf> {
    let Ok(relative_path) = managed_script_relative_path(LiveScriptKind::LlmAdapter, &adapter.id)
    else {
        return None;
    };
    let script_path = adapter_root.join(relative_path);
    let script_path_text = script_path.to_string_lossy();
    (adapter.args.as_slice() == [script_path_text.as_ref()]).then_some(script_path)
}

fn managed_script_exists(script_path: &Path) -> anyhow::Result<bool> {
    match fs::symlink_metadata(script_path) {
        Ok(metadata) => {
            if metadata.file_type().is_dir() {
                anyhow::bail!(
                    "refusing to remove managed adapter script `{}` because it is a directory",
                    script_path.display()
                );
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("inspect managed adapter script `{}`", script_path.display())),
    }
}

fn adapter_add_json_object(
    id: &str,
    command: &str,
    request: &AdapterAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "command".to_owned(),
        serde_json::Value::String(command.to_owned()),
    );
    if !request.args.is_empty() {
        object.insert("args".to_owned(), serde_json::json!(request.args));
    }
    if !request.env.is_empty() {
        object.insert(
            "env".to_owned(),
            serde_json::json!(parse_adapter_env(request.env)?),
        );
    }
    if let Some(working_dir) = request.working_dir {
        let trimmed = working_dir.trim();
        if trimmed.is_empty() {
            anyhow::bail!("text adapter field `working_dir` cannot be empty");
        }
        object.insert(
            "working_dir".to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(object)
}

fn parse_adapter_env(entries: &[String]) -> anyhow::Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for entry in entries {
        let (key, value) = entry
            .split_once('=')
            .with_context(|| format!("text adapter env `{entry}` is not KEY=VALUE"))?;
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("text adapter env `{entry}` has an empty key");
        }
        env.insert(key.to_owned(), value.to_owned());
    }
    Ok(env)
}

pub(super) fn llm_adapters_array_mut(
    document: &mut serde_json::Value,
) -> anyhow::Result<&mut Vec<serde_json::Value>> {
    document
        .pointer_mut("/llm/adapters")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/llm/adapters` not found or not an array")
}

pub(super) fn explicit_adapter_index(
    document: &serde_json::Value,
    id: &str,
) -> anyhow::Result<usize> {
    document
        .pointer("/llm/adapters")
        .and_then(serde_json::Value::as_array)
        .with_context(|| "config pointer `/llm/adapters` not found or not an array")?
        .iter()
        .position(|adapter| adapter.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .with_context(|| format!("text adapter `{id}` is not explicitly configured"))
}

pub(super) fn normalize_adapter_id(id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("text adapter id cannot be empty");
    }
    Ok(id.to_owned())
}

fn normalize_adapter_command(command: &str) -> anyhow::Result<String> {
    let command = command.trim();
    if command.is_empty() {
        anyhow::bail!("text adapter command cannot be empty");
    }
    Ok(command.to_owned())
}

fn adapter_add_outcome_json(outcome: &AdapterAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "adapter_id": outcome.adapter_id,
        "before_adapter_count": outcome.before_adapter_count,
        "after_adapter_count": outcome.after_adapter_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst adapter list to verify configured text adapters",
            "run vinpst scene list to inspect scenes that need adapters",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn adapter_remove_outcome_json(outcome: &AdapterRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "registry_source": outcome.registry_source,
        "removed_adapter_id": outcome.removed_adapter_id,
        "managed_script": outcome.managed_script,
        "script_path": outcome.script_path,
        "script_existed": outcome.script_existed,
        "will_remove_script": !outcome.dry_run && outcome.in_place && outcome.script_existed,
        "removed_script": outcome.removed_script,
        "before_adapter_count": outcome.before_adapter_count,
        "after_adapter_count": outcome.after_adapter_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst adapter list to verify configured text adapters",
            "run vinpst scene list to inspect scenes that need adapters",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_adapter_add_text(outcome: &AdapterAddOutcome) {
    let preview = format!("Would add text adapter `{}`.", outcome.adapter_id);
    let applied = format!("Added text adapter `{}`.", outcome.adapter_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_adapter_remove_text(outcome: &AdapterRemoveOutcome) {
    let preview = format!(
        "Would remove text adapter `{}`.",
        outcome.removed_adapter_id
    );
    let applied = format!("Removed text adapter `{}`.", outcome.removed_adapter_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}
