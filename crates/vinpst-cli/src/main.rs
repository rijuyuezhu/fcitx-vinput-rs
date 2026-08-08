//! `vinpst` command-line prototype.

mod audio_diagnostics;
mod cli;
mod commands;
mod config_examples;
mod config_io;
mod daemon_control;
mod hotword;
mod human_output;
mod live_i18n;
mod paths;
mod recording_control;
mod registry_support;
mod sandbox;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::Duration,
};

use anyhow::Context;
use audio_diagnostics::{audio_devices_json, capture_target_json};
use clap::ValueEnum;
use cli::{
    AdapterCommand, Command, ConfigCommand, ConfigExample, DaemonCommand, DeviceCommand,
    LlmCommand, ModelCommand, ProviderCommand, RecordingCommand, RegistryCommand, SceneCommand,
    force_json_output, parse_args_with_global_json_alias,
};
use commands::{
    ConfigEditRequest, ConfigSetRequest, InitRequest, asr_provider_kind_label,
    handle_adapter_command, handle_config_edit, handle_config_example, handle_config_get,
    handle_config_set, handle_device_command, handle_init, handle_llm_command,
    handle_model_command, handle_provider_command, handle_scene_command, normalize_provider_id,
    print_asr_state, print_audio_devices, print_doctor, print_protocol,
    print_registry_install_plan, print_registry_plan, print_registry_summary,
    print_user_activation_service_status, remove_user_activation_service, validate_config,
    validate_config_file, validate_registry_index, write_activation_service,
};
use config_examples::{config_example_contents, config_example_description};
use config_io::{
    LoadedConfigJson, config_backup_path, config_set_write_target, config_summary_json,
    load_config_file, load_config_json, same_path_text, split_editor_argv,
    validate_config_json_value, write_config_in_place, write_config_output,
    write_config_set_document, write_file_atomically, write_private_file_atomically,
};
use daemon_control::{
    daemon_owner_probe_plan_json, daemon_service_proxy, handle_daemon_command,
    reload_asr_backend_via_dbus,
};
use hotword::handle_hotword_command;
use live_i18n::{LoadedLiveI18n, load_live_i18n};
use paths::{
    default_adapter_root, default_cache_root, default_config_path,
    default_model_install_staging_root, default_model_root, default_provider_root, quote_exec_arg,
    user_activation_service_path, user_data_home, user_home,
};
use recording_control::handle_recording_command;
use registry_support::{LoadedLiveScriptRegistry, fetch_text_from_mirrors, live_registry_urls};
use vinpst_asr::{AsrBackendFactory, AsrTimeoutProbe, SherpaOnnxVadProbe};
use vinpst_audio::CaptureTarget;
use vinpst_config::{
    AsrProviderConfig, AsrProviderKind, RegistryConfig, SceneDefinition, VinpstConfig,
};
use vinpst_protocol::{RecognitionPayload, ServiceStatus, TextAdapterState, dbus};
use vinpst_registry::{
    ArchiveFormat, AssetEntry, AssetPlanSummary, InstalledModelInfo, LiveModelEntry,
    LiveModelFamily, LiveModelInstallRequest, LiveModelInstallResult, LiveModelRegistry,
    LiveRegistryI18n, LiveScriptKind, LiveScriptRegistry, PlannedAsset, RegistryIndex,
    ReqwestRegistryAssetSource, ReqwestRegistryTextSource, install_live_model, install_live_script,
    load_installed_model_info as load_registry_installed_model_info, managed_script_relative_path,
    materialize_asr_provider, materialize_llm_adapter, prepare_provider_script_edit,
    scan_installed_models,
};
use vinpst_text::{
    OpenAiCompatibleTextAdapter, ReqwestOpenAiCompatibleChatTransport, TextAdapter, TextRequest,
    build_openai_compatible_chat_request,
};

