mod catalog;
mod lifecycle;
mod mutation;

use catalog::{print_adapter_install, print_adapter_install_plan, print_adapter_list};
use lifecycle::{print_adapter_lifecycle, print_adapter_status};
use mutation::{print_adapter_add, print_adapter_edit, print_adapter_remove};

use crate::{
    AdapterCommand, BTreeMap, Context, Duration, LiveRegistryI18n, LiveScriptKind,
    LiveScriptRegistry, LoadedLiveI18n, LoadedLiveScriptRegistry, Path, PathBuf, RegistryConfig,
    RegistryIndex, ReqwestRegistryAssetSource, ReqwestRegistryTextSource, TextAdapterState,
    VinpstConfig, config_set_write_target, daemon_owner_probe_plan_json, daemon_service_proxy,
    dbus, default_adapter_root, default_config_path, fetch_text_from_mirrors, fs,
    install_live_script, live_registry_urls, load_config_file, load_config_json, load_live_i18n,
    managed_script_relative_path, materialize_llm_adapter, validate_config_json_value,
    write_config_set_document,
};

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterAddRequest<'a> {
    id: &'a str,
    command: &'a str,
    args: &'a [String],
    env: &'a [String],
    working_dir: Option<&'a str>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterEditRequest<'a> {
    id: &'a str,
    command: Option<&'a str>,
    args: &'a [String],
    clear_args: bool,
    env: &'a [String],
    clear_env: bool,
    working_dir: Option<&'a str>,
    clear_working_dir: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct AdapterEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    adapter_id: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterRemoveRequest<'a> {
    id: &'a str,
    registry_path: Option<&'a Path>,
    adapter_root: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct AdapterAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    adapter_id: String,
    before_adapter_count: usize,
    after_adapter_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct AdapterInstallRequest<'a> {
    selector: &'a str,
    registry_path: Option<&'a Path>,
    adapter_root: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct AdapterInstallOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    registry_source: serde_json::Value,
    adapter_id: String,
    short_id: Option<String>,
    script_path: PathBuf,
    required_env: Vec<String>,
    optional_env: Vec<String>,
    replacing_managed: bool,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_script: bool,
    wrote_config: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct AdapterRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    registry_source: Option<serde_json::Value>,
    removed_adapter_id: String,
    managed_script: bool,
    script_path: Option<PathBuf>,
    script_existed: bool,
    removed_script: bool,
    before_adapter_count: usize,
    after_adapter_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct AdapterListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinpstConfig,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn handle_adapter_command(command: AdapterCommand) -> anyhow::Result<()> {
    match command {
        AdapterCommand::List {
            available,
            registry,
            i18n,
            locale,
            config,
            json,
        } => print_adapter_list(
            config.as_ref(),
            available,
            registry.as_deref(),
            i18n.as_deref(),
            &locale,
            json,
        ),
        AdapterCommand::Add {
            id,
            command,
            args,
            env,
            working_dir,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_add(AdapterAddRequest {
            id: &id,
            command: &command,
            args: &args,
            env: &env,
            working_dir: working_dir.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        AdapterCommand::Install {
            id,
            registry,
            adapter_root,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_install(AdapterInstallRequest {
            selector: &id,
            registry_path: registry.as_deref(),
            adapter_root: adapter_root.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        AdapterCommand::InstallPlan {
            id,
            registry,
            target_root,
            config,
            summary_only,
            json,
        } => print_adapter_install_plan(
            &id,
            &registry,
            &target_root,
            config.as_ref(),
            summary_only,
            json,
        ),
        AdapterCommand::Start {
            id,
            registry,
            config,
            dry_run,
            json,
        } => print_adapter_lifecycle(
            "start",
            &id,
            dbus::method::START_ADAPTER,
            registry.as_deref(),
            config.as_ref(),
            dry_run,
            json,
        ),
        AdapterCommand::Stop {
            id,
            registry,
            config,
            dry_run,
            json,
        } => print_adapter_lifecycle(
            "stop",
            &id,
            dbus::method::STOP_ADAPTER,
            registry.as_deref(),
            config.as_ref(),
            dry_run,
            json,
        ),
        AdapterCommand::Status {
            id,
            registry,
            config,
            dry_run,
            json,
        } => print_adapter_status(
            id.as_deref(),
            registry.as_deref(),
            config.as_ref(),
            dry_run,
            json,
        ),
        AdapterCommand::Edit {
            id,
            command,
            args,
            clear_args,
            env,
            clear_env,
            working_dir,
            clear_working_dir,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_edit(AdapterEditRequest {
            id: &id,
            command: command.as_deref(),
            args: &args,
            clear_args,
            env: &env,
            clear_env,
            working_dir: working_dir.as_deref(),
            clear_working_dir,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        AdapterCommand::Remove {
            id,
            registry,
            adapter_root,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_adapter_remove(AdapterRemoveRequest {
            id: &id,
            registry_path: registry.as_deref(),
            adapter_root: adapter_root.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
    }
}

struct InstalledAdapterResolution {
    selector: String,
    adapter_id: String,
    config_path: Option<PathBuf>,
    config_source: &'static str,
    registry_source: Option<serde_json::Value>,
}
