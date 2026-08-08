use std::path::PathBuf;

use clap::Subcommand;
use vinpst_registry::detect_preferred_registry_locale;

/// ASR provider management commands.
#[derive(Debug, Subcommand)]
pub(crate) enum ProviderCommand {
    /// List configured or available ASR providers.
    #[command(alias = "ls")]
    List {
        /// List providers from the live script registry instead of configured providers.
        #[arg(short = 'a', long)]
        available: bool,
        /// Optional local registry/providers.json file. Omitted to fetch configured mirrors.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional local registry i18n JSON used by --available.
        #[arg(long, hide = true)]
        i18n: Option<PathBuf>,
        /// Registry locale used by --available.
        #[arg(long, default_value_t = detect_preferred_registry_locale(), hide = true)]
        locale: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Install or update an ASR provider from the live script registry.
    #[command(alias = "add")]
    Install {
        /// Full provider id or registry `short_id`.
        id: String,
        /// Optional local registry/providers.json file. Omitted to fetch configured mirrors.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Managed provider script root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/providers.
        #[arg(long, hide = true)]
        provider_root: Option<PathBuf>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Resolve and validate the installation without downloading or writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Select the active ASR provider in config.
    Use {
        /// Existing ASR provider id to activate.
        id: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Create a custom ASR provider in config.
    #[command(name = "create")]
    Add {
        /// New ASR provider id.
        id: String,
        /// Provider type: local, command, or remote.
        #[arg(long = "type", default_value = "local")]
        kind: String,
        /// Optional model id/path for this provider.
        #[arg(long)]
        model: Option<String>,
        /// Optional hotwords file path for local/command providers.
        #[arg(long)]
        hotwords_file: Option<String>,
        /// External command for command providers.
        #[arg(long)]
        command: Option<String>,
        /// Repeated argument for command providers.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Repeated KEY=VALUE environment assignment for command providers.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Endpoint URL or label for remote providers.
        #[arg(long)]
        endpoint: Option<String>,
        /// Optional provider timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Configure an existing ASR provider in config.
    #[command(name = "configure")]
    Edit {
        /// Existing ASR provider id to edit.
        id: String,
        /// Provider type: local, command, or remote.
        #[arg(long = "type")]
        kind: Option<String>,
        /// Set model id/path for this provider.
        #[arg(long)]
        model: Option<String>,
        /// Clear model from this provider.
        #[arg(long)]
        clear_model: bool,
        /// Set hotwords file path for local/command providers.
        #[arg(long)]
        hotwords_file: Option<String>,
        /// Clear hotwords file from this provider.
        #[arg(long)]
        clear_hotwords_file: bool,
        /// Set external command for command providers.
        #[arg(long)]
        command: Option<String>,
        /// Clear command from this provider.
        #[arg(long)]
        clear_command: bool,
        /// Replace command arguments. Repeat for multiple args.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Clear command arguments from this provider.
        #[arg(long)]
        clear_args: bool,
        /// Replace environment entries with repeated KEY=VALUE assignments.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Clear environment entries from this provider.
        #[arg(long)]
        clear_env: bool,
        /// Set endpoint URL or label for remote providers.
        #[arg(long)]
        endpoint: Option<String>,
        /// Clear endpoint from this provider.
        #[arg(long)]
        clear_endpoint: bool,
        /// Set provider timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Clear provider timeout.
        #[arg(long)]
        clear_timeout: bool,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Open the script referenced by an installed command ASR provider.
    #[command(name = "edit", aliases = ["e", "edit-script", "es"])]
    EditScript {
        /// Existing provider id or registry `short_id` to edit.
        id: String,
        /// Optional local registry/providers.json file used for short-id resolution.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Editor command. Defaults to `$VINPST_PROVIDER_EDITOR`, `$VISUAL`, `$EDITOR`, then `vi`.
        #[arg(long)]
        editor: Option<String>,
        /// Resolve the provider, script, and editor without launching it.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove a non-local ASR provider from config.
    #[command(alias = "rm")]
    Remove {
        /// Existing provider id or registry `short_id` to remove.
        id: String,
        /// Optional local registry/providers.json file used for short-id resolution.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input/user config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}
