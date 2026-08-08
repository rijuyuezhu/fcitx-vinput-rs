use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

/// Config-related commands.
#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Validate a local config JSON file and print a summary.
    Validate {
        /// Path to a config JSON file.
        path: PathBuf,
        /// Explicitly print only summary fields.
        #[arg(long, hide = true)]
        summary_only: bool,
    },
    /// Read a config value by JSON pointer.
    Get {
        /// JSON pointer such as `/global/default_language`. Use an empty string for the whole document.
        pointer: String,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Check whether POINTER exists and do not fail when it is missing.
        #[arg(long)]
        exists: bool,
        /// Value to print when POINTER is missing. Parsed as JSON when possible.
        #[arg(long, value_name = "VALUE", conflicts_with = "exists")]
        default: Option<String>,
        /// Treat --default VALUE as a literal string without JSON parsing.
        #[arg(long, conflicts_with = "exists")]
        default_string: bool,
        /// Print machine-readable JSON instead of the raw value.
        #[arg(long)]
        json: bool,
    },
    /// Set an existing config value by JSON pointer.
    Set {
        /// JSON pointer such as `/global/default_language`. The pointer must already exist.
        pointer: String,
        /// New value. Parsed as JSON when possible, otherwise treated as a string.
        value: String,
        /// Treat VALUE as a literal string without JSON parsing.
        #[arg(long)]
        string: bool,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update the input config in place and write a <config>.bak backup when it exists.
        #[arg(long)]
        in_place: bool,
        /// Preview the validated config patch without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Open a config in an editor, then validate and write it back safely.
    #[command(alias = "e")]
    Edit {
        /// Optional config JSON file. Omitted to edit the user config, or create it from the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Editor executable to run. Defaults to `$VINPST_CONFIG_EDITOR`, `$EDITOR`, then `$VISUAL`.
        #[arg(long)]
        editor: Option<String>,
        /// Print the editor plan without invoking the editor or writing files.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Print, list, or write a bundled example config JSON file.
    #[command(hide = true)]
    Example {
        /// Example config to export. Omit with --list to show available examples.
        #[arg(value_enum, required_unless_present = "list")]
        kind: Option<ConfigExample>,
        /// List available example configs as JSON.
        #[arg(long, conflicts_with = "output")]
        list: bool,
        /// Write the example config to this path instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum ConfigExample {
    /// Upstream-compatible default config skeleton.
    Default,
    /// Deterministic command ASR/text adapter demo config.
    CommandDemo,
    /// Configured command ASR/text adapter demo intended for live `PipeWire` smoke.
    ConfiguredPipewireLive,
}
