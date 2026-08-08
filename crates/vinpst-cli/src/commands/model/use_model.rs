use super::{
    AsrProviderKind, Context, ModelUsePreview, ModelUseRequest, ModelUseResolution,
    ModelUseWriteTarget, Path, PathBuf, VinpstConfig, config_backup_path, dbus, default_model_root,
    load_config_file, reload_asr_backend_via_dbus, same_path_text, write_config_in_place,
    write_config_output,
};
use super::{
    catalog::load_live_model_catalog,
    support::{managed_model_dir_name, safe_path_component},
};

fn model_use_write_target(request: &ModelUseRequest<'_>) -> anyhow::Result<ModelUseWriteTarget> {
    if request.output_path.is_some() && request.in_place {
        anyhow::bail!("model use cannot combine --output and --in-place");
    }
    if request.dry_run {
        return Ok(ModelUseWriteTarget::DryRun);
    }
    if request.in_place {
        let config_path = request
            .config_path
            .with_context(|| "model use --in-place requires --config <path>")?;
        return Ok(ModelUseWriteTarget::InPlace {
            config_path: config_path.clone(),
            backup_path: config_backup_path(config_path),
        });
    }
    let output_path = request.output_path.with_context(|| {
        "model use writes require --output <path> or --in-place; rerun with --dry-run to inspect the config patch"
    })?;
    if let Some(config_path) = request.config_path
        && same_path_text(config_path, output_path)
    {
        anyhow::bail!(
            "refusing to overwrite input config `{}` with --output; use --in-place to create a backup",
            config_path.display()
        );
    }
    Ok(ModelUseWriteTarget::Output(output_path.to_path_buf()))
}

pub(super) fn print_model_use_preview(request: ModelUseRequest<'_>) -> anyhow::Result<()> {
    let write_target = model_use_write_target(&request)?;

    let mut config = match request.config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinpstConfig::bundled_default().context("parse bundled config")?,
    };
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let resolution = resolve_model_use_value(
        request.selector,
        request.installed,
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
        &model_root,
    )?;
    let provider_id = request
        .provider
        .map_or_else(|| config.asr.active_provider.clone(), str::to_owned);
    let provider_index = config
        .asr
        .providers
        .iter()
        .position(|provider| provider.id == provider_id)
        .with_context(|| format!("ASR provider `{provider_id}` not found in config"))?;
    let provider = &config.asr.providers[provider_index];
    if provider.kind != AsrProviderKind::Local {
        anyhow::bail!("ASR provider `{provider_id}` is not local and cannot use a managed model");
    }

    let mut preview = ModelUsePreview {
        config_path: request.config_path.cloned(),
        provider_id: provider_id.clone(),
        provider_kind: provider.kind.clone(),
        output_path: write_target.output_path(),
        backup_path: write_target.backup_path(),
        in_place: write_target.in_place(),
        reload_daemon: request.reload_daemon,
        reloaded_daemon: false,
        wrote_config: false,
        before_active_provider: config.asr.active_provider.clone(),
        before_model: provider.model.clone(),
        after_active_provider: provider_id,
        after_model: resolution.model_value,
        selector_kind: resolution.selector_kind,
        selector: request.selector.to_owned(),
        resolved_model_id: resolution.resolved_model_id,
        resolved_short_id: resolution.resolved_short_id,
        resolved_title: resolution.resolved_title,
    };

    if !request.dry_run {
        config
            .asr
            .active_provider
            .clone_from(&preview.after_active_provider);
        config.asr.providers[provider_index].model = Some(preview.after_model.clone());
        config.validate().context("validate updated config")?;
        write_model_use_config(&config, &write_target)?;
        preview.wrote_config = true;
        if request.reload_daemon {
            reload_asr_backend_via_dbus().context("model use demon update")?;
            preview.reloaded_daemon = true;
        }
    }

    if request.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&model_use_preview_json(&preview))?
        );
    } else {
        print_model_use_preview_text(&preview);
    }
    Ok(())
}

