//! vinput daemon entrypoint.

use anyhow::Context;
use clap::{Parser, Subcommand};
use tracing::info;
use vinput_config::VinputConfig;
use vinput_daemon::{RuntimeState, VinputDbusService};

/// Rust daemon prototype for fcitx-vinput.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Print one mock recognition cycle and exit instead of running forever.
    #[arg(long)]
    once: bool,

    /// Command-mode selected text for `--once`.
    #[arg(long)]
    selected_text: Option<String>,

    /// Serve the legacy D-Bus ABI on the session bus.
    #[arg(long)]
    dbus: bool,

    /// Utility command.
    #[command(subcommand)]
    command: Option<Command>,
}

/// One-shot utility commands useful while bootstrapping the daemon.
#[derive(Debug, Subcommand)]
enum Command {
    /// Print the parsed bundled config as normalized JSON.
    PrintConfig,
    /// Print mock ASR backend state as JSON.
    AsrState,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = VinputConfig::bundled_default().context("load bundled default config")?;
    config
        .validate()
        .context("validate bundled default config")?;
    let mut runtime = RuntimeState::new(config.clone()).context("initialize runtime")?;

    match args.command {
        Some(Command::PrintConfig) => {
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        Some(Command::AsrState) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&runtime.reload_asr_backend()?)?
            );
        }
        None if args.once => {
            if let Some(selected_text) = args.selected_text {
                runtime.start_command_recording(selected_text)?;
            } else {
                runtime.start_recording()?;
            }
            if let Some(partial) = runtime.partial_text() {
                info!(partial, "mock partial recognition");
            }
            let payload = runtime.stop_recording(None)?;
            println!("{}", payload.to_json_string()?);
        }
        None if args.dbus => {
            let _connection = VinputDbusService::new(runtime)
                .serve_on_session_bus()
                .await
                .context("serve vinput D-Bus service")?;
            info!(
                bus = vinput_protocol::dbus::SERVICE_BUS_NAME,
                object = vinput_protocol::dbus::SERVICE_OBJECT_PATH,
                interface = vinput_protocol::dbus::SERVICE_INTERFACE,
                "mock daemon D-Bus service is running"
            );
            tokio::signal::ctrl_c().await.context("wait for ctrl-c")?;
        }
        None => {
            info!(
                status = %runtime.status(),
                uptime_ms = runtime.uptime().as_millis(),
                "mock daemon initialized; pass --dbus to expose the legacy D-Bus ABI"
            );
            tokio::signal::ctrl_c().await.context("wait for ctrl-c")?;
        }
    }

    Ok(())
}
