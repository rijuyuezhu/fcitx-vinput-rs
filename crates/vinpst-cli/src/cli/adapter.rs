use std::path::PathBuf;

use clap::Subcommand;
use vinpst_registry::detect_preferred_registry_locale;

/// Text adapter management commands.
#[derive(Debug, Subcommand)]
pub(crate) enum AdapterCommand {
    /// List configured text adapters.
    #[command(alias = "ls")]
    List {
        /// List text adapters available for installation.
        #[arg(short = 'a', long)]
        available: bool,
        /// Optional local registry index JSON used by --available.
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
    /// Create a custom command text adapter in config.
    #[command(name = "create")]
    Add {
        /// New adapter id.
        id: String,
        /// Adapter executable path or command name.
        #[arg(long)]
        command: String,
        /// Adapter command argument. Repeat for multiple args.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Adapter environment entry as KEY=VALUE. Repeat for multiple entries.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Optional working directory for the adapter process.
        #[arg(long)]
        working_dir: Option<String>,
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
    /// Install or update a command text adapter from the live script registry.
    #[command(alias = "add")]
    Install {
        /// Full adapter id or registry `short_id`.
        id: String,
        /// Optional local registry/adapters.json file. Omitted to fetch configured mirrors.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Managed adapter script root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/adapters.
        #[arg(long, hide = true)]
        adapter_root: Option<PathBuf>,
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
    /// Print a dry-run install plan for a registry adapter.
    #[command(hide = true)]
    InstallPlan {
        /// Adapter id from the registry index.
        id: String,
        /// Local registry index JSON containing adapter entries.
        #[arg(long, hide = true)]
        registry: PathBuf,
        /// Target root directory for planned adapter asset installation.
        #[arg(long)]
        target_root: PathBuf,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print only summary fields without per-asset rows.
        #[arg(long)]
        summary_only: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Edit a configured command text adapter.
    Edit {
        /// Existing adapter id to edit.
        id: String,
        /// Set adapter executable path or command name.
        #[arg(long)]
        command: Option<String>,
        /// Replace adapter command arguments. Repeat for multiple args.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// Clear adapter command arguments.
        #[arg(long)]
        clear_args: bool,
        /// Replace adapter environment entries with KEY=VALUE assignments.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Clear adapter environment entries.
        #[arg(long)]
        clear_env: bool,
        /// Set optional working directory for the adapter process.
        #[arg(long)]
        working_dir: Option<String>,
        /// Clear adapter working directory.
        #[arg(long)]
        clear_working_dir: bool,
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
    /// Start a configured text adapter through daemon D-Bus.
    Start {
        /// Existing adapter id or registry `short_id` to start.
        id: String,
        /// Optional local registry/adapters.json file used to resolve a short id.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop a configured text adapter through daemon D-Bus.
    Stop {
        /// Existing adapter id or registry `short_id` to stop.
        id: String,
        /// Optional local registry/adapters.json file used to resolve a short id.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Inspect daemon text adapter runtime state.
    Status {
        /// Optional adapter id or registry `short_id` to filter. Omitted to show all adapters.
        id: Option<String>,
        /// Optional local registry/adapters.json file used to resolve a short id.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional config JSON file used when filtering by adapter. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove a configured text adapter.
    #[command(alias = "rm")]
    Remove {
        /// Existing adapter id or registry `short_id` to remove.
        id: String,
        /// Optional local registry/adapters.json file used to resolve a short id.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Managed adapter script root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/adapters.
        #[arg(long, hide = true)]
        adapter_root: Option<PathBuf>,
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
