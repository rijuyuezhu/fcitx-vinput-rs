//! Managed provider and adapter installation transactions for the GUI.

#[cfg(test)]
mod removal_tests;

use std::{env, path::PathBuf, time::Duration};

use vinpst_config::{AsrProviderConfig, AsrProviderKind, LlmAdapterConfig, VinpstConfig};
use vinpst_registry::{
    LiveScriptKind, LiveScriptRegistry, RegistryOperationControl, RegistryOperationProgress,
    RegistryTextSource, ReqwestRegistryAssetSource, ReqwestRegistryTextSource,
    install_live_script_controlled, managed_script_relative_path, managed_script_rollback_path,
    materialize_asr_provider, materialize_llm_adapter,
};

use crate::{
    ConfigDocument, ConfigSaveOutcome, GuiLocale, GuiText, ensure_config_mutation_allowed,
    save_updated_config_with_daemon,
    script_install::{
        ScriptEnvironmentValue, ScriptInstallOutcome, ScriptInstallPlan, ScriptPrepareOutcome,
    },
    script_transaction::{
        ManagedScriptRollback, apply_managed_script_revision, failed_after_publication,
        failed_with_script_restore,
    },
};

pub(crate) fn prepare_registry_script_controlled(
    document: &ConfigDocument,
    kind: LiveScriptKind,
    selector: &str,
    control: &RegistryOperationControl,
) -> ScriptPrepareOutcome {
    let root = match default_script_root(kind) {
        Ok(root) => root,
        Err(error) => return ScriptPrepareOutcome::Failed(error),
    };
    let registry_source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    prepare_registry_script_from_source(document, kind, selector, control, &registry_source, &root)
}

fn prepare_registry_script_from_source(
    document: &ConfigDocument,
    kind: LiveScriptKind,
    selector: &str,
    control: &RegistryOperationControl,
    registry_source: &impl RegistryTextSource,
    root: &std::path::Path,
) -> ScriptPrepareOutcome {
    control.report(RegistryOperationProgress::ResolvingRegistry);
    if control.is_cancelled() {
        return ScriptPrepareOutcome::Cancelled;
    }
    if let Err(error) = ensure_config_mutation_allowed(document) {
        return ScriptPrepareOutcome::Failed(error);
    }
    let registry =
        match fetch_live_script_registry_from(&document.config, kind, control, registry_source) {
            Ok(registry) => registry,
            Err(_) if control.is_cancelled() => return ScriptPrepareOutcome::Cancelled,
            Err(error) => return ScriptPrepareOutcome::Failed(error),
        };
    let Some(entry) = registry.entry_by_id_or_short_id(selector, kind).cloned() else {
        return ScriptPrepareOutcome::Failed(format!(
            "Unknown {} registry id or short id `{selector}`.",
            resource_label(kind)
        ));
    };
    let script_path = match managed_script_relative_path(kind, &entry.id) {
        Ok(path) => root.join(path),
        Err(error) => return ScriptPrepareOutcome::Failed(error.to_string()),
    };
    let (prepared_config, _) =
        match materialize_config(&document.config, kind, &entry, &script_path) {
            Ok(value) => value,
            Err(error) => return ScriptPrepareOutcome::Failed(error),
        };
    if let Err(error) = prepared_config.validate() {
        return ScriptPrepareOutcome::Failed(format!(
            "Validate prepared {} configuration: {error}",
            resource_label(kind)
        ));
    }
    let environment = entry
        .envs
        .iter()
        .map(|spec| ScriptEnvironmentValue {
            name: spec.name.clone(),
            required: spec.required,
            value: prepared_environment_value(&prepared_config, kind, &entry.id, &spec.name)
                .unwrap_or_default(),
        })
        .collect();
    ScriptPrepareOutcome::Prepared(Box::new(ScriptInstallPlan {
        kind,
        selector: selector.to_owned(),
        entry,
        script_root: root.to_path_buf(),
        script_path,
        environment,
    }))
}

pub(crate) fn install_registry_script_controlled(
    document: &ConfigDocument,
    plan: &ScriptInstallPlan,
    control: &RegistryOperationControl,
    locale: GuiLocale,
) -> ScriptInstallOutcome {
    let asset_source = ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(120));
    install_registry_script_from_source(document, plan, control, &asset_source, locale)
}

