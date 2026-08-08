use crate::{
    ConfigExample, Context, LoadedConfigJson, Path, PathBuf, ProcessCommand, ValueEnum,
    VinpstConfig, config_backup_path, config_example_contents, config_example_description,
    config_set_write_target, config_summary_json, default_config_path, fs, load_config_json,
    split_editor_argv, validate_config_json_value, write_config_set_document,
    write_private_file_atomically,
};

pub(crate) fn handle_config_get(
    pointer: &str,
    config_path: Option<&PathBuf>,
    exists_only: bool,
    default_value: Option<&str>,
    default_string: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    ensure_json_pointer(pointer)?;
    if default_string && default_value.is_none() {
        anyhow::bail!("config get --default-string requires --default <VALUE>");
    }
    let loaded = load_config_json(config_path)?;
    let value = loaded.document.pointer(pointer);
    if exists_only {
        print_config_get_exists(&loaded, pointer, value, json_output)?;
        return Ok(());
    }
    let (value, exists, default_used, parsed_default_kind) = if let Some(value) = value {
        (value.clone(), true, false, None)
    } else {
        let default_value =
            default_value.with_context(|| format!("config pointer `{pointer}` not found"))?;
        let (value, kind) = parse_config_set_value(default_value, default_string);
        (value, false, true, Some(kind))
    };
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "config_path": loaded.path,
                "source": loaded.source,
                "pointer": pointer,
                "exists": exists,
                "default_used": default_used,
                "default_string": default_string,
                "parsed_default_kind": parsed_default_kind,
                "value": value,
            }))?
        );
    } else {
        print_config_value(&value)?;
    }
    Ok(())
}

fn print_config_get_exists(
    loaded: &LoadedConfigJson,
    pointer: &str,
    value: Option<&serde_json::Value>,
    json_output: bool,
) -> anyhow::Result<()> {
    let exists = value.is_some();
    if json_output {
        let mut payload = serde_json::json!({
            "ok": true,
            "config_path": loaded.path.clone(),
            "source": loaded.source,
            "pointer": pointer,
            "exists": exists,
        });
        if let Some(value) = value {
            payload["value"] = value.clone();
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{exists}");
    }
    Ok(())
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ConfigSetRequest<'a> {
    pub(crate) pointer: &'a str,
    pub(crate) raw_value: &'a str,
    pub(crate) force_string: bool,
    pub(crate) config_path: Option<&'a PathBuf>,
    pub(crate) output_path: Option<&'a Path>,
    pub(crate) in_place: bool,
    pub(crate) dry_run: bool,
    pub(crate) json_output: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct ConfigSetOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    pointer: String,
    raw_value: String,
    force_string: bool,
    parsed_value_kind: &'static str,
    before: serde_json::Value,
    after: serde_json::Value,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

pub(crate) fn handle_config_set(request: ConfigSetRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_config_set(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&config_set_outcome_json(&outcome))?
        );
    } else {
        print_config_set_outcome_text(&outcome);
    }
    Ok(())
}

fn run_config_set(request: &ConfigSetRequest<'_>) -> anyhow::Result<ConfigSetOutcome> {
    ensure_json_pointer(request.pointer)?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let before = loaded
        .document
        .pointer(request.pointer)
        .with_context(|| format!("config pointer `{}` not found", request.pointer))?
        .clone();
    let (after, parsed_value_kind) =
        parse_config_set_value(request.raw_value, request.force_string);
    *loaded
        .document
        .pointer_mut(request.pointer)
        .with_context(|| format!("config pointer `{}` not found", request.pointer))? =
        after.clone();

    validate_config_json_value(&loaded.document, "validate updated config")?;

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

    Ok(ConfigSetOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        pointer: request.pointer.to_owned(),
        raw_value: request.raw_value.to_owned(),
        force_string: request.force_string,
        parsed_value_kind,
        before,
        after,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn config_set_outcome_json(outcome: &ConfigSetOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path,
        "source": outcome.source,
        "pointer": outcome.pointer,
        "raw_value": outcome.raw_value,
        "force_string": outcome.force_string,
        "parsed_value_kind": outcome.parsed_value_kind,
        "before": outcome.before,
        "after": outcome.after,
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
    })
}

