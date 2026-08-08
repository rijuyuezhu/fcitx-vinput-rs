use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::hotword::HotwordCommand;

mod adapter;
mod config;
mod daemon;
mod device;
mod llm;
mod model;
mod provider;
mod recording;
mod registry;
mod scene;

pub(crate) use adapter::AdapterCommand;
pub(crate) use config::{ConfigCommand, ConfigExample};
pub(crate) use daemon::DaemonCommand;
pub(crate) use device::DeviceCommand;
pub(crate) use llm::LlmCommand;
pub(crate) use model::ModelCommand;
pub(crate) use provider::ProviderCommand;
pub(crate) use recording::RecordingCommand;
pub(crate) use registry::RegistryCommand;
pub(crate) use scene::SceneCommand;

/// Manage Vinpst voice input, configuration, and diagnostics.
#[derive(Debug, Parser)]
#[command(version, about)]
pub(crate) struct Args {
    /// Force machine-readable JSON output for JSON-capable subcommands.
    #[arg(short = 'j', long, global = true)]
    pub(crate) json: bool,
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// Supported bootstrap commands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Initialize per-user config and managed directories.
    Init {
        /// Config path to create. Defaults to $XDG_CONFIG_HOME/fcitx-vinpst/config.json.
        #[arg(long, hide = true)]
        config: Option<PathBuf>,
        /// Managed model root to create. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long, hide = true)]
        model_root: Option<PathBuf>,
        /// Managed cache root to create. Defaults to $XDG_CACHE_HOME/fcitx-vinpst.
        #[arg(long, hide = true)]
        cache_root: Option<PathBuf>,
        /// Overwrite an existing config file with the bundled default config.
        #[arg(long)]
        force: bool,
        /// Print the initialization plan without writing files or creating directories.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Print stable D-Bus names and methods.
    #[command(hide = true)]
    Protocol,
    /// Manage Vinpst configuration.
    Config {
        /// Config operation. Omitted to validate the bundled default config.
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    /// Inspect or validate registry metadata.
    #[command(hide = true)]
    Registry {
        /// Registry operation. Omitted to print URL resolution for the bundled config.
        #[command(subcommand)]
        command: Option<RegistryCommand>,
    },
    /// Start, stop, or inspect the Vinpst daemon.
    Daemon {
        /// Daemon operation.
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Start, stop, toggle, or inspect recording.
    Recording {
        /// Recording operation.
        #[command(subcommand)]
        command: RecordingCommand,
    },
    /// List or select capture devices.
    Device {
        /// Device operation.
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// Inspect ASR hotword configuration.
    Hotword {
        /// Hotword operation.
        #[command(subcommand)]
        command: HotwordCommand,
    },
    /// Inspect or manage LLM providers.
    Llm {
        /// LLM operation.
        #[command(subcommand)]
        command: LlmCommand,
    },
    /// Inspect or manage text adapters.
    Adapter {
        /// Adapter operation.
        #[command(subcommand)]
        command: AdapterCommand,
    },
    /// Inspect or manage recognition scenes.
    Scene {
        /// Scene operation.
        #[command(subcommand)]
        command: SceneCommand,
    },
    /// Inspect or manage ASR providers.
    Provider {
        /// Provider operation.
        #[command(subcommand)]
        command: ProviderCommand,
    },
    /// Manage ASR models.
    Model {
        /// Model operation.
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Print ASR backend availability diagnostics from config.
    #[command(hide = true)]
    AsrState {
        /// Optional config JSON file. Omitted to inspect the bundled default config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print capture-device diagnostics from config and optional live backend.
    #[command(hide = true)]
    AudioDevices {
        /// Optional config JSON file. Omitted to inspect the bundled default config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Check the local Vinpst setup and report problems.
    Doctor {
        /// Optional config JSON file. Omitted to inspect the bundled default config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Generate, install, or remove an org.fcitx.Vinpst D-Bus activation service file.
    #[command(hide = true)]
    ActivationService {
        /// Path to the vinpst-daemon executable used by D-Bus activation.
        #[arg(long, required_unless_present_any = ["remove_user", "user_status"])]
        daemon: Option<PathBuf>,
        /// Optional config JSON file passed to vinpst-daemon.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Activate configured ASR/text backends instead of the mock runtime.
        #[arg(long)]
        configured_backends: bool,
        /// Optional audio backend passed to vinpst-daemon, such as mock or pipewire.
        #[arg(long)]
        audio_backend: Option<String>,
        /// Extra argument forwarded to vinpst-daemon; repeat for multiple arguments.
        #[arg(long = "daemon-arg")]
        daemon_args: Vec<String>,
        /// Write to the per-user D-Bus activation service path.
        #[arg(long, conflicts_with = "output")]
        user: bool,
        /// Remove the per-user D-Bus activation service file and print JSON status.
        #[arg(
            long,
            conflicts_with_all = [
                "daemon",
                "config",
                "configured_backends",
                "audio_backend",
                "daemon_args",
                "user",
                "user_status",
                "output"
            ]
        )]
        remove_user: bool,
        /// Print per-user D-Bus activation service status as JSON.
        #[arg(
            long,
            conflicts_with_all = [
                "daemon",
                "config",
                "configured_backends",
                "audio_backend",
                "daemon_args",
                "user",
                "output"
            ]
        )]
        user_status: bool,
        /// Write the service file to this path instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Create a recognition JSON payload for tests/manual inspection.
    #[command(hide = true)]
    MockResult {
        /// Commit text for the payload.
        text: String,
    },
    /// Parse a status string and print the normalized wire value.
    #[command(hide = true)]
    Status {
        /// Status string such as idle, recording, inferring, postprocessing, or error.
        status: String,
    },
}

pub(crate) fn parse_args_with_global_json_alias() -> Args {
    match Args::try_parse() {
        Ok(args) => args,
        Err(original_error) => {
            let (filtered_args, saw_json_alias) = strip_global_json_aliases(std::env::args_os());
            if !saw_json_alias {
                original_error.exit();
            }
            match Args::try_parse_from(filtered_args) {
                Ok(mut args) => {
                    args.json = true;
                    args
                }
                Err(_) => original_error.exit(),
            }
        }
    }
}

fn strip_global_json_aliases(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> (Vec<std::ffi::OsString>, bool) {
    let mut saw_json_alias = false;
    let mut after_argument_delimiter = false;
    let filtered = args
        .into_iter()
        .filter(|arg| {
            if after_argument_delimiter {
                return true;
            }
            if arg == "--" {
                after_argument_delimiter = true;
                return true;
            }
            if arg == "-j" || arg == "--json" {
                saw_json_alias = true;
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    (filtered, saw_json_alias)
}

pub(crate) fn force_json_output(command: &mut Command) {
    match command {
        Command::Init { json, .. } => *json = true,
        Command::Config { command } => {
            if let Some(command) = command {
                match command {
                    ConfigCommand::Get { json, .. }
                    | ConfigCommand::Set { json, .. }
                    | ConfigCommand::Edit { json, .. } => *json = true,
                    ConfigCommand::Validate { .. } | ConfigCommand::Example { .. } => {}
                }
            }
        }
        Command::Daemon { command } => match command {
            DaemonCommand::Start { json, .. }
            | DaemonCommand::Status { json, .. }
            | DaemonCommand::Handoff { json, .. }
            | DaemonCommand::PrepareRemove { json, .. }
            | DaemonCommand::ReloadAsr { json, .. }
            | DaemonCommand::InstallService { json, .. }
            | DaemonCommand::Stop { json, .. }
            | DaemonCommand::Restart { json, .. }
            | DaemonCommand::Log { json, .. } => *json = true,
        },
        Command::Recording { command } => match command {
            RecordingCommand::Start { json, .. }
            | RecordingCommand::Stop { json, .. }
            | RecordingCommand::Status { json, .. }
            | RecordingCommand::Toggle { json, .. } => *json = true,
        },
        Command::Device { command } => match command {
            DeviceCommand::List { json, .. } | DeviceCommand::Use { json, .. } => *json = true,
        },
        Command::Hotword { command } => match command {
            HotwordCommand::Get { json, .. }
            | HotwordCommand::Set { json, .. }
            | HotwordCommand::Clear { json, .. }
            | HotwordCommand::Edit { json, .. } => *json = true,
        },
        Command::Llm { command } => match command {
            LlmCommand::List { json, .. }
            | LlmCommand::Add { json, .. }
            | LlmCommand::Edit { json, .. }
            | LlmCommand::Test { json, .. }
            | LlmCommand::Remove { json, .. } => *json = true,
        },
        Command::Adapter { command } => match command {
            AdapterCommand::List { json, .. }
            | AdapterCommand::Add { json, .. }
            | AdapterCommand::Install { json, .. }
            | AdapterCommand::InstallPlan { json, .. }
            | AdapterCommand::Edit { json, .. }
            | AdapterCommand::Start { json, .. }
            | AdapterCommand::Stop { json, .. }
            | AdapterCommand::Status { json, .. }
            | AdapterCommand::Remove { json, .. } => *json = true,
        },
        Command::Scene { command } => match command {
            SceneCommand::List { json, .. }
            | SceneCommand::Add { json, .. }
            | SceneCommand::Edit { json, .. }
            | SceneCommand::Use { json, .. }
            | SceneCommand::Remove { json, .. } => *json = true,
        },
        Command::Provider { command } => match command {
            ProviderCommand::List { json, .. }
            | ProviderCommand::Install { json, .. }
            | ProviderCommand::Add { json, .. }
            | ProviderCommand::Use { json, .. }
            | ProviderCommand::Edit { json, .. }
            | ProviderCommand::EditScript { json, .. }
            | ProviderCommand::Remove { json, .. } => *json = true,
        },
        Command::Model { command } => match command {
            ModelCommand::List { json, .. }
            | ModelCommand::Info { json, .. }
            | ModelCommand::Install { json, .. }
            | ModelCommand::Use { json, .. }
            | ModelCommand::Remove { json, .. } => *json = true,
        },
        Command::Registry { .. }
        | Command::Protocol
        | Command::AsrState { .. }
        | Command::AudioDevices { .. }
        | Command::Doctor { .. }
        | Command::ActivationService { .. }
        | Command::MockResult { .. }
        | Command::Status { .. } => {}
    }
}

#[cfg(test)]
mod help_surface_tests {
    use clap::{CommandFactory, Parser};

    use super::{AdapterCommand, Args, Command, ConfigCommand, ProviderCommand, SceneCommand};
    use crate::hotword::HotwordCommand;

    #[test]
    fn internal_top_level_commands_are_hidden_but_registered() {
        let command = Args::command();
        for name in [
            "protocol",
            "registry",
            "asr-state",
            "audio-devices",
            "activation-service",
            "mock-result",
            "status",
        ] {
            let subcommand = command
                .find_subcommand(name)
                .expect("registered subcommand");
            assert!(subcommand.is_hide_set(), "{name} should be hidden");
        }
    }

    #[test]
    fn public_root_command_surface_is_exact() {
        let command = Args::command();
        let visible = command
            .get_subcommands()
            .filter(|subcommand| !subcommand.is_hide_set() && subcommand.get_name() != "help")
            .map(clap::Command::get_name)
            .collect::<Vec<_>>();
        assert_eq!(
            visible,
            [
                "init",
                "config",
                "daemon",
                "recording",
                "device",
                "hotword",
                "llm",
                "adapter",
                "scene",
                "provider",
                "model",
                "doctor",
            ]
        );
    }

    #[test]
    fn maintenance_subcommands_are_hidden_but_registered() {
        let command = Args::command();
        let daemon = command.find_subcommand("daemon").expect("daemon command");
        for name in ["handoff", "prepare-remove", "install-service"] {
            let subcommand = daemon
                .find_subcommand(name)
                .expect("maintenance subcommand");
            assert!(subcommand.is_hide_set(), "daemon {name} should be hidden");
        }

        let adapter = command.find_subcommand("adapter").expect("adapter command");
        let install_plan = adapter
            .find_subcommand("install-plan")
            .expect("adapter install-plan");
        assert!(install_plan.is_hide_set());

        let config = command.find_subcommand("config").expect("config command");
        let example = config
            .find_subcommand("example")
            .expect("config example command");
        assert!(example.is_hide_set());
    }

    #[test]
    fn fixture_and_transport_plan_options_are_hidden_from_help() {
        let command = Args::command();

        let init = command.find_subcommand("init").expect("init command");
        for arg_id in ["config", "model_root", "cache_root"] {
            let arg = init
                .get_arguments()
                .find(|arg| arg.get_id().as_str() == arg_id)
                .expect("registered init fixture argument");
            assert!(arg.is_hide_set(), "init {arg_id} should be hidden");
        }

        let config = command.find_subcommand("config").expect("config command");
        let validate = config
            .find_subcommand("validate")
            .expect("config validate command");
        let summary_only = validate
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "summary_only")
            .expect("config validate summary-only argument");
        assert!(summary_only.is_hide_set());

        for (group, subcommand, hidden_args) in [
            (
                "model",
                "list",
                &["model_root", "registry", "i18n", "locale"][..],
            ),
            (
                "model",
                "install",
                &["registry", "i18n", "locale", "model_root", "staging_root"][..],
            ),
            ("provider", "list", &["registry", "i18n", "locale"][..]),
            ("provider", "install", &["registry", "provider_root"][..]),
            ("adapter", "list", &["registry", "i18n", "locale"][..]),
            ("adapter", "install", &["registry", "adapter_root"][..]),
            ("daemon", "status", &["dry_run"][..]),
            ("recording", "start", &["selected_text", "dry_run"][..]),
        ] {
            let group = command.find_subcommand(group).expect("command group");
            let subcommand = group.find_subcommand(subcommand).expect("user subcommand");
            for arg_id in hidden_args {
                let arg = subcommand
                    .get_arguments()
                    .find(|arg| arg.get_id().as_str() == *arg_id)
                    .expect("registered hidden argument");
                assert!(
                    arg.is_hide_set(),
                    "{arg_id} should remain callable but hidden"
                );
            }
        }
    }

    #[test]
    fn familiar_upstream_edit_and_remove_aliases_remain_accepted() {
        let args =
            Args::try_parse_from(["vinpst", "config", "e"]).expect("upstream config edit alias");
        assert!(matches!(
            args.command,
            Command::Config {
                command: Some(ConfigCommand::Edit { .. })
            }
        ));

        let args =
            Args::try_parse_from(["vinpst", "hotword", "e"]).expect("upstream hotword edit alias");
        assert!(matches!(
            args.command,
            Command::Hotword {
                command: HotwordCommand::Edit { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "provider", "e", "provider.demo"])
            .expect("upstream provider edit-script alias");
        assert!(matches!(
            args.command,
            Command::Provider {
                command: ProviderCommand::EditScript { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "provider", "rm", "provider.demo"])
            .expect("upstream provider remove alias");
        assert!(matches!(
            args.command,
            Command::Provider {
                command: ProviderCommand::Remove { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "provider", "add", "provider.demo"])
            .expect("upstream provider add alias");
        assert!(matches!(
            args.command,
            Command::Provider {
                command: ProviderCommand::Install { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "provider", "edit", "provider.demo"])
            .expect("upstream provider edit command");
        assert!(matches!(
            args.command,
            Command::Provider {
                command: ProviderCommand::EditScript { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "adapter", "add", "adapter.demo"])
            .expect("upstream adapter add alias");
        assert!(matches!(
            args.command,
            Command::Adapter {
                command: AdapterCommand::Install { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "scene", "e", "scene.demo"])
            .expect("upstream scene edit alias");
        assert!(matches!(
            args.command,
            Command::Scene {
                command: SceneCommand::Edit { .. }
            }
        ));

        let args = Args::try_parse_from(["vinpst", "scene", "rm", "scene.demo"])
            .expect("upstream scene remove alias");
        assert!(matches!(
            args.command,
            Command::Scene {
                command: SceneCommand::Remove { .. }
            }
        ));
    }
}
