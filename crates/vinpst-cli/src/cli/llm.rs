use std::path::PathBuf;

use clap::Subcommand;

/// LLM provider management commands.
#[derive(Debug, Subcommand)]
pub(crate) enum LlmCommand {
    /// List configured LLM providers.
    #[command(alias = "ls")]
    List {
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Add an OpenAI-compatible LLM provider to config.
    Add {
        /// New LLM provider id.
        id: String,
        /// Base URL for OpenAI-compatible chat completions.
        #[arg(short = 'u', long)]
        base_url: String,
        /// API key or environment-reference expression.
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        /// Optional default model name.
        #[arg(long)]
        model: Option<String>,
        /// Extra JSON object merged into provider requests.
        #[arg(short = 'e', long)]
        extra_body: Option<String>,
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
    /// Edit an existing LLM provider in config.
    #[command(alias = "e")]
    Edit {
        /// Existing LLM provider id to edit.
        id: String,
        /// Set base URL for OpenAI-compatible chat completions.
        #[arg(short = 'u', long)]
        base_url: Option<String>,
        /// Set API key or environment-reference expression.
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        /// Clear API key from this provider.
        #[arg(long)]
        clear_api_key: bool,
        /// Set default model name.
        #[arg(long)]
        model: Option<String>,
        /// Clear default model from this provider.
        #[arg(long)]
        clear_model: bool,
        /// Set extra JSON object merged into provider requests.
        #[arg(short = 'e', long)]
        extra_body: Option<String>,
        /// Clear extra JSON body from this provider.
        #[arg(long)]
        clear_extra_body: bool,
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
    /// Test an LLM provider with an OpenAI-compatible chat request.
    Test {
        /// Existing LLM provider id to test.
        id: String,
        /// Raw text used in the synthetic connectivity test prompt.
        #[arg(long, default_value = "vinpst LLM connectivity test")]
        text: String,
        /// Optional timeout in milliseconds for the test request.
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Optional config JSON file. Omitted to read the user config, then the bundled default.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Print the request plan without contacting the provider.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove an LLM provider from config.
    #[command(alias = "rm")]
    Remove {
        /// Existing LLM provider id to remove. Scene bindings to it are cleared.
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