fn print_config_set_outcome_text(outcome: &ConfigSetOutcome) {
    let preview = format!("Would update config `{}`.", outcome.pointer);
    let applied = format!("Updated config `{}`.", outcome.pointer);
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn ensure_json_pointer(pointer: &str) -> anyhow::Result<()> {
    if pointer.is_empty() || pointer.starts_with('/') {
        Ok(())
    } else {
        anyhow::bail!("config pointer `{pointer}` is not a JSON pointer; use /section/key")
    }
}

fn parse_config_set_value(
    raw_value: &str,
    force_string: bool,
) -> (serde_json::Value, &'static str) {
    if force_string {
        return (serde_json::Value::String(raw_value.to_owned()), "string");
    }
    match serde_json::from_str::<serde_json::Value>(raw_value) {
        Ok(value) => {
            let kind = match &value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object",
            };
            (value, kind)
        }
        Err(_) => (serde_json::Value::String(raw_value.to_owned()), "string"),
    }
}

fn print_config_value(value: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(value) = value.as_str() {
        println!("{value}");
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) struct ConfigEditRequest<'a> {
    pub(crate) config_path: Option<&'a PathBuf>,
    pub(crate) editor: Option<&'a str>,
    pub(crate) dry_run: bool,
    pub(crate) json_output: bool,
}

struct ConfigEditPlan {
    config_path: PathBuf,
    source: &'static str,
    editor_argv: Vec<String>,
    backup_path: Option<PathBuf>,
    existed: bool,
    dry_run: bool,
}

struct ConfigEditOutcome {
    plan: ConfigEditPlan,
    temp_path: Option<PathBuf>,
    changed: bool,
    wrote_config: bool,
    exit_status: Option<i32>,
}

pub(crate) fn handle_config_edit(request: ConfigEditRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_config_edit(request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&config_edit_outcome_json(&outcome))?
        );
    } else {
        print_config_edit_outcome_text(&outcome);
    }
    Ok(())
}

fn run_config_edit(request: ConfigEditRequest<'_>) -> anyhow::Result<ConfigEditOutcome> {
    let default_path = default_config_path()?;
    let target_path = request
        .config_path
        .cloned()
        .unwrap_or_else(|| default_path.clone());
    let loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string_pretty(&loaded.document).context("serialize editable config")?;
    let contents = format!("{contents}\n");
    let editor_argv = resolve_config_editor(request.editor)?;
    let existed = target_path.exists();
    let backup_path = existed.then(|| config_backup_path(&target_path));
    let plan = ConfigEditPlan {
        config_path: target_path.clone(),
        source: loaded.source,
        editor_argv,
        backup_path,
        existed,
        dry_run: request.dry_run,
    };

    if request.dry_run {
        return Ok(ConfigEditOutcome {
            plan,
            temp_path: None,
            changed: false,
            wrote_config: false,
            exit_status: None,
        });
    }

    let temp_path = config_edit_temp_path(&target_path);
    write_config_edit_temp_file(&temp_path, &contents)?;
    let status = run_config_editor(&plan.editor_argv, &temp_path)?;
    let exit_status = status.code();
    if !status.success() {
        let _ = fs::remove_file(&temp_path);
        anyhow::bail!(
            "config editor `{}` exited with status {}",
            plan.editor_argv.join(" "),
            exit_status.map_or_else(|| "signal".to_owned(), |code| code.to_string())
        );
    }

    let edited_contents = fs::read_to_string(&temp_path)
        .with_context(|| format!("read edited config `{}`", temp_path.display()))?;
    let edited_document = serde_json::from_str::<serde_json::Value>(&edited_contents)
        .with_context(|| format!("parse edited config `{}` as JSON", temp_path.display()))?;
    validate_config_json_value(&edited_document, "validate edited config")?;
    let normalized = format!(
        "{}\n",
        serde_json::to_string_pretty(&edited_document).context("serialize edited config")?
    );
    let changed = normalized != contents || !target_path.exists();
    if changed {
        if let Some(backup_path) = &plan.backup_path {
            let current = fs::read_to_string(&target_path)
                .with_context(|| format!("read config `{}` for backup", target_path.display()))?;
            write_private_file_atomically(backup_path, &current).with_context(|| {
                format!(
                    "backup config `{}` to `{}`",
                    target_path.display(),
                    backup_path.display()
                )
            })?;
        }
        if let Some(parent) = target_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("create config directory `{}`", parent.display()))?;
        }
        write_private_file_atomically(&target_path, &normalized)
            .with_context(|| format!("write edited config `{}`", target_path.display()))?;
    }
    fs::remove_file(&temp_path)
        .with_context(|| format!("remove temporary edit file `{}`", temp_path.display()))?;

    Ok(ConfigEditOutcome {
        plan,
        temp_path: Some(temp_path),
        changed,
        wrote_config: changed,
        exit_status,
    })
}

