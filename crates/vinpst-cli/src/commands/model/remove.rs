use super::{
    AsrProviderKind, Context, ModelRemovePlan, ModelRemoveRequest, ModelRemoveResolution, Path,
    PathBuf, VinpstConfig, default_model_root, fs, load_config_file, same_path_text,
};
use super::{
    catalog::load_live_model_catalog,
    support::{managed_model_dir_name, safe_path_component},
};
use vinpst_registry::{
    ManagedModelRemoveRequest, remove_managed_model, validate_managed_model_target,
};

pub(super) fn print_model_remove_plan(request: ModelRemoveRequest<'_>) -> anyhow::Result<()> {
    if request.dry_run && request.yes {
        anyhow::bail!("model remove cannot combine --dry-run and --yes");
    }
    if !request.dry_run && !request.yes {
        anyhow::bail!(
            "model remove requires --yes to delete; rerun with --dry-run to inspect the removal plan"
        );
    }
    let model_root = match request.model_root {
        Some(path) => path.to_path_buf(),
        None => default_model_root()?,
    };
    let mut plan = build_model_remove_plan(request, &model_root)?;
    if request.yes {
        remove_managed_model_dir(&plan, request.config_path)?;
        plan.removed = true;
    }
    if request.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&model_remove_plan_json(&plan))?
        );
    } else {
        print_model_remove_plan_text(&plan);
    }
    Ok(())
}

fn build_model_remove_plan(
    request: ModelRemoveRequest<'_>,
    model_root: &Path,
) -> anyhow::Result<ModelRemovePlan> {
    let resolution = resolve_model_remove_target(
        request.selector,
        request.installed,
        request.registry_path,
        request.i18n_path,
        request.config_path,
        request.locale,
        model_root,
    )?;
    validate_managed_model_target(model_root, &resolution.target_path)?;
    let metadata = fs::metadata(&resolution.target_path);
    let (exists, is_dir) = match metadata {
        Ok(metadata) => (true, metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (false, false),
        Err(error) => {
            anyhow::bail!(
                "inspect model remove target `{}`: {}",
                resolution.target_path.display(),
                error.kind()
            );
        }
    };
    Ok(ModelRemovePlan {
        selector: request.selector.to_owned(),
        selector_kind: resolution.selector_kind,
        model_root: model_root.to_path_buf(),
        target_path: resolution.target_path,
        exists,
        is_dir,
        resolved_model_id: resolution.resolved_model_id,
        resolved_short_id: resolution.resolved_short_id,
        resolved_title: resolution.resolved_title,
        removed: false,
    })
}

fn remove_managed_model_dir(
    plan: &ModelRemovePlan,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    if !plan.exists {
        anyhow::bail!(
            "model remove target `{}` does not exist",
            plan.target_path.display()
        );
    }
    if !plan.is_dir {
        anyhow::bail!(
            "model remove target `{}` is not a directory",
            plan.target_path.display()
        );
    }
    ensure_model_not_active(&plan.target_path, config_path)?;
    remove_managed_model(&ManagedModelRemoveRequest {
        model_root: &plan.model_root,
        target_path: &plan.target_path,
        active_model_paths: &[],
    })?;
    Ok(())
}

fn ensure_model_not_active(
    target_path: &Path,
    config_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    let config = match config_path {
        Some(config_path) => load_config_file(config_path)?,
        None => VinpstConfig::bundled_default().context("parse bundled config")?,
    };
    for provider in &config.asr.providers {
        if provider.kind == AsrProviderKind::Local
            && let Some(model) = &provider.model
            && same_path_text(Path::new(model), target_path)
        {
            anyhow::bail!(
                "refusing to remove active model `{}` used by ASR provider `{}`",
                target_path.display(),
                provider.id
            );
        }
    }
    Ok(())
}

fn resolve_model_remove_target(
    selector: &str,
    installed: bool,
    registry_path: Option<&Path>,
    i18n_path: Option<&Path>,
    config_path: Option<&PathBuf>,
    locale: &str,
    model_root: &Path,
) -> anyhow::Result<ModelRemoveResolution> {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() || selector.contains('/') {
        if installed {
            anyhow::bail!(
                "model remove --installed expects a managed model directory name, not a path"
            );
        }
        return Ok(ModelRemoveResolution {
            target_path: selector_path.to_path_buf(),
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
        return Ok(ModelRemoveResolution {
            target_path: model_root.join(managed_model_dir_name(model)),
            selector_kind: "registry".to_owned(),
            resolved_model_id: Some(model.id.clone()),
            resolved_short_id: model.short_id.clone(),
            resolved_title: Some(model.resolved_title(i18n.i18n.as_ref())),
        });
    }

    Ok(ModelRemoveResolution {
        target_path: model_root.join(safe_path_component(selector)),
        selector_kind: "managed-dir".to_owned(),
        resolved_model_id: None,
        resolved_short_id: None,
        resolved_title: None,
    })
}

fn model_remove_plan_json(plan: &ModelRemovePlan) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "dry_run": !plan.removed,
        "will_remove": plan.removed,
        "removed": plan.removed,
        "selector": {
            "input": plan.selector,
            "kind": plan.selector_kind,
            "resolved_model_id": plan.resolved_model_id,
            "resolved_short_id": plan.resolved_short_id,
            "title": plan.resolved_title,
        },
        "target": {
            "model_root": plan.model_root,
            "path": plan.target_path,
            "exists": plan.exists && !plan.removed,
            "is_dir": plan.is_dir,
            "managed": true,
        },
        "next_steps": [
            "run vinpst model use --dry-run to verify the active config does not point at the removed model",
            "restart or reload the daemon after removing an inactive model"
        ],
    })
}

fn print_model_remove_plan_text(plan: &ModelRemovePlan) {
    let display_name = plan
        .resolved_title
        .as_deref()
        .or(plan.resolved_short_id.as_deref())
        .or(plan.resolved_model_id.as_deref())
        .unwrap_or(&plan.selector);
    if plan.removed {
        println!("Removed model `{display_name}`.");
    } else {
        println!("Would remove model `{display_name}`.");
    }
    println!("Location: {}", plan.target_path.display());
}
