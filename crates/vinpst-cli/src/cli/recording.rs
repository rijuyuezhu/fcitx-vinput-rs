use clap::Subcommand;

/// Recording control commands backed by the daemon D-Bus service contract.
#[derive(Debug, Subcommand)]
pub(crate) enum RecordingCommand {
    /// Start normal or command-mode recording.
    Start {
        /// Selected text context for command-mode recording.
        #[arg(long, hide = true)]
        selected_text: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Stop recording and request a recognition result.
    Stop {
        /// Scene id forwarded to `StopRecording`. Defaults to an empty scene.
        #[arg(long)]
        scene: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Query current daemon recording/status state.
    Status {
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
    /// Toggle recording by querying daemon status first.
    Toggle {
        /// Selected text context used when toggle starts command-mode recording.
        #[arg(long, hide = true)]
        selected_text: Option<String>,
        /// Scene id used when toggle stops recording. Defaults to an empty scene.
        #[arg(long)]
        scene: Option<String>,
        /// Print the D-Bus call plan without contacting the daemon.
        #[arg(long, hide = true)]
        dry_run: bool,
        /// Print machine-readable JSON instead of text output.
        #[arg(long)]
        json: bool,
    },
}
