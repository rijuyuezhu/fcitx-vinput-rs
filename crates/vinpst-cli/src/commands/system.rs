use crate::{
    AsrBackendFactory, AsrTimeoutProbe, ConfigExample, Context, Path, PathBuf, ServiceStatus,
    SherpaOnnxVadProbe, VinpstConfig, audio_devices_json, config_example_contents,
    config_summary_json, daemon_owner_probe_plan_json, dbus, default_cache_root,
    default_config_path, default_model_root, fs, load_config_json, quote_exec_arg, sandbox,
    user_activation_service_path, user_data_home, user_home, write_private_file_atomically,
};

pub(crate) fn print_protocol() -> anyhow::Result<()> {
    let value = serde_json::json!({
        "service_bus_name": dbus::SERVICE_BUS_NAME,
        "service_object_path": dbus::SERVICE_OBJECT_PATH,
        "service_interface": dbus::SERVICE_INTERFACE,
        "frontend_notifier_object_path": dbus::FRONTEND_NOTIFIER_OBJECT_PATH,
        "frontend_notifier_interface": dbus::FRONTEND_NOTIFIER_INTERFACE,
        "frontend_notifier_method": dbus::method::NOTIFY,
        "operation_failed_error": dbus::error::OPERATION_FAILED,
        "error_info_signature": dbus::signature::ERROR_INFO,
        "methods": dbus::SERVICE_METHODS,
        "legacy_methods": dbus::LEGACY_SERVICE_METHODS,
        "diagnostic_extension_methods": dbus::DIAGNOSTIC_EXTENSION_METHODS,
        "signals": dbus::SERVICE_SIGNALS,
        "statuses": ServiceStatus::WIRE_VALUES,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) struct InitRequest<'a> {
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) model_root: Option<&'a Path>,
    pub(crate) cache_root: Option<&'a Path>,
    pub(crate) force: bool,
    pub(crate) dry_run: bool,
    pub(crate) json_output: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct InitOutcome {
    dry_run: bool,
    force: bool,
    config_path: PathBuf,
    config_existed: bool,
    wrote_config: bool,
    model_root: PathBuf,
    model_root_existed: bool,
    created_model_root: bool,
    cache_root: PathBuf,
    cache_root_existed: bool,
    created_cache_root: bool,
    activation_service_path: Option<PathBuf>,
    activation_command_argv: Vec<String>,
}

pub(crate) fn handle_init(request: InitRequest<'_>) -> anyhow::Result<()> {
    let json_output = request.json_output;
    let outcome = run_init(&request)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&init_outcome_json(&outcome))?
        );
    } else {
        print_init_outcome_text(&outcome);
    }
    Ok(())
}

fn run_init(request: &InitRequest<'_>) -> anyhow::Result<InitOutcome> {
    let config_path = match request.config_path {
        Some(path) => path.to_path_buf(),
        None => default_config_path()?,
    };
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let cache_root = match request.cache_root {
        Some(path) => path.to_path_buf(),
        None => default_cache_root()?,
    };
    let activation_service_path = user_activation_service_path().ok();
    let activation_command_argv = init_activation_command_argv(&config_path);

    let config_existed = config_path.exists();
    let model_root_existed = model_root.exists();
    let cache_root_existed = cache_root.exists();
    let mut wrote_config = false;
    let mut created_model_root = false;
    let mut created_cache_root = false;

    let bundled_config = VinpstConfig::bundled_default().context("parse bundled init config")?;
    bundled_config
        .validate()
        .context("validate bundled init config")?;

    if !request.dry_run {
        if !config_existed || request.force {
            if let Some(parent) = config_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create config directory `{}`", parent.display()))?;
            }
            let contents = config_example_contents(ConfigExample::Default);
            write_private_file_atomically(&config_path, contents)
                .with_context(|| format!("write default config `{}`", config_path.display()))?;
            wrote_config = true;
        }
        if !model_root_existed {
            fs::create_dir_all(&model_root)
                .with_context(|| format!("create model root `{}`", model_root.display()))?;
            created_model_root = true;
        }
        if !cache_root_existed {
            fs::create_dir_all(&cache_root)
                .with_context(|| format!("create cache root `{}`", cache_root.display()))?;
            created_cache_root = true;
        }
    }

    Ok(InitOutcome {
        dry_run: request.dry_run,
        force: request.force,
        config_path,
        config_existed,
        wrote_config,
        model_root,
        model_root_existed,
        created_model_root,
        cache_root,
        cache_root_existed,
        created_cache_root,
        activation_service_path,
        activation_command_argv,
    })
}