fn install_registry_script_from_source(
    document: &ConfigDocument,
    plan: &ScriptInstallPlan,
    control: &RegistryOperationControl,
    asset_source: &impl vinpst_registry::RegistryAssetSource,
    locale: GuiLocale,
) -> ScriptInstallOutcome {
    install_registry_script_from_source_and_save(
        document,
        plan,
        control,
        asset_source,
        locale,
        save_updated_config_with_daemon,
    )
}

pub(crate) fn install_registry_script_from_source_and_save(
    document: &ConfigDocument,
    plan: &ScriptInstallPlan,
    control: &RegistryOperationControl,
    asset_source: &impl vinpst_registry::RegistryAssetSource,
    locale: GuiLocale,
    save: impl FnOnce(&ConfigDocument, &VinpstConfig) -> Result<ConfigSaveOutcome, String>,
) -> ScriptInstallOutcome {
    control.report(RegistryOperationProgress::Preparing);
    if control.is_cancelled() {
        return ScriptInstallOutcome::Cancelled;
    }
    if let Err(error) = ensure_config_mutation_allowed(document) {
        return ScriptInstallOutcome::Failed(error);
    }
    if let Err(error) = validate_plan_environment(plan) {
        return ScriptInstallOutcome::Failed(error);
    }
    let (mut updated, replacing) =
        match materialize_config(&document.config, plan.kind, &plan.entry, &plan.script_path) {
            Ok(value) => value,
            Err(error) => return ScriptInstallOutcome::Failed(error),
        };
    apply_plan_environment(&mut updated, plan);
    let rollback = match ManagedScriptRollback::prepare(plan.kind, replacing, &plan.script_path) {
        Ok(rollback) => rollback,
        Err(error) => return ScriptInstallOutcome::Failed(error),
    };
    if let Err(error) = updated.validate() {
        return ScriptInstallOutcome::Failed(format!(
            "Validate installed {} configuration: {error}",
            resource_label(plan.kind)
        ));
    }
    if control.is_cancelled() {
        return ScriptInstallOutcome::Cancelled;
    }

    let installed = match install_live_script_controlled(
        asset_source,
        plan.kind,
        &plan.entry,
        &plan.script_root,
        control,
    ) {
        Ok(installed) => installed,
        Err(_) if control.is_cancelled() => return ScriptInstallOutcome::Cancelled,
        Err(error) => {
            let primary = format!("{} installation failed: {error}", resource_title(plan.kind));
            return failed_with_script_restore(primary, rollback.as_ref());
        }
    };
    if installed.script_path != plan.script_path {
        return failed_with_script_restore(
            format!(
                "Installed script path `{}` did not match planned path `{}`.",
                installed.script_path.display(),
                plan.script_path.display()
            ),
            rollback.as_ref(),
        );
    }
    if let Err(error) = apply_managed_script_revision(
        &mut updated,
        plan.kind,
        &plan.entry.id,
        &installed.script_path,
        rollback.as_ref(),
    ) {
        return failed_after_publication(error, rollback.as_ref());
    }

    control.report(RegistryOperationProgress::UpdatingConfiguration);
    let saved = match save(document, &updated) {
        Ok(saved) => saved,
        Err(error) => return failed_after_publication(error, rollback.as_ref()),
    };
    control.report(RegistryOperationProgress::Completed);
    let resource = locale.text(match plan.kind {
        LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
        LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
    });
    ScriptInstallOutcome::Installed(locale.script_installed(
        replacing,
        resource,
        &plan.entry.id,
        &plan.script_path.display().to_string(),
        &saved.daemon_reload,
    ))
}

pub(crate) fn validate_plan_environment(plan: &ScriptInstallPlan) -> Result<(), String> {
    if plan.environment.len() != plan.entry.envs.len()
        || !plan
            .environment
            .iter()
            .zip(&plan.entry.envs)
            .all(|(value, spec)| value.name == spec.name && value.required == spec.required)
    {
        return Err(format!(
            "Prepared {} environment no longer matches registry metadata; resolve the catalog again.",
            resource_label(plan.kind)
        ));
    }
    if let Some(name) = plan.missing_required_environment() {
        return Err(format!(
            "Required environment variable `{name}` must have a value before installing {}.",
            resource_label(plan.kind)
        ));
    }
    Ok(())
}

