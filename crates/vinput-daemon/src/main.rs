//! vinput daemon entrypoint.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use tracing::info;
use vinput_asr::{AsrBackendFactory, MockAsrBackend};
use vinput_audio::{
    AudioRecorder, CaptureTarget, CapturedAudio, MockAudioSource, PcmBuffer, PcmSpec,
};
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

    /// Audio recorder backend used for long-running daemon sessions.
    #[arg(long, value_enum, default_value_t = AudioBackendArg::Mock)]
    audio_backend: AudioBackendArg,

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

/// Audio recorder backend selection for long-running daemon sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum AudioBackendArg {
    /// Deterministic mock PCM source used by CI and non-desktop checks.
    Mock,
    /// Live `PipeWire` recorder. Requires the `pipewire-backend` Cargo feature.
    Pipewire,
}

const DEFAULT_FILE_AUDIO_FRAMES: usize = 4;

/// One-shot utility commands useful while bootstrapping the daemon.
#[derive(Debug, Subcommand)]
enum Command {
    /// Print the sanitized config summary as JSON.
    PrintConfig,
    /// Print configured ASR backend diagnostics as JSON.
    AsrState,
    /// Print configured command text adapter diagnostics as JSON.
    TextAdapters,
    /// Print configured audio capture diagnostics as JSON.
    AudioDevices,
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
    if (args.pcm16le.is_some() || args.wav.is_some()) && !(args.once || args.dbus) {
        bail!("--pcm16le and --wav are only supported together with --once or --dbus");
    }
    config.validate().context("validate daemon config")?;
    if let Some(command) = &args.command {
        match command {
            Command::PrintConfig => {
                println!("{}", serde_json::to_string_pretty(&config.summary())?);
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
            Command::AudioDevices => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&audio_devices_summary(&config)?)?
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

fn audio_devices_summary(config: &VinputConfig) -> anyhow::Result<serde_json::Value> {
    let capture_target = RuntimeState::configured_capture_target(config)?;
    let audio_report = enumerate_audio_devices();
    Ok(serde_json::json!({
        "ok": true,
        "capture_device": config.global.capture_device,
        "capture_target": capture_target_json(&capture_target),
        "backend": audio_devices_backend_name(),
        "live": audio_report.live,
        "devices": audio_report.devices,
        "enumeration_error": audio_report.enumeration_error,
    }))
}

struct AudioDevicesReport {
    devices: Vec<vinput_audio::AudioDeviceInfo>,
    live: bool,
    enumeration_error: Option<String>,
}

#[cfg(feature = "pipewire-backend")]
fn enumerate_audio_devices() -> AudioDevicesReport {
    use vinput_audio::AudioDeviceEnumerator as _;

    let mut enumerator = vinput_audio::pipewire_backend::PipeWireDeviceEnumerator;
    match enumerator
        .enumerate_audio_sources()
        .context("enumerate PipeWire audio sources")
    {
        Ok(devices) => AudioDevicesReport {
            devices,
            live: true,
            enumeration_error: None,
        },
        Err(error) => AudioDevicesReport {
            devices: Vec::new(),
            live: false,
            enumeration_error: Some(format!("{error:#}")),
        },
    }
}

#[cfg(not(feature = "pipewire-backend"))]
fn enumerate_audio_devices() -> AudioDevicesReport {
    AudioDevicesReport {
        devices: Vec::new(),
        live: false,
        enumeration_error: None,
    }
}

#[cfg(feature = "pipewire-backend")]
fn audio_devices_backend_name() -> &'static str {
    "pipewire"
}

#[cfg(not(feature = "pipewire-backend"))]
fn audio_devices_backend_name() -> &'static str {
    "unavailable"
}

fn capture_target_json(target: &CaptureTarget) -> serde_json::Value {
    match target {
        CaptureTarget::Default => serde_json::json!({"kind": "default"}),
        CaptureTarget::Object(value) => serde_json::json!({"kind": "object", "value": value}),
    }
}

fn build_runtime(args: &Args, config: VinputConfig) -> anyhow::Result<RuntimeState> {
    if let Some(audio_source) = input_audio_source(args)? {
        return if args.configured_backends {
            let backend = AsrBackendFactory::build_active(&config.asr)
                .context("build configured ASR backend")?;
            RuntimeState::with_configured_text(config, backend, Box::new(audio_source))
                .context("build configured runtime with file input")
        } else {
            let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
            RuntimeState::with_backends(config, Box::new(backend), Box::new(audio_source))
                .context("build mock runtime with file input")
        };
    }

    if let Some(audio_recorder) = selected_audio_recorder(args)? {
        return if args.configured_backends {
            let backend = AsrBackendFactory::build_active(&config.asr)
                .context("build configured ASR backend")?;
            RuntimeState::with_configured_audio_recorder(config, backend, audio_recorder)
                .context("build configured runtime with selected audio recorder")
        } else {
            let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
            RuntimeState::with_audio_recorder(config, Box::new(backend), audio_recorder)
                .context("build mock runtime with selected audio recorder")
        };
    }

    if args.configured_backends {
        RuntimeState::with_configured_backends(config).context("build configured runtime")
    } else {
        RuntimeState::new(config).context("build mock runtime")
    }
}

#[cfg_attr(feature = "pipewire-backend", allow(clippy::unnecessary_wraps))]
fn selected_audio_recorder(args: &Args) -> anyhow::Result<Option<Box<dyn AudioRecorder>>> {
    match args.audio_backend {
        AudioBackendArg::Mock => Ok(None),
        AudioBackendArg::Pipewire => {
            #[cfg(feature = "pipewire-backend")]
            {
                Ok(Some(Box::new(
                    vinput_audio::pipewire_backend::PipeWireAudioRecorder::new(),
                )))
            }
            #[cfg(not(feature = "pipewire-backend"))]
            {
                bail!("--audio-backend pipewire requires the pipewire-backend Cargo feature")
            }
        }
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
    Ok(file_audio_source(
        format!("pcm16le:{}", path.display()),
        pcm,
    ))
}

fn wav_audio_source(path: &Path) -> anyhow::Result<MockAudioSource> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read WAV file `{}`", path.display()))?;
    let pcm = PcmBuffer::from_wav_pcm16le_bytes(&bytes)
        .with_context(|| format!("decode WAV file `{}`", path.display()))?;
    Ok(file_audio_source(format!("wav:{}", path.display()), pcm))
}

fn file_audio_source(source_name: String, pcm: PcmBuffer) -> MockAudioSource {
    let frame = CapturedAudio::named(pcm, source_name);
    MockAudioSource::from_frames(vec![frame; DEFAULT_FILE_AUDIO_FRAMES])
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