fn init_outcome_json(outcome: &InitOutcome) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": outcome.dry_run,
        "force": outcome.force,
        "config": {
            "path": outcome.config_path,
            "existed": outcome.config_existed,
            "will_write": outcome.dry_run && (!outcome.config_existed || outcome.force),
            "wrote": outcome.wrote_config,
        },
        "directories": {
            "model_root": {
                "path": outcome.model_root,
                "existed": outcome.model_root_existed,
                "will_create": outcome.dry_run && !outcome.model_root_existed,
                "created": outcome.created_model_root,
            },
            "cache_root": {
                "path": outcome.cache_root,
                "existed": outcome.cache_root_existed,
                "will_create": outcome.dry_run && !outcome.cache_root_existed,
                "created": outcome.created_cache_root,
            },
        },
        "activation_service": {
            "user_service_path": outcome.activation_service_path,
            "command": outcome.activation_command_argv.join(" "),
            "command_argv": outcome.activation_command_argv,
        },
        "next_steps": [
            "browse available models with vinpst model list --available",
            "install a model with vinpst model install <id-or-short-id>",
            "select it with vinpst model use <id-or-short-id> --in-place --reload-daemon",
            "check setup with vinpst doctor"
        ],
    })
}

fn print_init_outcome_text(outcome: &InitOutcome) {
    if outcome.dry_run {
        println!("Initialization preview");
    } else {
        println!("Vinpst is initialized.");
    }
    println!();
    println!(
        "Config: {} ({})",
        outcome.config_path.display(),
        init_config_state(outcome)
    );
    println!(
        "Models: {} ({})",
        outcome.model_root.display(),
        init_directory_state(
            outcome.dry_run,
            outcome.model_root_existed,
            outcome.created_model_root
        )
    );
    println!(
        "Cache:  {} ({})",
        outcome.cache_root.display(),
        init_directory_state(
            outcome.dry_run,
            outcome.cache_root_existed,
            outcome.created_cache_root
        )
    );
    if outcome.dry_run {
        println!();
        println!("No files were changed.");
        return;
    }
    println!();
    println!("Next:");
    println!("  1. Browse models:  vinpst model list --available");
    println!("  2. Install a model: vinpst model install <id-or-short-id>");
    println!("  3. Select it:       vinpst model use <id-or-short-id> --in-place --reload-daemon");
    println!("  4. Check setup:     vinpst doctor");
}

fn init_config_state(outcome: &InitOutcome) -> &'static str {
    if outcome.dry_run {
        if outcome.config_existed && outcome.force {
            "would replace"
        } else if outcome.config_existed {
            "kept"
        } else {
            "would create"
        }
    } else if outcome.wrote_config {
        if outcome.config_existed {
            "replaced"
        } else {
            "created"
        }
    } else {
        "kept"
    }
}

const fn init_directory_state(dry_run: bool, existed: bool, created: bool) -> &'static str {
    if dry_run {
        if existed { "ready" } else { "would create" }
    } else if created {
        "created"
    } else {
        "ready"
    }
}

fn init_activation_command_argv(config_path: &Path) -> Vec<String> {
    vec![
        "vinpst".to_owned(),
        "activation-service".to_owned(),
        "--daemon".to_owned(),
        default_daemon_path_hint().to_string_lossy().into_owned(),
        "--config".to_owned(),
        config_path.to_string_lossy().into_owned(),
        "--configured-backends".to_owned(),
        "--user".to_owned(),
    ]
}