fn prepared_environment_value(
    config: &VinpstConfig,
    kind: LiveScriptKind,
    id: &str,
    name: &str,
) -> Option<String> {
    match kind {
        LiveScriptKind::AsrProvider => config
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .and_then(|provider| provider.env.get(name))
            .cloned(),
        LiveScriptKind::LlmAdapter => config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == id)
            .and_then(|adapter| adapter.env.get(name))
            .cloned(),
    }
}

pub(crate) fn apply_plan_environment(config: &mut VinpstConfig, plan: &ScriptInstallPlan) {
    let environment = match plan.kind {
        LiveScriptKind::AsrProvider => config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.id == plan.entry.id)
            .map(|provider| &mut provider.env),
        LiveScriptKind::LlmAdapter => config
            .llm
            .adapters
            .iter_mut()
            .find(|adapter| adapter.id == plan.entry.id)
            .map(|adapter| &mut adapter.env),
    };
    if let Some(environment) = environment {
        for value in &plan.environment {
            environment.insert(value.name.clone(), value.value.clone());
        }
    }
}

fn fetch_live_script_registry_from(
    config: &VinpstConfig,
    kind: LiveScriptKind,
    control: &RegistryOperationControl,
    source: &impl RegistryTextSource,
) -> Result<LiveScriptRegistry, String> {
    fetch_live_script_registry_with_base_from(config, kind, control, source)
        .map(|(registry, _)| registry)
}

pub(crate) fn fetch_live_script_registry_with_base_from(
    config: &VinpstConfig,
    kind: LiveScriptKind,
    control: &RegistryOperationControl,
    source: &impl RegistryTextSource,
) -> Result<(LiveScriptRegistry, String), String> {
    let filename = match kind {
        LiveScriptKind::AsrProvider => "providers.json",
        LiveScriptKind::LlmAdapter => "adapters.json",
    };
    let urls = config
        .registry
        .base_urls
        .iter()
        .map(|base| format!("{}/registry/{filename}", base.trim_end_matches('/')))
        .collect::<Vec<_>>();
    if urls.is_empty() {
        return Err("No registry mirrors are configured.".to_owned());
    }
    let mut failure_count = 0;
    for url in &urls {
        if control.is_cancelled() {
            return Err("Registry request cancelled.".to_owned());
        }
        match source.fetch_registry_text(url) {
            Ok(text) => {
                let registry = LiveScriptRegistry::from_json_str(&text, kind).map_err(|error| {
                    format!(
                        "{} registry catalog is invalid: {error}",
                        resource_title(kind)
                    )
                })?;
                let base = url
                    .strip_suffix(&format!("/registry/{filename}"))
                    .unwrap_or(url)
                    .to_owned();
                return Ok((registry, base));
            }
            Err(_) => failure_count += 1,
        }
    }
    Err(format!(
        "All {failure_count} configured {} registry mirrors failed.",
        resource_label(kind)
    ))
}

pub(crate) fn materialize_config(
    config: &VinpstConfig,
    kind: LiveScriptKind,
    entry: &vinpst_registry::LiveScriptEntry,
    script_path: &std::path::Path,
) -> Result<(VinpstConfig, bool), String> {
    let mut updated = config.clone();
    match kind {
        LiveScriptKind::AsrProvider => {
            let existing = updated
                .asr
                .providers
                .iter()
                .find(|provider| provider.id == entry.id);
            let materialized = materialize_asr_provider(entry, script_path, existing)
                .map_err(|error| error.to_string())?;
            let replacing = materialized.replacing_managed;
            if let Some(index) = updated
                .asr
                .providers
                .iter()
                .position(|provider| provider.id == entry.id)
            {
                updated.asr.providers[index] = materialized.provider;
            } else {
                updated.asr.providers.push(materialized.provider);
            }
            Ok((updated, replacing))
        }
        LiveScriptKind::LlmAdapter => {
            let existing = updated
                .llm
                .adapters
                .iter()
                .find(|adapter| adapter.id == entry.id);
            let materialized = materialize_llm_adapter(entry, script_path, existing)
                .map_err(|error| error.to_string())?;
            let replacing = materialized.replacing_managed;
            if let Some(index) = updated
                .llm
                .adapters
                .iter()
                .position(|adapter| adapter.id == entry.id)
            {
                updated.llm.adapters[index] = materialized.adapter;
            } else {
                updated.llm.adapters.push(materialized.adapter);
            }
            Ok((updated, replacing))
        }
    }
}

