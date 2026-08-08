use crate::{
    Context, LlmCommand, OpenAiCompatibleTextAdapter, Path, PathBuf, RecognitionPayload,
    ReqwestOpenAiCompatibleChatTransport, SceneDefinition, TextAdapter, TextRequest, VinpstConfig,
    build_openai_compatible_chat_request, config_set_write_target, default_config_path,
    load_config_json, validate_config_json_value, write_config_set_document,
};

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct LlmAddRequest<'a> {
    id: &'a str,
    base_url: &'a str,
    api_key: Option<&'a str>,
    model: Option<&'a str>,
    extra_body: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct LlmEditRequest<'a> {
    id: &'a str,
    base_url: Option<&'a str>,
    api_key: Option<&'a str>,
    clear_api_key: bool,
    model: Option<&'a str>,
    clear_model: bool,
    extra_body: Option<&'a str>,
    clear_extra_body: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct LlmEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct LlmRemoveRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct LlmAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct LlmRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_provider_id: String,
    cleared_scene_references: usize,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct LlmListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinpstConfig,
}

pub(crate) fn handle_llm_command(command: LlmCommand) -> anyhow::Result<()> {
    match command {
        LlmCommand::List { config, json } => print_llm_list(config.as_ref(), json),
        LlmCommand::Add {
            id,
            base_url,
            api_key,
            model,
            extra_body,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_llm_add(LlmAddRequest {
            id: &id,
            base_url: &base_url,
            api_key: api_key.as_deref(),
            model: model.as_deref(),
            extra_body: extra_body.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        LlmCommand::Edit {
            id,
            base_url,
            api_key,
            clear_api_key,
            model,
            clear_model,
            extra_body,
            clear_extra_body,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_llm_edit(LlmEditRequest {
            id: &id,
            base_url: base_url.as_deref(),
            api_key: api_key.as_deref(),
            clear_api_key,
            model: model.as_deref(),
            clear_model,
            extra_body: extra_body.as_deref(),
            clear_extra_body,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        LlmCommand::Test {
            id,
            text,
            timeout_ms,
            config,
            dry_run,
            json,
        } => print_llm_test(&id, &text, timeout_ms, config.as_ref(), dry_run, json),
        LlmCommand::Remove {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_llm_remove(LlmRemoveRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

fn print_llm_test(
    id: &str,
    text: &str,
    timeout_ms: Option<u64>,
    config_path: Option<&PathBuf>,
    dry_run: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let outcome = run_llm_test(id, text, timeout_ms, config_path, dry_run)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        print_llm_test_text(&outcome);
    }
    Ok(())
}

fn run_llm_test(
    id: &str,
    text: &str,
    timeout_ms: Option<u64>,
    config_path: Option<&PathBuf>,
    dry_run: bool,
) -> anyhow::Result<serde_json::Value> {
    let id = normalize_llm_provider_id(id)?;
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm test")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for llm test")?;
    config.validate().context("validate config for llm test")?;
    let provider = config
        .llm
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .with_context(|| format!("LLM provider `{id}` not found"))?;
    let scene = llm_test_scene(provider, timeout_ms);
    let effective_timeout_ms = scene.effective_timeout_ms();
    let request = TextRequest {
        raw_text: text,
        scene: &scene,
        selected_text: None,
    };
    let built =
        build_openai_compatible_chat_request(&request, provider, "")?.with_context(|| {
            format!("LLM provider `{id}` cannot build an OpenAI-compatible request")
        })?;
    if dry_run {
        return Ok(llm_test_output(
            loaded.path.as_ref(),
            loaded.source,
            &id,
            effective_timeout_ms,
            true,
            &built,
            None,
            None,
        ));
    }

    let adapter = OpenAiCompatibleTextAdapter::new(
        provider.clone(),
        ReqwestOpenAiCompatibleChatTransport::new(),
    );
    let payload = adapter
        .finish(&request)
        .with_context(|| format!("test LLM provider `{id}`"))?;
    Ok(llm_test_output(
        loaded.path.as_ref(),
        loaded.source,
        &id,
        effective_timeout_ms,
        false,
        &built,
        Some(&payload),
        Some(payload.candidates.len()),
    ))
}

fn llm_test_scene(
    provider: &vinpst_config::LlmProviderConfig,
    timeout_ms: Option<u64>,
) -> SceneDefinition {
    SceneDefinition {
        id: "__llm_test__".to_owned(),
        label: "LLM Test".to_owned(),
        prompt: Some(
            "Return a JSON object with a candidates array containing one short connectivity result."
                .to_owned(),
        ),
        provider_id: Some(provider.id.clone()),
        model: provider.model.clone(),
        candidate_count: 1,
        timeout_ms,
        context_lines: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn llm_test_output(
    config_path: Option<&PathBuf>,
    source: &'static str,
    provider_id: &str,
    timeout_ms: u64,
    dry_run: bool,
    request: &vinpst_text::OpenAiCompatibleChatRequest,
    payload: Option<&RecognitionPayload>,
    candidate_count: Option<usize>,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": dry_run,
        "config_path": config_path,
        "source": source,
        "provider_id": provider_id,
        "timeout_ms": timeout_ms,
        "will_call_http": !dry_run,
        "called": !dry_run,
        "request": {
            "url": request.redacted_url(),
            "headers": request.redacted_headers(),
            "body": request.body,
            "ignored_extra_body_keys": request.ignored_extra_body_keys,
        },
        "result": payload.map(|payload| serde_json::json!({
            "commit_text": payload.commit_text,
            "candidate_count": candidate_count.unwrap_or(0),
        })),
        "next_steps": [
            "run vinpst llm list to verify configured LLM providers",
            "run vinpst scene list to inspect scene/provider bindings",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_llm_test_text(outcome: &serde_json::Value) {
    let provider_id = outcome["provider_id"].as_str().unwrap_or("-");
    if outcome["dry_run"].as_bool().unwrap_or(false) {
        println!("Would test LLM provider `{provider_id}`.");
        if let Some(url) = outcome["request"]["url"].as_str() {
            println!("Request: {url}");
        }
        return;
    }
    println!("LLM provider `{provider_id}` responded successfully.");
    if let Some(result) = outcome.get("result").filter(|value| !value.is_null()) {
        println!(
            "Response: {}",
            result["commit_text"].as_str().unwrap_or("-")
        );
    }
}

fn print_llm_add(request: LlmAddRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_llm_add(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_add_outcome_json(&outcome))?
        );
    } else {
        print_llm_add_text(&outcome);
    }
    Ok(())
}

fn run_llm_add(request: &LlmAddRequest<'_>) -> anyhow::Result<LlmAddOutcome> {
    let id = normalize_llm_provider_id(request.id)?;
    let base_url = normalize_llm_base_url(request.base_url)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm add")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for llm add")?;
    config.validate().context("validate config for llm add")?;
    if config
        .llm
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("LLM provider `{id}` already exists");
    }
    let before_provider_count = config.llm.providers.len();
    let provider = llm_add_json_object(&id, &base_url, request)?;
    llm_providers_array_mut(&mut loaded.document)?.push(serde_json::Value::Object(provider));
    validate_config_json_value(&loaded.document, "validate updated LLM config")?;

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
    Ok(LlmAddOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        before_provider_count,
        after_provider_count: before_provider_count + 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn print_llm_edit(request: LlmEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_llm_edit(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_edit_outcome_json(&outcome))?
        );
    } else {
        print_llm_edit_text(&outcome);
    }
    Ok(())
}

fn run_llm_edit(request: &LlmEditRequest<'_>) -> anyhow::Result<LlmEditOutcome> {
    let id = normalize_llm_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm edit")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for llm edit")?;
    config.validate().context("validate config for llm edit")?;
    if !config
        .llm
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("LLM provider `{id}` not found");
    }
    let provider_index = explicit_llm_provider_index(&loaded.document, &id)?;
    let provider_object = llm_providers_array_mut(&mut loaded.document)?
        .get_mut(provider_index)
        .and_then(serde_json::Value::as_object_mut)
        .with_context(|| format!("LLM provider `{id}` is not a JSON object"))?;
    let changed_fields = apply_llm_edit(provider_object, request)?;
    if changed_fields.is_empty() {
        anyhow::bail!("LLM provider edit requires at least one field change");
    }
    validate_config_json_value(&loaded.document, "validate updated LLM config")?;

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
    Ok(LlmEditOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        provider_id: id,
        changed_fields,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn apply_llm_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    request: &LlmEditRequest<'_>,
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    if let Some(base_url) = request.base_url {
        provider_object.insert(
            "base_url".to_owned(),
            serde_json::Value::String(normalize_llm_base_url(base_url)?),
        );
        changed.push("base_url".to_owned());
    }
    apply_optional_llm_string_edit(
        provider_object,
        "api_key",
        "api-key",
        request.api_key,
        request.clear_api_key,
        &mut changed,
    )?;
    apply_optional_llm_string_edit(
        provider_object,
        "model",
        "model",
        request.model,
        request.clear_model,
        &mut changed,
    )?;
    if request.extra_body.is_some() && request.clear_extra_body {
        anyhow::bail!("LLM provider edit cannot combine --extra-body and --clear-extra-body");
    }
    if let Some(extra_body) = request.extra_body {
        provider_object.insert("extra_body".to_owned(), parse_llm_extra_body(extra_body)?);
        changed.push("extra_body".to_owned());
    } else if request.clear_extra_body {
        provider_object.remove("extra_body");
        changed.push("extra_body".to_owned());
    }
    Ok(changed)
}

fn apply_optional_llm_string_edit(
    provider_object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    option_name: &str,
    value: Option<&str>,
    clear: bool,
    changed: &mut Vec<String>,
) -> anyhow::Result<()> {
    if value.is_some() && clear {
        anyhow::bail!("LLM provider edit cannot combine --{option_name} and --clear-{option_name}");
    }
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("LLM provider field `{key}` cannot be empty");
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

fn llm_edit_outcome_json(outcome: &LlmEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "changed_fields": outcome.changed_fields,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst llm list to verify configured LLM providers",
            "run vinpst scene list to inspect scene/provider bindings",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_llm_edit_text(outcome: &LlmEditOutcome) {
    let preview = format!("Would update LLM provider `{}`.", outcome.provider_id);
    let applied = format!("Updated LLM provider `{}`.", outcome.provider_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_llm_remove(request: LlmRemoveRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_llm_remove(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_remove_outcome_json(&outcome))?
        );
    } else {
        print_llm_remove_text(&outcome);
    }
    Ok(())
}

fn run_llm_remove(request: &LlmRemoveRequest<'_>) -> anyhow::Result<LlmRemoveOutcome> {
    let id = normalize_llm_provider_id(request.id)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm remove")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for llm remove")?;
    config
        .validate()
        .context("validate config for llm remove")?;
    if !config
        .llm
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        anyhow::bail!("LLM provider `{id}` not found");
    }
    let before_provider_count = config.llm.providers.len();
    let provider_index = explicit_llm_provider_index(&loaded.document, &id)?;
    llm_providers_array_mut(&mut loaded.document)?.remove(provider_index);
    let cleared_scene_references = clear_llm_provider_scene_references(&mut loaded.document, &id)?;
    validate_config_json_value(&loaded.document, "validate updated LLM config")?;

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
    Ok(LlmRemoveOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        removed_provider_id: id,
        cleared_scene_references,
        before_provider_count,
        after_provider_count: before_provider_count - 1,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn llm_add_json_object(
    id: &str,
    base_url: &str,
    request: &LlmAddRequest<'_>,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let mut object = serde_json::Map::new();
    object.insert("id".to_owned(), serde_json::Value::String(id.to_owned()));
    object.insert(
        "base_url".to_owned(),
        serde_json::Value::String(base_url.to_owned()),
    );
    insert_optional_llm_string(&mut object, "api_key", request.api_key)?;
    insert_optional_llm_string(&mut object, "model", request.model)?;
    if let Some(extra_body) = request.extra_body {
        object.insert("extra_body".to_owned(), parse_llm_extra_body(extra_body)?);
    }
    Ok(object)
}

fn insert_optional_llm_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(value) = value {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            anyhow::bail!("LLM provider field `{key}` cannot be empty");
        }
        object.insert(
            key.to_owned(),
            serde_json::Value::String(trimmed.to_owned()),
        );
    }
    Ok(())
}

fn parse_llm_extra_body(extra_body: &str) -> anyhow::Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(extra_body).with_context(|| "parse --extra-body as JSON object")?;
    if !value.is_object() {
        anyhow::bail!("LLM provider --extra-body must be a JSON object");
    }
    Ok(value)
}

fn llm_providers_array_mut(
    document: &mut serde_json::Value,
) -> anyhow::Result<&mut Vec<serde_json::Value>> {
    document
        .pointer_mut("/llm/providers")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/llm/providers` not found or not an array")
}

fn clear_llm_provider_scene_references(
    document: &mut serde_json::Value,
    provider_id: &str,
) -> anyhow::Result<usize> {
    let scenes = document
        .pointer_mut("/scenes/definitions")
        .and_then(serde_json::Value::as_array_mut)
        .with_context(|| "config pointer `/scenes/definitions` not found or not an array")?;
    let mut cleared = 0;
    for scene in scenes {
        let object = scene
            .as_object_mut()
            .with_context(|| "scene definition is not a JSON object")?;
        let references_provider = object
            .get("provider_id")
            .and_then(serde_json::Value::as_str)
            == Some(provider_id);
        if references_provider {
            object.remove("provider_id");
            object.remove("model");
            cleared += 1;
        }
    }
    Ok(cleared)
}

fn explicit_llm_provider_index(document: &serde_json::Value, id: &str) -> anyhow::Result<usize> {
    document
        .pointer("/llm/providers")
        .and_then(serde_json::Value::as_array)
        .with_context(|| "config pointer `/llm/providers` not found or not an array")?
        .iter()
        .position(|provider| provider.get("id").and_then(serde_json::Value::as_str) == Some(id))
        .with_context(|| format!("LLM provider `{id}` is not explicitly configured"))
}

fn normalize_llm_provider_id(id: &str) -> anyhow::Result<String> {
    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("LLM provider id cannot be empty");
    }
    Ok(id.to_owned())
}

fn normalize_llm_base_url(base_url: &str) -> anyhow::Result<String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        anyhow::bail!("LLM provider base URL cannot be empty");
    }
    Ok(base_url.to_owned())
}

fn llm_add_outcome_json(outcome: &LlmAddOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "provider_id": outcome.provider_id,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst llm list to verify configured LLM providers",
            "run vinpst scene list to inspect scene/provider bindings",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn llm_remove_outcome_json(outcome: &LlmRemoveOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path.as_ref(),
        "source": outcome.source,
        "removed_provider_id": outcome.removed_provider_id,
        "cleared_scene_references": outcome.cleared_scene_references,
        "before_provider_count": outcome.before_provider_count,
        "after_provider_count": outcome.after_provider_count,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst llm list to verify configured LLM providers",
            "run vinpst scene list to inspect scene/provider bindings",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn print_llm_add_text(outcome: &LlmAddOutcome) {
    let preview = format!("Would add LLM provider `{}`.", outcome.provider_id);
    let applied = format!("Added LLM provider `{}`.", outcome.provider_id);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_llm_remove_text(outcome: &LlmRemoveOutcome) {
    let preview = if outcome.cleared_scene_references == 0 {
        format!(
            "Would remove LLM provider `{}`.",
            outcome.removed_provider_id
        )
    } else {
        format!(
            "Would remove LLM provider `{}` and clear it from {} scene(s).",
            outcome.removed_provider_id, outcome.cleared_scene_references
        )
    };
    let applied = if outcome.cleared_scene_references == 0 {
        format!("Removed LLM provider `{}`.", outcome.removed_provider_id)
    } else {
        format!(
            "Removed LLM provider `{}` and cleared it from {} scene(s).",
            outcome.removed_provider_id, outcome.cleared_scene_references
        )
    };
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn print_llm_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_llm_list_context(config_path)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&llm_list_json(&context))?
        );
    } else {
        print_llm_list_text(&context);
    }
    Ok(())
}

fn load_llm_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<LlmListContext> {
    let loaded = load_config_json(config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for llm list")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for llm list")?;
    config.validate().context("validate config for llm list")?;
    Ok(LlmListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn llm_list_json(context: &LlmListContext) -> serde_json::Value {
    let providers = context
        .config
        .llm
        .providers
        .iter()
        .map(llm_provider_summary_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "ok": true,
        "config_path": context.config_path.as_ref(),
        "source": context.source,
        "provider_count": providers.len(),
        "providers": providers,
        "next_steps": [
            "run vinpst scene list to inspect scene/provider bindings",
            "run vinpst adapter list to inspect command text adapters",
            "run vinpst doctor to inspect full local diagnostics"
        ],
    })
}

fn llm_provider_summary_json(provider: &vinpst_config::LlmProviderConfig) -> serde_json::Value {
    serde_json::json!({
        "id": provider.id.as_str(),
        "base_url_configured": !provider.base_url.trim().is_empty(),
        "api_key_configured": !provider.api_key.trim().is_empty(),
        "model": provider.model.as_deref(),
        "extra_body_configured": provider.extra_body.as_object().is_some_and(|object| !object.is_empty()),
        "extra_field_count": provider.extra.len(),
    })
}

fn print_llm_list_text(context: &LlmListContext) {
    println!("ID\tBASE URL\tMODEL\tAPI KEY");
    for provider in &context.config.llm.providers {
        println!(
            "{}\t{}\t{}\t{}",
            provider.id,
            vinpst_config::redact_url_for_diagnostics(&provider.base_url),
            provider.model.as_deref().unwrap_or("-"),
            if provider.api_key.trim().is_empty() {
                "not set"
            } else {
                "set"
            },
        );
    }
}