#[allow(clippy::too_many_lines)]
fn main() -> anyhow::Result<()> {
    let mut args = parse_args_with_global_json_alias();
    if args.json {
        force_json_output(&mut args.command);
    }

    match args.command {
        Command::Init {
            config,
            model_root,
            cache_root,
            force,
            dry_run,
            json,
        } => handle_init(InitRequest {
            config_path: config.as_deref(),
            model_root: model_root.as_deref(),
            cache_root: cache_root.as_deref(),
            force,
            dry_run,
            json_output: json,
        }),
        Command::Protocol => print_protocol(),
        Command::Config { command } => match command {
            Some(ConfigCommand::Validate { path, summary_only }) => {
                validate_config_file(&path, summary_only)
            }
            Some(ConfigCommand::Get {
                pointer,
                config,
                exists,
                default,
                default_string,
                json,
            }) => handle_config_get(
                &pointer,
                config.as_ref(),
                exists,
                default.as_deref(),
                default_string,
                json,
            ),
            Some(ConfigCommand::Set {
                pointer,
                value,
                string,
                config,
                output,
                in_place,
                dry_run,
                json,
            }) => handle_config_set(ConfigSetRequest {
                pointer: &pointer,
                raw_value: &value,
                force_string: string,
                config_path: config.as_ref(),
                output_path: output.as_deref(),
                in_place,
                dry_run,
                json_output: json,
            }),
            Some(ConfigCommand::Edit {
                config,
                editor,
                dry_run,
                json,
            }) => handle_config_edit(ConfigEditRequest {
                config_path: config.as_ref(),
                editor: editor.as_deref(),
                dry_run,
                json_output: json,
            }),
            Some(ConfigCommand::Example { kind, list, output }) => {
                handle_config_example(kind, list, output.as_deref())
            }
            None => validate_config(),
        },
        Command::Registry { command } => match command {
            Some(RegistryCommand::Validate { path }) => validate_registry_index(&path),
            Some(RegistryCommand::Plan {
                path,
                config,
                model,
                adapter,
                summary_only,
            }) => print_registry_plan(
                &path,
                config.as_ref(),
                model.as_deref(),
                adapter.as_deref(),
                summary_only,
            ),
            Some(RegistryCommand::InstallPlan {
                path,
                target_root,
                config,
                model,
                adapter,
                summary_only,
            }) => print_registry_install_plan(
                &path,
                &target_root,
                config.as_ref(),
                model.as_deref(),
                adapter.as_deref(),
                summary_only,
            ),
            None => print_registry_summary(),
        },
        Command::Daemon { command } => handle_daemon_command(&command),
        Command::Recording { command } => handle_recording_command(command),
        Command::Device { command } => handle_device_command(command),
        Command::Hotword { command } => handle_hotword_command(command),
        Command::Llm { command } => handle_llm_command(command),
        Command::Adapter { command } => handle_adapter_command(command),
        Command::Scene { command } => handle_scene_command(command),
        Command::Provider { command } => handle_provider_command(command),
        Command::Model { command } => handle_model_command(command),
        Command::AsrState { config } => print_asr_state(config.as_ref()),
        Command::AudioDevices { config } => print_audio_devices(config.as_ref()),
        Command::Doctor { config } => print_doctor(config.as_ref()),
        Command::ActivationService {
            daemon,
            config,
            configured_backends,
            audio_backend,
            daemon_args,
            user,
            remove_user,
            user_status,
            output,
        } => {
            if remove_user {
                remove_user_activation_service()
            } else if user_status {
                print_user_activation_service_status()
            } else {
                let daemon = daemon.context("--daemon is required unless --remove-user is set")?;
                write_activation_service(
                    &daemon,
                    config.as_deref(),
                    configured_backends,
                    audio_backend.as_deref(),
                    &daemon_args,
                    user,
                    output.as_deref(),
                )
            }
        }
        Command::MockResult { text } => {
            let payload = RecognitionPayload::raw(text);
            println!("{}", payload.to_json_string()?);
            Ok(())
        }
        Command::Status { status } => {
            let status = ServiceStatus::parse_wire(&status)
                .with_context(|| format!("parse status `{status}`"))?;
            println!("{status}");
            Ok(())
        }
    }
}

pub(crate) fn hotword_supported(kind: &AsrProviderKind) -> bool {
    matches!(kind, AsrProviderKind::Local | AsrProviderKind::Command)
}