fn default_script_root(kind: LiveScriptKind) -> Result<PathBuf, String> {
    let data_home = match env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is required to locate managed script storage".to_owned())?
            .join(".local/share"),
    };
    Ok(data_home.join("fcitx-vinpst").join(match kind {
        LiveScriptKind::AsrProvider => "providers",
        LiveScriptKind::LlmAdapter => "adapters",
    }))
}

pub(crate) const fn resource_label(kind: LiveScriptKind) -> &'static str {
    match kind {
        LiveScriptKind::AsrProvider => "ASR provider",
        LiveScriptKind::LlmAdapter => "text adapter",
    }
}

pub(crate) fn managed_provider_script_path(provider: &AsrProviderConfig) -> Option<PathBuf> {
    (provider.kind == AsrProviderKind::Command)
        .then(|| {
            configured_managed_script_path(
                LiveScriptKind::AsrProvider,
                &provider.id,
                &provider.args,
            )
        })
        .flatten()
}

pub(crate) fn managed_adapter_script_path(adapter: &LlmAdapterConfig) -> Option<PathBuf> {
    configured_managed_script_path(LiveScriptKind::LlmAdapter, &adapter.id, &adapter.args)
}

pub(crate) fn remove_managed_script_entry(
    document: &ConfigDocument,
    kind: LiveScriptKind,
    id: &str,
    locale: GuiLocale,
) -> Result<String, String> {
    let root = default_script_root(kind)?;
    remove_managed_script_entry_from_root(document, kind, id, &root, locale)
}

fn remove_managed_script_entry_from_root(
    document: &ConfigDocument,
    kind: LiveScriptKind,
    id: &str,
    root: &std::path::Path,
    locale: GuiLocale,
) -> Result<String, String> {
    ensure_config_mutation_allowed(document)?;
    let mut updated = document.config.clone();
    let script_path = match kind {
        LiveScriptKind::AsrProvider => {
            let index = updated
                .asr
                .providers
                .iter()
                .position(|provider| provider.id == id)
                .ok_or_else(|| format!("ASR provider `{id}` is not configured."))?;
            let provider = &updated.asr.providers[index];
            if provider.id == updated.asr.active_provider {
                return Err(format!(
                    "Active ASR provider `{id}` cannot be removed; select another provider first."
                ));
            }
            let script_path = configured_managed_script_path_from_root(
                LiveScriptKind::AsrProvider,
                &provider.id,
                &provider.args,
                root,
            )
            .ok_or_else(|| {
                format!(
                    "ASR provider `{id}` is not a managed registry provider and cannot be removed from the GUI."
                )
            })?;
            inspect_removable_script(&script_path)?;
            updated.asr.providers.remove(index);
            script_path
        }
        LiveScriptKind::LlmAdapter => {
            let index = updated
                .llm
                .adapters
                .iter()
                .position(|adapter| adapter.id == id)
                .ok_or_else(|| format!("Text adapter `{id}` is not configured."))?;
            let adapter = &updated.llm.adapters[index];
            let script_path = configured_managed_script_path_from_root(
                LiveScriptKind::LlmAdapter,
                &adapter.id,
                &adapter.args,
                root,
            )
            .ok_or_else(|| {
                    format!(
                        "Text adapter `{id}` is not a managed registry adapter and cannot be removed from the GUI."
                    )
                })?;
            inspect_removable_script(&script_path)?;
            updated.llm.adapters.remove(index);
            script_path
        }
    };
    updated
        .validate()
        .map_err(|error| format!("Validate configuration after removing {id}: {error}"))?;
    let saved = save_updated_config_with_daemon(document, &updated)?;
    let cleanup = cleanup_managed_script(&script_path);
    let rollback_cleanup = (kind == LiveScriptKind::LlmAdapter)
        .then(|| cleanup_managed_script(&managed_script_rollback_path(&script_path)));
    let resource = locale.text(match kind {
        LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
        LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
    });
    let cleanup_error = cleanup.as_ref().err().map(ToString::to_string).or_else(|| {
        rollback_cleanup
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(ToString::to_string)
    });
    Ok(locale.script_removed(
        resource,
        id,
        &script_path.display().to_string(),
        cleanup_error.as_deref(),
        cleanup == Ok(true),
        &saved.daemon_reload,
    ))
}

