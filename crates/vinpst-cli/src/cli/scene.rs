use std::path::PathBuf;

use clap::Subcommand;

/// Scene management commands.
#[derive(Debug, Subcommand)]
pub(crate) enum SceneCommand {
    /// List configured recognition scenes.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Add a recognition scene to config.
    Add {
        /// New scene id.
        id: String,
        /// Display label or translation key for the scene.
        #[arg(long)]
        label: String,
        /// Optional prompt template for post-processing.
        #[arg(long)]
        prompt: Option<String>,
        /// Optional LLM provider id for this scene.
        #[arg(long)]
        provider_id: Option<String>,
        /// Optional model override for this scene.
        #[arg(long)]
        model: Option<String>,
        /// Number of result candidates requested from post-processing.
        #[arg(long, default_value_t = 0)]
        candidate_count: u8,
        /// Optional per-scene timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Recent input context lines to include.
        #[arg(long, default_value_t = 0)]
        context_lines: u8,
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
    /// Edit an explicitly configured recognition scene.
    #[command(alias = "e")]
    Edit {
        /// Existing scene id to edit.
        id: String,
        /// Set display label or translation key for the scene.
        #[arg(long)]
        label: Option<String>,
        /// Set prompt template for post-processing.
        #[arg(long)]
        prompt: Option<String>,
        /// Clear prompt from this scene.
        #[arg(long)]
        clear_prompt: bool,
        /// Set LLM provider id for this scene.
        #[arg(long)]
        provider_id: Option<String>,
        /// Clear LLM provider id from this scene.
        #[arg(long)]
        clear_provider_id: bool,
        /// Set model override for this scene.
        #[arg(long)]
        model: Option<String>,
        /// Clear model override from this scene.
        #[arg(long)]
        clear_model: bool,
        /// Set candidate count.
        #[arg(long)]
        candidate_count: Option<u8>,
        /// Set per-scene timeout in milliseconds.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Clear per-scene timeout.
        #[arg(long)]
        clear_timeout: bool,
        /// Set recent input context lines to include.
        #[arg(long)]
        context_lines: Option<u8>,
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
    /// Select the active recognition scene in config.
    Use {
        /// Existing scene id to activate.
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
    /// Remove an inactive explicitly configured recognition scene.
    #[command(alias = "rm")]
    Remove {
        /// Existing inactive scene id to remove.
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
}
