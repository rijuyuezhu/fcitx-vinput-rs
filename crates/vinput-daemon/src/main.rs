//! vinput daemon entrypoint.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use tracing::info;
use vinput_asr::{AsrBackendFactory, MockAsrBackend};
use vinput_audio::{CapturedAudio, MockAudioSource, PcmBuffer, PcmSpec};
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

    /// Use configured ASR and command text adapters instead of mock runtime backends.
    #[arg(long)]
    configured_backends: bool,

    /// Optional config JSON file. Omitted to use the bundled default config.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Raw signed 16-bit little-endian PCM file to use for `--once`.
    #[arg(long, value_name = "PATH")]
    pcm16le: Option<PathBuf>,

    /// Uncompressed RIFF/WAVE signed 16-bit PCM file to use for `--once`.
    #[arg(long, value_name = "PATH")]
    wav: Option<PathBuf>,

    /// Sample rate of `--pcm16le` input.
    #[arg(long, default_value_t = vinput_audio::DEFAULT_SAMPLE_RATE_HZ)]
    pcm_sample_rate: u32,

    /// Channel count of `--pcm16le` input.
    #[arg(long, default_value_t = vinput_audio::DEFAULT_CHANNELS)]
    pcm_channels: u16,

    /// Utility command.
    #[command(subcommand)]
    command: Option<Command>,
}

/// One-shot utility commands useful while bootstrapping the daemon.
#[derive(Debug, Subcommand)]
enum Command {
    /// Print the parsed config as normalized JSON.
    PrintConfig,
    /// Print configured ASR backend diagnostics as JSON.
    AsrState,
    /// Print configured command text adapter diagnostics as JSON.
    TextAdapters,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = load_config(args.config.as_ref())?;
    if args.pcm16le.is_some() && args.wav.is_some() {
        bail!("--pcm16le and --wav cannot be used together");
    }
    if (args.pcm16le.is_some() || args.wav.is_some()) && !args.once {
        bail!("--pcm16le and --wav are only supported together with --once");
    }
    config.validate().context("validate daemon config")?;
    if let Some(command) = &args.command {
        match command {
            Command::PrintConfig => {
                println!("{}", serde_json::to_string_pretty(&config)?);
            }
            Command::AsrState => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&RuntimeState::configured_asr_state(&config))?
                );
            }
            Command::TextAdapters => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&RuntimeState::configured_text_adapter_state(
                        &config
                    ))?
                );
            }
        }
        return Ok(());
    }

    let mut runtime = build_runtime(&args, config).context("initialize runtime")?;

    if args.once {
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
    } else if args.dbus {
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
    } else {
        info!(
            status = %runtime.status(),
            uptime_ms = runtime.uptime().as_millis(),
            "mock daemon initialized; pass --dbus to expose the legacy D-Bus ABI"
        );
        tokio::signal::ctrl_c().await.context("wait for ctrl-c")?;
    }

    Ok(())
}

fn build_runtime(args: &Args, config: VinputConfig) -> anyhow::Result<RuntimeState> {
    let Some(audio_source) = input_audio_source(args)? else {
        return if args.configured_backends {
            RuntimeState::with_configured_backends(config).context("build configured runtime")
        } else {
            RuntimeState::new(config).context("build mock runtime")
        };
    };

    if args.configured_backends {
        let backend =
            AsrBackendFactory::build_active(&config.asr).context("build configured ASR backend")?;
        RuntimeState::with_configured_text(config, backend, Box::new(audio_source))
            .context("build configured runtime with file input")
    } else {
        let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
        RuntimeState::with_backends(config, Box::new(backend), Box::new(audio_source))
            .context("build mock runtime with file input")
    }
}

fn input_audio_source(args: &Args) -> anyhow::Result<Option<MockAudioSource>> {
    if let Some(path) = args.pcm16le.as_deref() {
        return pcm16le_audio_source(path, args.pcm_sample_rate, args.pcm_channels).map(Some);
    }
    args.wav.as_deref().map(wav_audio_source).transpose()
}

fn pcm16le_audio_source(
    path: &Path,
    sample_rate_hz: u32,
    channels: u16,
) -> anyhow::Result<MockAudioSource> {
    let spec = PcmSpec {
        sample_rate_hz,
        channels,
    };
    let pcm = read_pcm16le(path, spec)?;
    file_audio_source(format!("pcm16le:{}", path.display()), pcm)
}

fn wav_audio_source(path: &Path) -> anyhow::Result<MockAudioSource> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read WAV file `{}`", path.display()))?;
    let pcm = PcmBuffer::from_wav_pcm16le_bytes(&bytes)
        .with_context(|| format!("decode WAV file `{}`", path.display()))?;
    file_audio_source(format!("wav:{}", path.display()), pcm)
}

fn file_audio_source(source_name: String, pcm: PcmBuffer) -> anyhow::Result<MockAudioSource> {
    let empty = PcmBuffer::with_spec(pcm.spec(), Vec::<i16>::new())
        .context("build empty warm-up audio frame")?;
    Ok(MockAudioSource::from_frames(vec![
        CapturedAudio::named(empty, source_name.clone()),
        CapturedAudio::named(pcm, source_name),
    ]))
}

fn read_pcm16le(path: &Path, spec: PcmSpec) -> anyhow::Result<PcmBuffer> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read PCM file `{}`", path.display()))?;
    PcmBuffer::from_pcm16le_bytes(spec, &bytes)
        .with_context(|| format!("decode PCM file `{}`", path.display()))
}

fn load_config(path: Option<&PathBuf>) -> anyhow::Result<VinputConfig> {
    match path {
        Some(path) => VinputConfig::from_json_file(path)
            .with_context(|| format!("load daemon config `{}`", path.display())),
        None => VinputConfig::bundled_default().context("load bundled default config"),
    }
}