fn default_daemon_path_hint() -> PathBuf {
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(parent) = current_exe.parent()
    {
        let sibling = parent.join("vinpst-daemon");
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("vinpst-daemon")
}

pub(crate) fn validate_config() -> anyhow::Result<()> {
    let config = VinpstConfig::bundled_default().context("parse bundled config")?;
    config.validate().context("validate bundled config")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&config_summary_json(&config))?
    );
    Ok(())
}

pub(crate) fn print_asr_state(config_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let (_, config) = load_diagnostic_config(config_path, "ASR state")?;
    let state = AsrBackendFactory::state_for_config(&config.asr);
    println!("{}", serde_json::to_string_pretty(&state)?);
    Ok(())
}

pub(crate) fn print_audio_devices(config_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let (_, config) = load_diagnostic_config(config_path, "audio device diagnostics")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&audio_devices_json(&config)?)?
    );
    Ok(())
}

pub(crate) fn print_doctor(config_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let (resolved_config_path, config) = load_diagnostic_config(config_path, "doctor")?;
    let asr_state = AsrBackendFactory::state_for_config(&config.asr);
    let vad = SherpaOnnxVadProbe::inspect(&config.asr.vad);
    let timeout = AsrTimeoutProbe::inspect(&config.asr);
    let audio = audio_devices_json(&config)?;
    let activation_service = match user_activation_service_path() {
        Ok(path) => user_activation_service_json(&path),
        Err(error) => serde_json::json!({
            "user_service_path": null,
            "user_service_exists": false,
            "user_service_exec": null,
            "read_error": null,
            "path_error": format!("{error:#}"),
            "next_steps": activation_service_status_next_steps(),
        }),
    };
    let sandbox_report = sandbox::permission_report_json();
    let ready = asr_state.has_effective_backend;
    let status = if ready { "ready" } else { "setup-required" };
    let target_model_is_empty = asr_state.target_model_id.is_empty();
    let summary = serde_json::json!({
        "ok": ready,
        "status": status,
        "config_path": resolved_config_path.map(|path| path.to_string_lossy().into_owned()),
        "config": config_summary_json(&config),
        "asr": asr_state,
        "vad": doctor_vad_json(&vad),
        "asr_timeout": timeout,
        "audio": audio,
        "activation_service": activation_service,
        "sandbox": sandbox_report,
        "fcitx_addon": user_fcitx_addon_json(),
        "daemon_owner_probe": daemon_owner_probe_plan_json(),
        "next_steps": doctor_next_steps(
            &config,
            ready,
            target_model_is_empty,
            &vad,
            &timeout,
        ),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn load_diagnostic_config(
    config_path: Option<&PathBuf>,
    purpose: &str,
) -> anyhow::Result<(Option<PathBuf>, VinpstConfig)> {
    let loaded = load_config_json(config_path)?;
    let contents = serde_json::to_string(&loaded.document)
        .with_context(|| format!("serialize config for {purpose}"))?;
    let config = VinpstConfig::from_json_str(&contents)
        .with_context(|| format!("parse config for {purpose}"))?;
    config
        .validate()
        .with_context(|| format!("validate config for {purpose}"))?;
    Ok((loaded.path, config))
}

fn doctor_vad_json(probe: &SherpaOnnxVadProbe) -> serde_json::Value {
    let status = if !probe.enabled {
        "disabled"
    } else if probe.available {
        "ready"
    } else {
        "missing"
    };
    serde_json::json!({
        "status": status,
        "enabled": probe.enabled,
        "available": probe.available,
        "model": probe.model,
        "requested_model": probe.requested_model,
        "source": probe.source,
        "scope": "offline-sherpa-only",
        "threshold": probe.threshold,
        "min_speech_duration": probe.min_speech_duration,
        "min_silence_duration": probe.min_silence_duration,
        "speech_pad_ms": probe.speech_pad_ms,
    })
}

fn doctor_next_steps(
    config: &VinpstConfig,
    asr_ready: bool,
    target_model_is_empty: bool,
    vad: &SherpaOnnxVadProbe,
    timeout: &AsrTimeoutProbe,
) -> Vec<String> {
    let mut next_steps = Vec::new();
    if !asr_ready {
        next_steps.push(
            "run vinpst asr-state --json to inspect why the active ASR backend is unavailable"
                .to_owned(),
        );
        if target_model_is_empty {
            next_steps.extend([
                "run vinpst model list --available to choose a compatible managed model".to_owned(),
                "run vinpst model install <id-or-short-id> to install the selected model"
                    .to_owned(),
                "run vinpst model use <id-or-short-id> --in-place --reload-daemon to activate it"
                    .to_owned(),
            ]);
        }
    }
    next_steps.extend([
        "run vinpst provider list to inspect configured ASR providers".to_owned(),
        format!(
            "run vinpst provider use {} --dry-run --json to preview provider selection",
            config.asr.active_provider
        ),
        "run vinpst hotword get --json to inspect hotword configuration".to_owned(),
        "run vinpst device list --json to inspect capture devices".to_owned(),
        "run vinpst device use <target> --dry-run --json to preview capture-device selection"
            .to_owned(),
        "run vinpst daemon status --dry-run --json to inspect daemon D-Bus owner/procfs probes"
            .to_owned(),
    ]);
    if vad.enabled && !vad.available {
        next_steps.push(
            "install silero_vad.onnx under $XDG_DATA_HOME/fcitx-vinpst/vad or set VINPST_SHERPA_VAD_MODEL"
                .to_owned(),
        );
    }
    if timeout.enforcement == vinpst_asr::AsrTimeoutEnforcement::Unsupported {
        next_steps.push(
            "native timeout_ms is diagnostic-only; remove it or use a cancellable command ASR provider"
                .to_owned(),
        );
    }
    next_steps
}

fn user_fcitx_addon_json() -> serde_json::Value {
    match user_fcitx_addon_paths() {
        Ok((module_path, metadata_path)) => fcitx_addon_status_json(&module_path, &metadata_path),
        Err(error) => serde_json::json!({
            "user_module_path": null,
            "user_module_exists": false,
            "user_addon_metadata_path": null,
            "user_addon_metadata_exists": false,
            "user_addon_library": null,
            "user_addon_library_matches": false,
            "user_addon_type": null,
            "read_error": null,
            "path_error": format!("{error:#}"),
        }),
    }
}

fn user_fcitx_addon_paths() -> anyhow::Result<(PathBuf, PathBuf)> {
    let lib_dir = match std::env::var_os("VINPST_USER_FCITX_LIB_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => user_home()?.join(".local/lib/fcitx5"),
    };
    let metadata_dir = match std::env::var_os("VINPST_USER_FCITX_ADDON_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => user_data_home()?.join("fcitx5").join("addon"),
    };
    Ok((
        lib_dir.join("fcitx5-vinpst.so"),
        metadata_dir.join("vinpst.conf"),
    ))
}

fn fcitx_addon_status_json(module_path: &Path, metadata_path: &Path) -> serde_json::Value {
    let module_exists = module_path.exists();
    if !metadata_path.exists() {
        return serde_json::json!({
            "user_module_path": module_path,
            "user_module_exists": module_exists,
            "user_addon_metadata_path": metadata_path,
            "user_addon_metadata_exists": false,
            "user_addon_library": null,
            "user_addon_library_matches": false,
            "user_addon_type": null,
            "read_error": null,
        });
    }

    match fs::read_to_string(metadata_path) {
        Ok(contents) => {
            let library = activation_service_field(&contents, "Library");
            serde_json::json!({
                "user_module_path": module_path,
                "user_module_exists": module_exists,
                "user_addon_metadata_path": metadata_path,
                "user_addon_metadata_exists": true,
                "user_addon_library": library,
                "user_addon_library_matches": library.as_deref() == Some("fcitx5-vinpst"),
                "user_addon_type": activation_service_field(&contents, "Type"),
                "read_error": null,
            })
        }
        Err(error) => serde_json::json!({
            "user_module_path": module_path,
            "user_module_exists": module_exists,
            "user_addon_metadata_path": metadata_path,
            "user_addon_metadata_exists": true,
            "user_addon_library": null,
            "user_addon_library_matches": false,
            "user_addon_type": null,
            "read_error": error.to_string(),
        }),
    }
}

fn user_activation_service_json(path: &Path) -> serde_json::Value {
    let exists = path.exists();
    if !exists {
        return serde_json::json!({
            "user_service_path": path,
            "user_service_exists": false,
            "user_service_name": null,
            "user_service_name_matches": false,
            "user_service_exec": null,
            "read_error": null,
            "next_steps": activation_service_status_next_steps(),
        });
    }

    match fs::read_to_string(path) {
        Ok(contents) => {
            let name = activation_service_field(&contents, "Name");
            serde_json::json!({
                "user_service_path": path,
                "user_service_exists": true,
                "user_service_name": name,
                "user_service_name_matches": name.as_deref() == Some(dbus::SERVICE_BUS_NAME),
                "user_service_exec": activation_service_field(&contents, "Exec"),
                "read_error": null,
                "next_steps": activation_service_status_next_steps(),
            })
        }
        Err(error) => serde_json::json!({
            "user_service_path": path,
            "user_service_exists": true,
            "user_service_name": null,
            "user_service_name_matches": false,
            "user_service_exec": null,
            "read_error": error.to_string(),
            "next_steps": activation_service_status_next_steps(),
        }),
    }
}

fn activation_service_status_next_steps() -> Vec<&'static str> {
    vec![
        "run vinpst daemon start --dry-run --json to inspect activation strategy",
        "run vinpst daemon status --dry-run --json to inspect daemon owner/procfs probes",
        "run vinpst doctor to inspect activation, addon, audio, and config diagnostics",
    ]
}

fn activation_service_field(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(ToOwned::to_owned))
}

pub(crate) fn write_activation_service(
    daemon: &Path,
    config: Option<&Path>,
    configured_backends: bool,
    audio_backend: Option<&str>,
    daemon_args: &[String],
    user: bool,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    let mut args = vec!["--dbus".to_owned()];
    if configured_backends {
        args.push("--configured-backends".to_owned());
    }
    if let Some(config) = config {
        args.push("--config".to_owned());
        args.push(config.to_string_lossy().into_owned());
    }
    if let Some(audio_backend) = audio_backend {
        args.push("--audio-backend".to_owned());
        args.push(audio_backend.to_owned());
    }
    args.extend(daemon_args.iter().cloned());
    args.push("--exit-when-executable-replaced".to_owned());

    let mut exec_parts = Vec::with_capacity(args.len() + 1);
    exec_parts.push(quote_exec_arg(&daemon.to_string_lossy()));
    exec_parts.extend(args.iter().map(|arg| quote_exec_arg(arg)));

    let service = format!(
        "[D-BUS Service]\nName={}\nExec={}\n",
        dbus::SERVICE_BUS_NAME,
        exec_parts.join(" ")
    );

    let user_output;
    let output = if user {
        user_output = user_activation_service_path()?;
        Some(user_output.as_path())
    } else {
        output
    };

    if let Some(output) = output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).with_context(|| {
                format!("create activation service directory `{}`", parent.display())
            })?;
        }
        fs::write(output, service)
            .with_context(|| format!("write activation service `{}`", output.display()))?;
    } else {
        print!("{service}");
    }
    Ok(())
}

pub(crate) fn print_user_activation_service_status() -> anyhow::Result<()> {
    let path = user_activation_service_path()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&user_activation_service_json(&path))?
    );
    Ok(())
}

pub(crate) fn remove_user_activation_service() -> anyhow::Result<()> {
    let path = user_activation_service_path()?;
    let existed = path.exists();
    if existed {
        fs::remove_file(&path)
            .with_context(|| format!("remove activation service `{}`", path.display()))?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "removed": existed,
            "user_service_path": path,
            "next_steps": activation_service_status_next_steps(),
        }))?
    );
    Ok(())
}