fn config_edit_outcome_json(outcome: &ConfigEditOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.plan.dry_run,
        "config_path": outcome.plan.config_path,
        "source": outcome.plan.source,
        "existed": outcome.plan.existed,
        "editor": outcome.plan.editor_argv.join(" "),
        "editor_argv": outcome.plan.editor_argv,
        "backup_path": outcome.plan.backup_path,
        "temp_path": outcome.temp_path,
        "changed": outcome.changed,
        "will_write_config": !outcome.plan.dry_run,
        "wrote_config": outcome.wrote_config,
        "exit_status": outcome.exit_status,
    })
}

fn print_config_edit_outcome_text(outcome: &ConfigEditOutcome) {
    if outcome.plan.dry_run {
        println!(
            "Would open config for editing: {}",
            outcome.plan.config_path.display()
        );
    } else if outcome.changed {
        println!("Updated config: {}", outcome.plan.config_path.display());
        if let Some(path) = &outcome.plan.backup_path {
            println!("Backup: {}", path.display());
        }
    } else {
        println!("No config changes.");
    }
}

fn resolve_config_editor(editor: Option<&str>) -> anyhow::Result<Vec<String>> {
    let editor = editor
        .map(str::to_owned)
        .or_else(|| std::env::var("VINPST_CONFIG_EDITOR").ok())
        .or_else(|| std::env::var("EDITOR").ok())
        .or_else(|| std::env::var("VISUAL").ok())
        .with_context(
            || "config edit requires --editor or $VINPST_CONFIG_EDITOR/$EDITOR/$VISUAL",
        )?;
    let argv = split_editor_argv(&editor);
    if argv.is_empty() {
        anyhow::bail!("config editor command is empty");
    }
    Ok(argv)
}

fn run_config_editor(
    editor_argv: &[String],
    path: &Path,
) -> anyhow::Result<std::process::ExitStatus> {
    let (program, args) = editor_argv
        .split_first()
        .with_context(|| "config editor command is empty")?;
    ProcessCommand::new(program)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| format!("run config editor `{}`", editor_argv.join(" ")))
}

fn config_edit_temp_path(target_path: &Path) -> PathBuf {
    let mut path = std::env::temp_dir();
    let target_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    path.push(format!(
        "vinpst-config-edit-{}-{}-{target_name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    path
}

fn write_config_edit_temp_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create temporary edit directory `{}`", parent.display()))?;
    }
    write_private_file_atomically(path, contents)
        .with_context(|| format!("write private temporary edit file `{}`", path.display()))
}

pub(crate) fn validate_config_file(path: &PathBuf, _summary_only: bool) -> anyhow::Result<()> {
    let input =
        fs::read_to_string(path).with_context(|| format!("read config `{}`", path.display()))?;
    let config = VinpstConfig::from_json_str(&input)
        .with_context(|| format!("parse config `{}`", path.display()))?;
    config
        .validate()
        .with_context(|| format!("validate config `{}`", path.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&config_summary_json(&config))?
    );
    Ok(())
}

pub(crate) fn handle_config_example(
    kind: Option<ConfigExample>,
    list: bool,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    if list {
        return list_config_examples();
    }
    let kind = kind.context("config example kind is required unless --list is set")?;
    export_config_example(kind, output)
}

fn list_config_examples() -> anyhow::Result<()> {
    let examples = ConfigExample::value_variants()
        .iter()
        .map(|kind| {
            serde_json::json!({
                "name": kind.to_possible_value().expect("config example has clap value").get_name(),
                "description": config_example_description(*kind),
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"examples": examples}))?
    );
    Ok(())
}

fn export_config_example(kind: ConfigExample, output: Option<&Path>) -> anyhow::Result<()> {
    let contents = config_example_contents(kind);
    let config = VinpstConfig::from_json_str(contents).context("parse bundled example config")?;
    config
        .validate()
        .context("validate bundled example config before export")?;

    if let Some(output) = output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create config example directory `{}`", parent.display())
            })?;
        }
        fs::write(output, contents)
            .with_context(|| format!("write config example `{}`", output.display()))?;
    } else {
        print!("{contents}");
    }
    Ok(())
}