fn resolve_model_use_value(
    selector: &str,
    installed: bool,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    model_root: &Path,
) -> anyhow::Result<ModelUseResolution> {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() || selector.contains('/') {
        if installed {
            anyhow::bail!(
                "model use --installed expects a managed model directory name, not a path"
            );
        }
        return Ok(ModelUseResolution {
            model_value: selector_path.to_string_lossy().into_owned(),
            selector_kind: "path".to_owned(),
            resolved_model_id: None,
            resolved_short_id: None,
            resolved_title: None,
        });
    }

    if !installed
        && let Ok((loaded, i18n)) =
            load_live_model_catalog(registry_path, i18n_path, config_path, locale)
        && let Some(model) = loaded.registry.model_by_id_or_short_id(selector)
    {
        return Ok(ModelUseResolution {
            model_value: model_root
                .join(managed_model_dir_name(model))
                .to_string_lossy()
                .into_owned(),
            selector_kind: "registry".to_owned(),
            resolved_model_id: Some(model.id.clone()),
            resolved_short_id: model.short_id.clone(),
            resolved_title: Some(model.resolved_title(i18n.i18n.as_ref())),
        });
    }

    Ok(ModelUseResolution {
        model_value: model_root
            .join(safe_path_component(selector))
            .to_string_lossy()
            .into_owned(),
        selector_kind: "managed-dir".to_owned(),
        resolved_model_id: None,
        resolved_short_id: None,
        resolved_title: None,
    })
}

fn model_use_preview_json(preview: &ModelUsePreview) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": !preview.wrote_config,
        "will_write_config": preview.wrote_config,
        "wrote_config": preview.wrote_config,
        "config_path": preview.config_path,
        "output_path": preview.output_path,
        "backup_path": preview.backup_path,
        "in_place": preview.in_place,
        "reload_daemon": {
            "requested": preview.reload_daemon,
            "will_call_dbus": preview.reload_daemon && !preview.reloaded_daemon,
            "called": preview.reloaded_daemon,
            "dbus": {
                "service": dbus::SERVICE_BUS_NAME,
                "object_path": dbus::SERVICE_OBJECT_PATH,
                "interface": dbus::SERVICE_INTERFACE,
                "method": dbus::method::RELOAD_ASR_BACKEND,
            },
        },
        "selector": {
            "input": preview.selector,
            "kind": preview.selector_kind,
            "resolved_model_id": preview.resolved_model_id,
            "resolved_short_id": preview.resolved_short_id,
            "title": preview.resolved_title,
        },
        "patch": {
            "asr.active_provider": {
                "before": preview.before_active_provider,
                "after": preview.after_active_provider,
            },
            "asr.providers[].model": {
                "provider_id": preview.provider_id,
                "provider_type": format!("{:?}", preview.provider_kind).to_lowercase(),
                "before": preview.before_model,
                "after": preview.after_model,
            }
        },
        "next_steps": [
            "use the written config with vinpst asr-state --config <path>",
            "restart or reload the daemon with the updated config"
        ],
    })
}

fn print_model_use_preview_text(preview: &ModelUsePreview) {
    let display_name = preview
        .resolved_title
        .as_deref()
        .or(preview.resolved_short_id.as_deref())
        .or(preview.resolved_model_id.as_deref())
        .unwrap_or(&preview.selector);
    let preview_message = format!(
        "Would select model `{display_name}` for ASR provider `{}`.",
        preview.provider_id
    );
    let applied_message = format!(
        "Selected model `{display_name}` for ASR provider `{}`.",
        preview.provider_id
    );
    crate::human_output::print_config_mutation(
        !preview.wrote_config,
        &preview_message,
        &applied_message,
        preview.output_path.as_deref(),
        preview.backup_path.as_deref(),
    );
    if preview.reloaded_daemon {
        println!("Reloaded the ASR backend.");
    }
}

fn write_model_use_config(
    config: &VinpstConfig,
    target: &ModelUseWriteTarget,
) -> anyhow::Result<()> {
    match target {
        ModelUseWriteTarget::DryRun => Ok(()),
        ModelUseWriteTarget::Output(output_path) => write_config_output(config, output_path),
        ModelUseWriteTarget::InPlace {
            config_path,
            backup_path,
        } => write_config_in_place(config, config_path, backup_path),
    }
}
