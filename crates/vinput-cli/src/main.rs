//! `vinput` command-line prototype.

use anyhow::Context;
use clap::{Parser, Subcommand};
use vinput_config::VinputConfig;
use vinput_protocol::{RecognitionPayload, ServiceStatus, dbus};
use vinput_registry::AssetEntry;

/// CLI for inspecting and controlling the vinput daemon.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

/// Supported bootstrap commands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Print stable D-Bus names and methods.
    Protocol,
    /// Validate the bundled upstream-compatible default config.
    Config,
    /// Print registry URL resolution for the bundled config.
    Registry,
    /// Create a recognition JSON payload for tests/manual inspection.
    MockResult {
        /// Commit text for the payload.
        text: String,
    },
    /// Parse a status string and print the normalized wire value.
    Status {
        /// Status string such as idle, recording, inferring, postprocessing, or error.
        status: String,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Protocol => print_protocol(),
        Command::Config => validate_config(),
        Command::Registry => print_registry_summary(),
        Command::MockResult { text } => {
            let payload = RecognitionPayload::raw(text);
            println!("{}", payload.to_json_string()?);
            Ok(())
        }
        Command::Status { status } => {
            let status = ServiceStatus::parse_wire(&status)
                .with_context(|| format!("parse status `{status}`"))?;
            println!("{status}");
            Ok(())
        }
    }
}

fn print_protocol() -> anyhow::Result<()> {
    let value = serde_json::json!({
        "service_bus_name": dbus::SERVICE_BUS_NAME,
        "service_object_path": dbus::SERVICE_OBJECT_PATH,
        "service_interface": dbus::SERVICE_INTERFACE,
        "frontend_notifier_object_path": dbus::FRONTEND_NOTIFIER_OBJECT_PATH,
        "frontend_notifier_interface": dbus::FRONTEND_NOTIFIER_INTERFACE,
        "methods": [
            dbus::method::START_RECORDING,
            dbus::method::START_COMMAND_RECORDING,
            dbus::method::STOP_RECORDING,
            dbus::method::GET_STATUS,
            dbus::method::GET_ASR_BACKEND_STATE,
            dbus::method::RELOAD_ASR_BACKEND,
            dbus::method::START_ADAPTER,
            dbus::method::STOP_ADAPTER,
            dbus::method::NOTIFY,
        ],
        "signals": [
            dbus::signal::RECOGNITION_RESULT,
            dbus::signal::RECOGNITION_PARTIAL,
            dbus::signal::STATUS_CHANGED,
            dbus::signal::DAEMON_NOTIFICATION,
        ]
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn validate_config() -> anyhow::Result<()> {
    let config = VinputConfig::bundled_default().context("parse bundled config")?;
    config.validate().context("validate bundled config")?;
    let summary = serde_json::json!({
        "ok": true,
        "version": config.version,
        "active_scene": config.scenes.active_scene,
        "active_provider": config.asr.active_provider,
        "scene_count": config.scenes.definitions.len(),
        "provider_count": config.asr.providers.len(),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn print_registry_summary() -> anyhow::Result<()> {
    let config = VinputConfig::bundled_default().context("parse bundled config")?;
    let index_asset = AssetEntry {
        path: "index.json".to_owned(),
        sha256: None,
        size_bytes: None,
    };
    let summary = serde_json::json!({
        "base_url_count": config.registry.base_urls.len(),
        "index_urls": index_asset.resolved_urls(&config.registry),
    });
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
