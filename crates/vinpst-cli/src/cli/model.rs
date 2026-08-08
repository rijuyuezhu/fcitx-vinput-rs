use std::path::PathBuf;

use clap::Subcommand;
use vinpst_registry::detect_preferred_registry_locale;

/// Manage ASR models.
#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// List available or installed models.
    #[command(alias = "ls")]
    List {
        /// List models available for installation.
        #[arg(short = 'a', long)]
        available: bool,
        /// List installed models.
        #[arg(long)]
        installed: bool,
        /// Managed model root used by --installed. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long, hide = true)]
        model_root: Option<PathBuf>,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long, hide = true)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale(), hide = true)]
        locale: String,
        /// Print machine-readable JSON instead of text table output.
        #[arg(long)]
        json: bool,
    },
    /// Show model information.
    Info {
        /// Full model id, `short_id`, installed path, or managed model directory name with --installed.
        id: String,
        /// Read information for an installed model.
        #[arg(long)]
        installed: bool,
        /// Managed model root used by --installed. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long, hide = true)]
        model_root: Option<PathBuf>,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long, hide = true)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale(), hide = true)]
        locale: String,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Install a model, or inspect the plan with --dry-run.
    #[command(alias = "add")]
    Install {
        /// Full model id or `short_id`.
        id: String,
        /// Optional local live registry/models.json file. Omitted to fetch configured mirrors.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description fallback.
        #[arg(long, hide = true)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale(), hide = true)]
        locale: String,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long, hide = true)]
        model_root: Option<PathBuf>,
        /// Temporary staging root. Defaults to $XDG_CACHE_HOME/fcitx-vinpst/model-install.
        #[arg(long, hide = true)]
        staging_root: Option<PathBuf>,
        /// Print the install plan without downloading, extracting, or writing config.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Select the active local ASR model.
    Use {
        /// Full model id, `short_id`, installed model path, or managed model dir name.
        selector: String,
        /// Treat selector as an installed model name.
        #[arg(long)]
        installed: bool,
        /// Optional local live registry/models.json file for resolving `id`/`short_id`.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description output.
        #[arg(long, hide = true)]
        i18n: Option<PathBuf>,
        /// Config JSON file to update.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale(), hide = true)]
        locale: String,
        /// ASR provider id to update. Defaults to the config active provider.
        #[arg(long)]
        provider: Option<String>,
        /// Write the updated config to this path when not using --dry-run.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Update --config in place and write a <config>.bak backup.
        #[arg(long)]
        in_place: bool,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long, hide = true)]
        model_root: Option<PathBuf>,
        /// Reload the running daemon ASR backend after writing config. Dry-run prints the planned call.
        #[arg(long)]
        reload_daemon: bool,
        /// Preview config changes without writing.
        #[arg(long)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Remove a managed installed model.
    #[command(alias = "rm")]
    Remove {
        /// Full model id, `short_id`, managed model dir name, or installed path under model root.
        selector: String,
        /// Treat selector as an installed model name.
        #[arg(long)]
        installed: bool,
        /// Optional local live registry/models.json file for resolving `id`/`short_id`.
        #[arg(long, hide = true)]
        registry: Option<PathBuf>,
        /// Optional local live i18n JSON file used for title/description output.
        #[arg(long, hide = true)]
        i18n: Option<PathBuf>,
        /// Optional config JSON file that provides registry mirrors.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Live registry i18n locale to fetch when reading remote mirrors.
        #[arg(long, default_value_t = detect_preferred_registry_locale(), hide = true)]
        locale: String,
        /// Managed model root. Defaults to $XDG_DATA_HOME/fcitx-vinpst/models.
        #[arg(long, hide = true)]
        model_root: Option<PathBuf>,
        /// Print the removal plan without deleting anything.
        #[arg(long)]
        dry_run: bool,
        /// Confirm removal. Required for deleting the managed model directory.
        #[arg(long)]
        yes: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}