fn configured_managed_script_path(
    kind: LiveScriptKind,
    id: &str,
    args: &[String],
) -> Option<PathBuf> {
    let root = default_script_root(kind).ok()?;
    configured_managed_script_path_from_root(kind, id, args, &root)
}

fn configured_managed_script_path_from_root(
    kind: LiveScriptKind,
    id: &str,
    args: &[String],
    root: &std::path::Path,
) -> Option<PathBuf> {
    let path = root.join(managed_script_relative_path(kind, id).ok()?);
    let expected = path.to_string_lossy();
    (args == [expected.as_ref()]).then_some(path)
}

fn inspect_removable_script(path: &std::path::Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Err(format!(
            "Refusing to remove managed script `{}` because it is a directory.",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Inspect managed script `{}`: {error}",
            path.display()
        )),
    }
}

fn cleanup_managed_script(path: &std::path::Path) -> Result<bool, String> {
    match std::fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::remove_dir(parent);
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("remove `{}`: {error}", path.display())),
    }
}

const fn resource_title(kind: LiveScriptKind) -> &'static str {
    match kind {
        LiveScriptKind::AsrProvider => "ASR provider",
        LiveScriptKind::LlmAdapter => "Text adapter",
    }
}

#[cfg(test)]
#[path = "script_management_revision_tests.rs"]
mod revision_tests;

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;
    use vinpst_config::MANAGED_SCRIPT_REVISION_KEY;
    use vinpst_registry::{LiveScriptEntry, RegistryAssetSource, sha256_hex};

    struct FixtureTextSource(&'static str);

    impl RegistryTextSource for FixtureTextSource {
        fn fetch_registry_text(&self, _url: &str) -> Result<String, String> {
            Ok(self.0.to_owned())
        }
    }

    struct FixtureAssetSource(&'static [u8]);

    impl RegistryAssetSource for FixtureAssetSource {
        fn fetch_asset(&self, _url: &str, destination: &Path) -> Result<(), String> {
            fs::write(destination, self.0).map_err(|error| error.to_string())
        }
    }

    struct UnexpectedAssetSource;

    impl RegistryAssetSource for UnexpectedAssetSource {
        fn fetch_asset(&self, _url: &str, _destination: &Path) -> Result<(), String> {
            panic!("asset download must not start before required environment validation")
        }
    }

    fn prepared_plan(outcome: ScriptPrepareOutcome) -> ScriptInstallPlan {
        match outcome {
            ScriptPrepareOutcome::Prepared(plan) => *plan,
            other => panic!("expected prepared plan, got {other:?}"),
        }
    }

    #[test]
    fn provider_materialization_adds_managed_command_entry() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let entry = LiveScriptEntry {
            id: "provider.fixture.batch".to_owned(),
            short_id: Some("fixture".to_owned()),
            stream: false,
            command: "python3".to_owned(),
            script_urls: vec!["https://example.invalid/provider.py".to_owned()],
            readme_url: None,
            envs: Vec::new(),
        };

        let (updated, replacing) = materialize_config(
            &config,
            LiveScriptKind::AsrProvider,
            &entry,
            std::path::Path::new("/tmp/provider.py"),
        )
        .expect("materialize provider");

        assert!(!replacing);
        assert!(
            updated.asr.providers.iter().any(|provider| {
                provider.id == entry.id && provider.args == ["/tmp/provider.py"]
            })
        );
    }

    #[test]
    fn adapter_materialization_adds_managed_command_entry() {
        let config = VinpstConfig::bundled_default().expect("bundled config");
        let entry = LiveScriptEntry {
            id: "adapter.fixture.command".to_owned(),
            short_id: Some("fixture".to_owned()),
            stream: false,
            command: "python3".to_owned(),
            script_urls: vec!["https://example.invalid/adapter.py".to_owned()],
            readme_url: None,
            envs: Vec::new(),
        };

        let (updated, replacing) = materialize_config(
            &config,
            LiveScriptKind::LlmAdapter,
            &entry,
            std::path::Path::new("/tmp/adapter.py"),
        )
        .expect("materialize adapter");

        assert!(!replacing);
        assert!(
            updated
                .llm
                .adapters
                .iter()
                .any(|adapter| { adapter.id == entry.id && adapter.args == ["/tmp/adapter.py"] })
        );
    }

    #[test]
    fn provider_install_publishes_script_and_validated_config() {
        let directory = tempfile::tempdir().expect("temp dir");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "provider.fixture.batch",
                    "short_id": "fixture",
                    "stream": false,
                    "command": "python3",
                    "script_urls": ["https://example.invalid/provider.py"],
                    "envs": [{"name": "TOKEN", "required": true}]
                }]
            }"#,
        );
        let asset = FixtureAssetSource(b"#!/usr/bin/env python3\nprint('ok')\n");
        let root = directory.path().join("providers");

        let mut plan = prepared_plan(prepare_registry_script_from_source(
            &document,
            LiveScriptKind::AsrProvider,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &root,
        ));
        assert_eq!(plan.missing_required_environment(), Some("TOKEN"));
        plan.environment[0].value = "super-secret".to_owned();

        let outcome = install_registry_script_from_source(
            &document,
            &plan,
            &RegistryOperationControl::default(),
            &asset,
            GuiLocale::EnUs,
        );

        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        assert!(root.join("fixture/batch").is_file());
        let config = VinpstConfig::from_json_file(&document.path).expect("saved config");
        let provider = config
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == "provider.fixture.batch")
            .expect("installed provider");
        assert_eq!(
            provider.args,
            [root.join("fixture/batch").display().to_string()]
        );
        assert_eq!(
            provider.env.get("TOKEN").map(String::as_str),
            Some("super-secret")
        );
    }

    #[test]
    fn required_environment_is_rejected_before_script_download() {
        let directory = tempfile::tempdir().expect("temp dir");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "provider.fixture.batch",
                    "short_id": "fixture",
                    "stream": false,
                    "command": "python3",
                    "script_urls": ["https://example.invalid/provider.py"],
                    "envs": [{"name": "TOKEN", "required": true}]
                }]
            }"#,
        );
        let root = directory.path().join("providers");
        let plan = prepared_plan(prepare_registry_script_from_source(
            &document,
            LiveScriptKind::AsrProvider,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &root,
        ));

        let outcome = install_registry_script_from_source(
            &document,
            &plan,
            &RegistryOperationControl::default(),
            &UnexpectedAssetSource,
            GuiLocale::EnUs,
        );

        assert!(matches!(outcome, ScriptInstallOutcome::Failed(error) if error.contains("TOKEN")));
        assert!(!document.path.exists());
        assert!(!root.exists());
    }

    #[test]
    fn preparation_preserves_existing_managed_environment_value() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("providers");
        let entry = LiveScriptEntry {
            id: "provider.fixture.batch".to_owned(),
            short_id: Some("fixture".to_owned()),
            stream: false,
            command: "python3".to_owned(),
            script_urls: vec!["https://example.invalid/provider.py".to_owned()],
            readme_url: None,
            envs: vec![vinpst_registry::LiveScriptEnvSpec {
                name: "TOKEN".to_owned(),
                required: true,
            }],
        };
        let (mut config, _) = materialize_config(
            &VinpstConfig::bundled_default().expect("bundled config"),
            LiveScriptKind::AsrProvider,
            &entry,
            &root.join("fixture/batch"),
        )
        .expect("materialize provider");
        config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.id == entry.id)
            .expect("provider")
            .env
            .insert("TOKEN".to_owned(), "existing-secret".to_owned());
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config,
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "provider.fixture.batch",
                    "short_id": "fixture",
                    "stream": false,
                    "command": "python3",
                    "script_urls": ["https://example.invalid/provider.py"],
                    "envs": [{"name": "TOKEN", "required": true}]
                }]
            }"#,
        );

        let plan = prepared_plan(prepare_registry_script_from_source(
            &document,
            LiveScriptKind::AsrProvider,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &root,
        ));

        assert_eq!(plan.environment[0].value, "existing-secret");
        assert_eq!(plan.missing_required_environment(), None);
        assert!(!format!("{plan:?}").contains("existing-secret"));
    }

    #[test]
    fn managed_update_replaces_declared_value_and_preserves_extra_environment() {
        let directory = tempfile::tempdir().expect("temp dir");
        let root = directory.path().join("providers");
        let entry = LiveScriptEntry {
            id: "provider.fixture.batch".to_owned(),
            short_id: Some("fixture".to_owned()),
            stream: false,
            command: "python3".to_owned(),
            script_urls: vec!["https://example.invalid/provider.py".to_owned()],
            readme_url: None,
            envs: vec![vinpst_registry::LiveScriptEnvSpec {
                name: "TOKEN".to_owned(),
                required: true,
            }],
        };
        let script_path = root.join("fixture/batch");
        let (mut config, _) = materialize_config(
            &VinpstConfig::bundled_default().expect("bundled config"),
            LiveScriptKind::AsrProvider,
            &entry,
            &script_path,
        )
        .expect("materialize provider");
        let provider = config
            .asr
            .providers
            .iter_mut()
            .find(|provider| provider.id == entry.id)
            .expect("provider");
        provider
            .env
            .insert("TOKEN".to_owned(), "old-secret".to_owned());
        provider
            .env
            .insert("EXTRA_FLAG".to_owned(), "keep-me".to_owned());
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config,
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "provider.fixture.batch",
                    "short_id": "fixture",
                    "stream": false,
                    "command": "python3",
                    "script_urls": ["https://example.invalid/provider.py"],
                    "envs": [{"name": "TOKEN", "required": true}]
                }]
            }"#,
        );
        let asset = FixtureAssetSource(b"#!/usr/bin/env python3\nprint('updated')\n");
        let mut plan = prepared_plan(prepare_registry_script_from_source(
            &document,
            LiveScriptKind::AsrProvider,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &root,
        ));
        plan.environment[0].value = "new-secret".to_owned();

        let outcome = install_registry_script_from_source(
            &document,
            &plan,
            &RegistryOperationControl::default(),
            &asset,
            GuiLocale::EnUs,
        );

        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        let saved = VinpstConfig::from_json_file(&document.path).expect("saved config");
        let provider = saved
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == entry.id)
            .expect("provider");
        assert_eq!(
            provider.env.get("TOKEN").map(String::as_str),
            Some("new-secret")
        );
        assert_eq!(
            provider.env.get("EXTRA_FLAG").map(String::as_str),
            Some("keep-me")
        );
    }

    #[test]
    fn adapter_install_publishes_script_and_validated_config() {
        let directory = tempfile::tempdir().expect("temp dir");
        let document = ConfigDocument {
            path: directory.path().join("config.json"),
            from_disk: false,
            config: VinpstConfig::bundled_default().expect("bundled config"),
        };
        let registry = FixtureTextSource(
            r#"{
                "version": 1,
                "items": [{
                    "id": "adapter.fixture.command",
                    "short_id": "fixture",
                    "command": "python3",
                    "script_urls": ["https://example.invalid/adapter.py"],
                    "envs": [{"name": "OPTIONAL_MODE", "required": false}]
                }]
            }"#,
        );
        let asset = FixtureAssetSource(b"#!/usr/bin/env python3\nprint('ok')\n");
        let root = directory.path().join("adapters");

        let plan = prepared_plan(prepare_registry_script_from_source(
            &document,
            LiveScriptKind::LlmAdapter,
            "fixture",
            &RegistryOperationControl::default(),
            &registry,
            &root,
        ));
        assert_eq!(plan.environment[0].value, "");

        let outcome = install_registry_script_from_source(
            &document,
            &plan,
            &RegistryOperationControl::default(),
            &asset,
            GuiLocale::EnUs,
        );

        assert!(matches!(outcome, ScriptInstallOutcome::Installed(_)));
        assert!(root.join("fixture/command").is_file());
        let config = VinpstConfig::from_json_file(&document.path).expect("saved config");
        let adapter = config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == "adapter.fixture.command")
            .expect("installed adapter");
        assert_eq!(
            adapter.args,
            [root.join("fixture/command").display().to_string()]
        );
        assert_eq!(
            adapter.env.get("OPTIONAL_MODE").map(String::as_str),
            Some("")
        );
        assert_eq!(
            adapter
                .extra
                .get(MANAGED_SCRIPT_REVISION_KEY)
                .and_then(serde_json::Value::as_str),
            Some(sha256_hex(b"#!/usr/bin/env python3\nprint('ok')\n").as_str())
        );
    }
}
