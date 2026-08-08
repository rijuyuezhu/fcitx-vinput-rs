mod catalog;
mod edit;
mod mutation;

use catalog::{print_available_provider_list, print_provider_install, print_provider_list};
use edit::{print_provider_edit, print_provider_edit_script};
use mutation::{print_provider_add, print_provider_remove, print_provider_use};

use crate::{
    AsrProviderConfig, AsrProviderKind, BTreeMap, Context, Duration, LiveRegistryI18n,
    LiveScriptKind, LiveScriptRegistry, LoadedLiveI18n, LoadedLiveScriptRegistry, Path, PathBuf,
    ProviderCommand, RegistryConfig, ReqwestRegistryAssetSource, ReqwestRegistryTextSource,
    VinpstConfig, config_set_write_target, default_config_path, default_provider_root,
    fetch_text_from_mirrors, fs, install_live_script, live_registry_urls, load_config_json,
    load_live_i18n, managed_script_relative_path, materialize_asr_provider,
    prepare_provider_script_edit, validate_config_json_value, write_config_set_document,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn handle_provider_command(command: ProviderCommand) -> anyhow::Result<()> {
    match command {
        ProviderCommand::List {
            available,
            registry,
            i18n,
            locale,
            config,
            json,
        } => {
            if available {
                print_available_provider_list(
                    registry.as_deref(),
                    i18n.as_deref(),
                    config.as_ref(),
                    &locale,
                    json,
                )
            } else {
                print_provider_list(config.as_ref(), json)
            }
        }
        ProviderCommand::Install {
            id,
            registry,
            provider_root,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_install(ProviderInstallRequest {
            selector: &id,
            registry_path: registry.as_deref(),
            provider_root: provider_root.as_deref(),
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Use {
            id,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_use(ProviderUseRequest {
            id: &id,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Add {
            id,
            kind,
            model,
            hotwords_file,
            command,
            args,
            env,
            endpoint,
            timeout_ms,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_add(ProviderAddRequest {
            id: &id,
            kind: &kind,
            model: model.as_deref(),
            hotwords_file: hotwords_file.as_deref(),
            command: command.as_deref(),
            args: &args,
            env: &env,
            endpoint: endpoint.as_deref(),
            timeout_ms,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Edit {
            id,
            kind,
            model,
            clear_model,
            hotwords_file,
            clear_hotwords_file,
            command,
            clear_command,
            args,
            clear_args,
            env,
            clear_env,
            endpoint,
            clear_endpoint,
            timeout_ms,
            clear_timeout,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_edit(ProviderEditRequest {
            id: &id,
            kind: kind.as_deref(),
            model: model.as_deref(),
            clear_model,
            hotwords_file: hotwords_file.as_deref(),
            clear_hotwords_file,
            command: command.as_deref(),
            clear_command,
            args: &args,
            clear_args,
            env: &env,
            clear_env,
            endpoint: endpoint.as_deref(),
            clear_endpoint,
            timeout_ms,
            clear_timeout,
            config_path: config.as_ref(),
            output_path: output.as_deref(),
            in_place,
            dry_run,
            json_output: json,
        }),
        ProviderCommand::EditScript {
            id,
            registry,
            config,
            editor,
            dry_run,
            json,
        } => print_provider_edit_script(ProviderEditScriptRequest {
            selector: &id,
            registry_path: registry.as_deref(),
            config_path: config.as_ref(),
            editor: editor.as_deref(),
            dry_run,
            json_output: json,
        }),
        ProviderCommand::Remove {
            id,
            registry,
            config,
            output,
            in_place,
            dry_run,
            json,
        } => print_provider_remove(ProviderRemoveRequest {
            id: &id,
            registry_path: registry.as_deref(),
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
struct ProviderAddRequest<'a> {
    id: &'a str,
    kind: &'a str,
    model: Option<&'a str>,
    hotwords_file: Option<&'a str>,
    command: Option<&'a str>,
    args: &'a [String],
    env: &'a [String],
    endpoint: Option<&'a str>,
    timeout_ms: Option<u64>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderAddOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    provider_type: &'static str,
    active_provider: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderInstallRequest<'a> {
    selector: &'a str,
    registry_path: Option<&'a Path>,
    provider_root: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct ProviderInstallOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    registry_source: serde_json::Value,
    provider_id: String,
    short_id: Option<String>,
    streaming: bool,
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

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderEditRequest<'a> {
    id: &'a str,
    kind: Option<&'a str>,
    model: Option<&'a str>,
    clear_model: bool,
    hotwords_file: Option<&'a str>,
    clear_hotwords_file: bool,
    command: Option<&'a str>,
    clear_command: bool,
    args: &'a [String],
    clear_args: bool,
    env: &'a [String],
    clear_env: bool,
    endpoint: Option<&'a str>,
    clear_endpoint: bool,
    timeout_ms: Option<u64>,
    clear_timeout: bool,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderEditOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    provider_id: String,
    before_provider_type: &'static str,
    after_provider_type: &'static str,
    active_provider: String,
    changed_fields: Vec<String>,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
struct ProviderEditScriptRequest<'a> {
    selector: &'a str,
    registry_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    editor: Option<&'a str>,
    dry_run: bool,
    json_output: bool,
}

struct ProviderEditScriptOutcome {
    selector: String,
    provider_id: String,
    config_path: Option<PathBuf>,
    source: &'static str,
    registry_source: Option<serde_json::Value>,
    script_path: PathBuf,
    editor_argv: Vec<String>,
    dry_run: bool,
    edited: bool,
    exit_status: Option<i32>,
}
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderRemoveRequest<'a> {
    id: &'a str,
    registry_path: Option<&'a Path>,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderRemoveOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    removed_provider_id: String,
    removed_provider_type: &'static str,
    active_provider: String,
    before_provider_count: usize,
    after_provider_count: usize,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderUseRequest<'a> {
    id: &'a str,
    config_path: Option<&'a PathBuf>,
    output_path: Option<&'a Path>,
    in_place: bool,
    dry_run: bool,
    json_output: bool,
}

struct ProviderUseOutcome {
    config_path: Option<PathBuf>,
    source: &'static str,
    before: String,
    after: String,
    provider_type: &'static str,
    output_path: Option<PathBuf>,
    backup_path: Option<PathBuf>,
    in_place: bool,
    dry_run: bool,
    wrote_config: bool,
}

struct ProviderListContext {
    config_path: Option<PathBuf>,
    source: &'static str,
    config: VinpstConfig,
}

struct InstalledProviderResolution {
    selector: String,
    provider: AsrProviderConfig,
    config_path: Option<PathBuf>,
    source: &'static str,
    registry_source: Option<serde_json::Value>,
}

pub(crate) fn normalize_provider_id(input: &str) -> anyhow::Result<String> {
    mutation::normalize_provider_id(input)
}

pub(crate) fn asr_provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    mutation::asr_provider_kind_label(kind)
}
