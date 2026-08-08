use crate::{
    Context, Path, PathBuf, SceneCommand, VinpstConfig, config_set_write_target,
    default_config_path, load_config_json, validate_config_json_value, write_config_set_document,
};

struct SceneListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinpstConfig,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneAddRequest<'a> {
    id: &'a str,
    label: &'a str,
    prompt: Option<&'a str>,
    provider_id: Option<&'a str>,
    model: Option<&'a str>,
    candidate_count: u8,
    timeout_ms: Option<u64>,
    context_lines: u8,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneEditRequest<'a> {
    id: &'a str,
    label: Option<&'a str>,
    prompt: Option<&'a str>,
    clear_prompt: bool,
    provider_id: Option<&'a str>,
    clear_provider_id: bool,
    model: Option<&'a str>,
    clear_model: bool,
    candidate_count: Option<u8>,
    timeout_ms: Option<u64>,
    clear_timeout: bool,
    context_lines: Option<u8>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneRemoveRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct SceneAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    scene_id: String,
    active_scene: String,
    before_scene_count: usize,
    after_scene_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct SceneEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    scene_id: String,
    active_scene: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct SceneRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_scene_id: String,
    active_scene: String,
    before_scene_count: usize,
    after_scene_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct SceneUseRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct SceneUseOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    before: String,
    after: String,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

pub(crate) fn handle_scene_command(command: SceneCommand) -> anyhow::Result<()> {
    match command {
        SceneCommand::List { config, json } => print_scene_list(config.as_ref(), json),
        SceneCommand::Add {
            id,
            label,
            prompt,
            provider_id,
            model,
            candidate_count,
            timeout_ms,
            context_lines,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_add(SceneAddRequest {
            id: &id,
            label: &label,
            prompt: prompt.as_deref(),
            provider_id: provider_id.as_deref(),
            model: model.as_deref(),
            candidate_count,
            timeout_ms,
            context_lines,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        SceneCommand::Edit {
            id,
            label,
            prompt,
            clear_prompt,
            provider_id,
            clear_provider_id,
            model,
            clear_model,
            candidate_count,
            timeout_ms,
            clear_timeout,
            context_lines,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_edit(SceneEditRequest {
            id: &id,
            label: label.as_deref(),
            prompt: prompt.as_deref(),
            clear_prompt,
            provider_id: provider_id.as_deref(),
            clear_provider_id,
            model: model.as_deref(),
            clear_model,
            candidate_count,
            timeout_ms,
            clear_timeout,
            context_lines,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        SceneCommand::Use {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_use(SceneUseRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        SceneCommand::Remove {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_scene_remove(SceneRemoveRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

fn print_scene_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_scene_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_list_json(&context))?
        );
    } else {
        print_scene_list_text(&context);
    }
    Ok(())
}

fn load_scene_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<SceneListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene list")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for scene list")?;
    config
        .validate()
        .context("validate config for scene list")?;
    Ok(SceneListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn scene_list_json(context: &SceneListContext) -> serde_json::Value {
    let active_scene = context.config.scenes.active_scene.as_str();
    let scenes = context
        .config
        .scenes
        .definitions
        .iter()
        .map(|scene| scene_summary_json(scene, active_scene))
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "active_scene": active_scene,
        "scene_count": scenes.len(),
        "scenes": scenes,
        "next_steps": [
            "run vinpst scene use <id> --dry-run --json to preview scene selection",
            "run vinpst recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn scene_summary_json(
    scene: &vinpst_config::SceneDefinition,
    active_scene: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": scene.id.as_str(),
        "label": scene.label.as_str(),
        "active": scene.id.as_str() == active_scene,
        "prompt_configured": scene.prompt.as_ref().is_some_and(|value| !value.trim().is_empty()),
        "provider_id": scene.provider_id.as_deref(),
        "model": scene.model.as_deref(),
        "candidate_count": scene.candidate_count,
        "timeout_ms": scene.timeout_ms,
        "context_lines": scene.context_lines,
    })
}

fn print_scene_list_text(context: &SceneListContext) {
    println!("ID\tLABEL\tPROVIDER\tMODEL\tCANDIDATES\tSTATUS");
    for scene in &context.config.scenes.definitions {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            scene.id,
            scene_display_label(&scene.label),
            scene.provider_id.as_deref().unwrap_or("-"),
            scene.model.as_deref().unwrap_or("-"),
            scene.candidate_count,
            if scene.id == context.config.scenes.active_scene {
                "active"
            } else {
                ""
            },
        );
    }
}

fn scene_display_label(label: &str) -> &str {
    match label {
        "__label_raw__" => "Raw",
        "__label_command__" => "Command",
        _ => label,
    }
}

fn print_scene_add(request: SceneAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_add_outcome_json(&outcome))?
        );
    } else {
        print_scene_add_text(&outcome);
    }
    Ok(())
}

fn run_scene_add(request: &SceneAddRequest<'_>) -> anyhow::Result<SceneAddOutcome> {
    let id = normalize_scene_id(request.id)?;
    let label = normalize_scene_label(request.label)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene add")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for scene add")?;
    config.validate().context("validate config for scene add")?;
    if config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` already exists");
    }
    let before_scene_count = config.scenes.definitions.len();
    let scene = scene_add_json_object(&id, &label, request)?;
    scene_definitions_array_mut(&mut loaded.document)?.push(serde_json::Value::Object(scene));
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

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
    Ok(SceneAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        scene_id: id,
        active_scene: config.scenes.active_scene,
        before_scene_count,
        after_scene_count: before_scene_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_scene_edit(request: SceneEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_edit_outcome_json(&outcome))?
        );
    } else {
        print_scene_edit_text(&outcome);
    }
    Ok(())
}

fn run_scene_edit(request: &SceneEditRequest<'_>) -> anyhow::Result<SceneEditOutcome> {
    let id = normalize_scene_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene edit")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for scene edit")?;
    config
        .validate()
        .context("validate config for scene edit")?;
    if !config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` not found");
    }
    let scene_index = explicit_scene_index(&loaded.document, &id)?;
    let scene_object = scene_definitions_array_mut(&mut loaded.document)?
        .get_mut(scene_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("scene `{id}` is not a JSON object"))?;
    let changed_fields = apply_scene_edit(scene_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("scene edit requires at least one field change");
    }
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

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
    Ok(SceneEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        scene_id: id,
        active_scene: config.scenes.active_scene,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_scene_remove(request: SceneRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_remove_outcome_json(&outcome))?
        );
    } else {
        print_scene_remove_text(&outcome);
    }
    Ok(())
}

