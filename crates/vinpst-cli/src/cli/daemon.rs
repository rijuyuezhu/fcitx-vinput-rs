use std::path::PathBuf;

use clap::Subcommand;

/// Daemon-related commands backed by the D-Bus service contract.
#[derive(Debug, Subcommand)]
pub(crate) enum DaemonCommand {
    /// Start the daemon if it is not already running.
    Start {
        /// Print the D-Bus activation plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Show daemon and ASR status.
    Status {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Restart the user service only when daemon status reports a stale owner.
    #[command(hide = true)]
    Handoff {
        /// Print the conditional restart plan without contacting the daemon or systemd.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop and disable the running daemon before package removal.
    #[command(hide = true)]
    PrepareRemove {
        /// Print the guarded removal plan without contacting D-Bus or systemd.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Probe the live session and removal guards without stopping or signalling anything.
        #[arg(long, conflicts_with = "dry_run")]
        preflight: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Reload the selected ASR backend.
    ReloadAsr {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Install or refresh the per-user daemon service, rewriting it for Flatpak when detected.
    #[command(hide = true)]
    InstallService {
        /// Read an explicit service template instead of the packaged default.
        #[arg(long)]
        template: Option<PathBuf>,
        /// Write an explicit user-service path instead of the XDG default.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Print the rendered service without writing or reloading systemd.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },

    /// Stop the user daemon service.
    Stop {
        /// Print the stop plan without mutating user services.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Restart the user daemon service.
    Restart {
        /// Print the restart plan without mutating user services.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Print daemon logs from the user service journal.
    Log {
        /// Limit journal output to the last N lines.
        #[arg(short = 'n', long)]
        lines: Option<u16>,
        /// Print the log retrieval plan without invoking external tools.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}
