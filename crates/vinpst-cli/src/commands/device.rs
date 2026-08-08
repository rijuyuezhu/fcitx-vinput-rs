use crate::{
    CaptureTarget, Context, DeviceCommand, Path, PathBuf, VinpstConfig, audio_devices_json,
    capture_target_json, config_set_write_target, default_config_path, load_config_json,
    validate_config_json_value, write_config_set_document,
};

pub(crate) fn handle_device_command(command: DeviceCommand) -> anyhow::Result<()> {
    match command {
        DeviceCommand::List { config, json } => print_device_list(config.as_ref(), json),
        DeviceCommand::Use {
            target,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_device_use(DeviceUseRequest {
            target: &target,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct DeviceUseRequest<'a> {
    target: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct DeviceListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinpstConfig,
}

struct DeviceUseOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    before: String,
    after: String,
    capture_target: CaptureTarget,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

fn print_device_list(config_path: Option<&PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let context = load_device_list_context(config_path)?;
    let audio = audio_devices_json(&context.config)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "config_path": context.config_path.as_ref(),
                "source": context.source,
                "audio": audio,
            }))?
        );
    } else {
        print_device_list_text(context.config_path.as_ref(), context.source, &audio);
    }
    Ok(())
}

fn load_device_list_context(config_path: Option<&PathBuf>) -> anyhow::Result<DeviceListContext> {
    let loaded = load_config_json(config_path)?;
    let document = loaded.document;
    let contents = serde_json::to_string(&document).context("serialize config for device list")?;
    let config = VinpstConfig::from_json_str(&contents).context("parse config for device list")?;
    config
        .validate()
        .context("validate config for device list")?;
    Ok(DeviceListContext {
        config_path: loaded.path,
        source: loaded.source,
        config,
    })
}

fn print_device_list_text(
    _config_path: Option<&PathBuf>,
    _source: &str,
    audio: &serde_json::Value,
) {
    let selected = audio["capture_device"].as_str().unwrap_or("default");
    if let Some(error) = audio["enumeration_error"].as_str() {
        println!("Live device discovery unavailable: {error}");
    }
    println!("TARGET\tID\tNAME\tDESCRIPTION\tSTATUS");
    println!(
        "default\t-\tdefault\tDefault capture source\t{}",
        if selected == "default" { "active" } else { "" }
    );
    if let Some(devices) = audio["devices"].as_array() {
        for device in devices {
            let id = device["id"]
                .as_u64()
                .map_or_else(|| "-".to_owned(), |id| id.to_string());
            let name = device["name"].as_str().unwrap_or("");
            let description = device["description"].as_str().unwrap_or("");
            println!(
                "{name}\t{id}\t{name}\t{description}\t{}",
                if selected == name { "active" } else { "" }
            );
        }
    }
}

fn print_device_use(request: DeviceUseRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_device_use(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&device_use_outcome_json(&outcome))?
        );
    } else {
        print_device_use_text(&outcome);
    }
    Ok(())
}

fn run_device_use(request: &DeviceUseRequest<'_>) -> anyhow::Result<DeviceUseOutcome> {
    let after = normalize_capture_device_value(request.target)?;
    let capture_target = CaptureTarget::from_config_value(&after)
        .with_context(|| format!("parse capture device `{}`", request.target))?;
    let default_path = default_config_path()?;
    let mut loaded = load_config_json(request.config_path)?;
    let contents =
        serde_json::to_string(&loaded.document).context("serialize config for device selection")?;
    let before = VinpstConfig::from_json_str(&contents)
        .context("parse config for device selection")?
        .global
        .capture_device;
    let root = loaded
        .document
        .as_object_mut()
        .context("device config root must be an object")?;
    let global = root
        .entry("global")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("device config field `global` must be an object")?;
    global.insert(
        "capture_device".to_owned(),
        serde_json::Value::String(after.clone()),
    );
    validate_config_json_value(&loaded.document, "validate updated device config")?;

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

    Ok(DeviceUseOutcome {
        config_path: loaded.path.take(),
        source: loaded.source,
        before,
        after,
        capture_target,
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        dry_run: request.dry_run,
        wrote_config,
    })
}

fn normalize_capture_device_value(input: &str) -> anyhow::Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("capture device cannot be empty");
    }
    Ok(trimmed.to_owned())
}

fn device_use_outcome_json(outcome: &DeviceUseOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "config_path": outcome.config_path,
        "source": outcome.source,
        "before": outcome.before,
        "after": outcome.after,
        "capture_target": capture_target_json(&outcome.capture_target),
        "output_path": outcome.output_path,
        "backup_path": outcome.backup_path,
        "in_place": outcome.in_place,
        "will_write_config": !outcome.dry_run,
        "wrote_config": outcome.wrote_config,
        "next_steps": [
            "run vinpst device list to verify the configured capture target",
            "run vinpst doctor to inspect audio and config diagnostics"
        ],
    })
}

fn print_device_use_text(outcome: &DeviceUseOutcome) {
    let target = capture_target_label(&outcome.capture_target);
    let preview = format!("Would select capture device `{target}`.");
    let applied = format!("Selected capture device `{target}`.");
    crate::human_output::print_config_mutation(
        outcome.dry_run,
        &preview,
        &applied,
        outcome.output_path.as_deref(),
        outcome.backup_path.as_deref(),
    );
}

fn capture_target_label(target: &CaptureTarget) -> String {
    match target {
        CaptureTarget::Default => "default".to_owned(),
        CaptureTarget::Object(value) => format!("object:{value}"),
    }
}