fn run_scene_remove(request: &SceneRemoveRequest<'_>) -> anyhow::Result<SceneRemoveOutcome> {
    let id = normalize_scene_id(request.id)?;
    if matches!(
        id.as_str(),
        vinpst_config::RAW_SCENE_ID | vinpst_config::COMMAND_SCENE_ID
    ) {
        anyhow::bail!("refusing to remove built-in scene `{id}`");
    }
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene remove")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for scene remove")?;
    config
        .validate()
        .context("validate config for scene remove")?;
    if !config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` not found");
    }
    if id == config.scenes.active_scene {
        anyhow::bail!("refusing to remove active scene `{id}`; run vinpst scene use <id> first");
    }
    let before_scene_count = config.scenes.definitions.len();
    let scene_index = explicit_scene_index(&loaded.document, &id)?;
    scene_definitions_array_mut(&mut loaded.document)?.remove(scene_index);
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

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
    Ok(SceneRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_scene_id: id,
        active_scene: config.scenes.active_scene,
        before_scene_count,
        after_scene_count: before_scene_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn scene_add_json_object(
    id: &str,
    label: &str,
    request: &SceneAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "label".to_owned(),
        serde_json::Value::String(label.to_owned()),
    );
    insert_optional_scene_string(&mut object, "prompt", request.prompt)?;
    insert_optional_scene_string(&mut object, "provider_id", request.provider_id)?;
    insert_optional_scene_string(&mut object, "model", request.model)?;
    object.insert(
        "candidate_count".to_owned(),
        serde_json::json!(request.candidate_count),
    );
    if let Some(timeout_ms) = request.timeout_ms {
        object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
    }
    object.insert(
        "context_lines".to_owned(),
        serde_json::json!(request.context_lines),
    );
    Ok(object)
}

fn insert_optional_scene_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("scene field `{key}` cannot be empty");
        }
        object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(())
}

fn apply_scene_edit(
    scene_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &SceneEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(label) = request.label {
        scene_object.insert(
            "label".to_owned(),
            serde_json::Value::String(normalize_scene_label(label)?),
        );
        changed.push("label".to_owned());
    }
    apply_optional_scene_string_edit(
        scene_object,
        "prompt",
        "prompt",
        request.prompt,
        request.clear_prompt,
        &mut changed,
    )?;
    apply_optional_scene_string_edit(
        scene_object,
        "provider_id",
        "provider-id",
        request.provider_id,
        request.clear_provider_id,
        &mut changed,
    )?;
    apply_optional_scene_string_edit(
        scene_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    if let Some(candidate_count) = request.candidate_count {
        scene_object.insert(
            "candidate_count".to_owned(),
            serde_json::json!(candidate_count),
        );
        changed.push("candidate_count".to_owned());
    }
    if request.timeout_ms.is_some() && request.clear_timeout {
        anyhow::bail!("scene edit cannot combine --timeout-ms and --clear-timeout");
    }
    if let Some(timeout_ms) = request.timeout_ms {
        scene_object.insert("timeout_ms".to_owned(), serde_json::json!(timeout_ms));
        changed.push("timeout_ms".to_owned());
    } else if request.clear_timeout {
        scene_object.remove("timeout_ms");
        changed.push("timeout_ms".to_owned());
    }
    if let Some(context_lines) = request.context_lines {
        scene_object.insert("context_lines".to_owned(), serde_json::json!(context_lines));
        changed.push("context_lines".to_owned());
    }
    Ok(changed)
}

fn apply_optional_scene_string_edit(
    scene_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("scene edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("scene field `{key}` cannot be empty");
        }
        scene_object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
        changed.push(key.to_owned());
    } else if clear {
        scene_object.remove(key);
        changed.push(key.to_owned());
    }
    Ok(())
}

fn explicit_scene_index(document: &serde_json::Value, id: &str) -> anyhow::Result<usize> {
    document
        .pointer("/scenes/definitions")
        .and_then(serde_json::Value::as_array)
        .with_context(|| "config pointer `/scenes/definitions` not found or not an array")?
        .iter()
        .position(|scene| scene.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .with_context(|| format!("scene `{id}` is not explicitly configured"))
}

fn scene_definitions_array_mut(
    document: &mut serde_json::Value,
) -> anyhow::Result<&mut Vec<serde_json::Value>> {
    document
        .pointer_mut("/scenes/definitions")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/scenes/definitions` not found or not an array")
}

fn normalize_scene_label(label: &str) -> anyhow::Result<String> {
    let label = label.trim();
    if label.is_empty() {
        anyhow::bail!("scene label cannot be empty");
    }
    Ok(label.to_owned())
}

fn scene_add_outcome_json(outcome: &SceneAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "scene_id": outcome.scene_id,
        "active_scene": outcome.active_scene,
        "before_scene_count": outcome.before_scene_count,
        "after_scene_count": outcome.after_scene_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst scene list to verify configured scenes",
            "run vinpst scene use <id> --dry-run --json to preview scene selection",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn scene_edit_outcome_json(outcome: &SceneEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "scene_id": outcome.scene_id,
        "active_scene": outcome.active_scene,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst scene list to verify configured scenes",
            "run vinpst recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn scene_remove_outcome_json(outcome: &SceneRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_scene_id": outcome.removed_scene_id,
        "active_scene": outcome.active_scene,
        "before_scene_count": outcome.before_scene_count,
        "after_scene_count": outcome.after_scene_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst scene list to verify configured scenes",
            "run vinpst recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_scene_add_text(outcome: &SceneAddOutcome) {
    let preview = format!("Would add scene `{}`.", outcome.scene_id);
    let applied = format!("Added scene `{}`.", outcome.scene_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_scene_edit_text(outcome: &SceneEditOutcome) {
    let preview = format!("Would update scene `{}`.", outcome.scene_id);
    let applied = format!("Updated scene `{}`.", outcome.scene_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_scene_remove_text(outcome: &SceneRemoveOutcome) {
    let preview = format!("Would remove scene `{}`.", outcome.removed_scene_id);
    let applied = format!("Removed scene `{}`.", outcome.removed_scene_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_scene_use(request: SceneUseRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_scene_use(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&scene_use_outcome_json(&outcome))?
        );
    } else {
        print_scene_use_text(&outcome);
    }
    Ok(())
}

fn run_scene_use(request: &SceneUseRequest<'_>) -> anyhow::Result<SceneUseOutcome> {
    let id = normalize_scene_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for scene use")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for scene use")?;
    config.validate().context("validate config for scene use")?;
    if !config.scenes.definitions.iter().any(|scene| scene.id == id) {
        anyhow::bail!("scene `{id}` not found");
    }
    let before = config.scenes.active_scene;
    *loaded
        .document
        .pointer_mut("/scenes/active_scene")
        .with_context(|| "config pointer `/scenes/active_scene` not found")? =
        serde_json::Value::String(id.clone());
    validate_config_json_value(&loaded.document, "validate updated scene config")?;

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

    Ok(SceneUseOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        before,
        after: id,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn scene_use_outcome_json(outcome: &SceneUseOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "before": outcome.before,
        "after": outcome.after,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst scene list to verify the active scene",
            "run vinpst recording start --dry-run --json to inspect recording D-Bus calls",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_scene_use_text(outcome: &SceneUseOutcome) {
    let preview = format!("Would select scene `{}`.", outcome.after);
    let applied = format!("Selected scene `{}`.", outcome.after);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn normalize_scene_id(id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("scene id cannot be empty");
    }
    Ok(id.to_owned())
}
