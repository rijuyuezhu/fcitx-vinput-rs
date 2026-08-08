//! vinpst daemon entrypoint.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use tracing::info;
use vinpst_asr::{AsrBackendFactory, MockAsrBackend, UnavailableAsrBackend};
use vinpst_audio::{
    AudioRecorder, CaptureTarget, CapturedAudio, MockAudioSource, PcmBuffer, PcmSpec,
};
use vinpst_config::VinpstConfig;
use vinpst_daemon::{
    RuntimeState, VinpstDbusService,
    remote::{RemoteTextServer, remote_text_settings},
};

/// Vinpst background service for recognition and text processing.
#[derive(Debug, Parser)]
#[command(version, about, disable_help_subcommand = true)]
#[allow(clippy::struct_excessive_bools)] // Clap models independent command-line switches as bools.
struct Args {
    /// Start without constructing an ASR backend.
    #[arg(long)]
    no_asr: bool,

    /// Print one mock recognition cycle and exit instead of running forever.
    #[arg(long, hide = true)]
    once: bool,

    /// Command-mode selected text for `--once`.
    #[arg(long, hide = true)]
    selected_text: Option<String>,

    /// Milliseconds to keep recording before stopping in `--once` mode.
    #[arg(long, default_value_t = 0, hide = true)]
    record_ms: u64,

    /// Serve the legacy D-Bus ABI on the session bus.
    #[arg(long, hide = true)]
    dbus: bool,

    /// Use configured ASR and command text adapters instead of mock runtime backends.
    #[arg(long, hide = true)]
    configured_backends: bool,

    /// Audio recorder backend used for long-running daemon sessions.
    #[arg(long, value_enum, hide = true)]
    audio_backend: Option<AudioBackendArg>,

    #[command(flatten)]
    upgrade: UpgradeArgs,

    /// Optional config JSON file. Omitted to use the bundled default config.
    #[arg(long, hide = true)]
    config: Option<PathBuf>,

    /// Installed model root exposed to the ASR selection menu.
    #[arg(long, value_name = "DIR", hide = true)]
    model_root: Option<PathBuf>,

    /// Raw signed 16-bit little-endian PCM file to use for `--once`.
    #[arg(long, value_name = "PATH", hide = true)]
    pcm16le: Option<PathBuf>,

    /// Uncompressed RIFF/WAVE signed 16-bit PCM file to use for `--once`.
    #[arg(long, value_name = "PATH", hide = true)]
    wav: Option<PathBuf>,

    /// Sample rate of `--pcm16le` input.
    #[arg(
        long,
        default_value_t = vinpst_audio::DEFAULT_SAMPLE_RATE_HZ,
        hide = true
    )]
    pcm_sample_rate: u32,

    /// Channel count of `--pcm16le` input.
    #[arg(long, default_value_t = vinpst_audio::DEFAULT_CHANNELS, hide = true)]
    pcm_channels: u16,

    /// Utility command.
    #[command(subcommand)]
    command: Option<Command>,
}

/// Package-upgrade lifecycle options.
#[derive(Debug, clap::Args)]
struct UpgradeArgs {
    /// Exit with failure after the running executable is replaced on disk.
    #[arg(long, hide = true)]
    exit_when_executable_replaced: bool,
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
const ASR_DISABLED_REASON: &str = "ASR disabled by command line.";

impl AudioBackendArg {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Pipewire => "pipewire",
        }
    }
}

impl Args {
    fn uses_implicit_service_defaults(&self) -> bool {
        self.command.is_none() && !self.once && !self.dbus
    }

    fn serves_dbus(&self) -> bool {
        self.dbus || self.uses_implicit_service_defaults()
    }

    fn uses_configured_backends(&self) -> bool {
        self.configured_backends || self.uses_implicit_service_defaults()
    }

    fn effective_audio_backend(&self) -> AudioBackendArg {
        self.audio_backend.unwrap_or_else(|| {
            if self.uses_implicit_service_defaults() {
                default_service_audio_backend()
            } else {
                AudioBackendArg::Mock
            }
        })
    }
}

#[cfg(feature = "pipewire-backend")]
const fn default_service_audio_backend() -> AudioBackendArg {
    AudioBackendArg::Pipewire
}

#[cfg(not(feature = "pipewire-backend"))]
const fn default_service_audio_backend() -> AudioBackendArg {
    AudioBackendArg::Mock
}

/// One-shot utility commands useful while bootstrapping the daemon.
#[derive(Debug, Subcommand)]
enum Command {
    /// Print the sanitized config summary as JSON.
    #[command(hide = true)]
    PrintConfig,
    /// Print configured ASR backend diagnostics as JSON.
    #[command(hide = true)]
    AsrState,
    /// Print configured command text adapter diagnostics as JSON.
    #[command(hide = true)]
    TextAdapters,
    /// Print configured audio capture diagnostics as JSON.
    #[command(hide = true)]
    AudioDevices,
    /// Build the selected runtime and print runtime status diagnostics as JSON.
    #[command(hide = true)]
    RuntimeStatus,
    /// Run only the legacy-compatible remote text HTTP/WebSocket service.
    #[command(hide = true)]
    RemoteTextServer {
        /// Listen address. The port is read from the active remote provider settings.
        #[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED))]
        bind: IpAddr,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    trace_startup("before args parse");
    let args = Args::parse();
    trace_startup("parsed args");
    let loaded_config = load_config(args.config.as_ref())?;
    let config = loaded_config.config;
    if args.pcm16le.is_some() && args.wav.is_some() {
        bail!("--pcm16le and --wav cannot be used together");
    }
    if (args.pcm16le.is_some() || args.wav.is_some()) && !(args.once || args.dbus) {
        bail!("--pcm16le and --wav are only supported together with --once or --dbus");
    }
    if args.record_ms > 0 && !args.once {
        bail!("--record-ms is only supported together with --once");
    }
    config.validate().context("validate daemon config")?;
    if let Some(command) = &args.command {
        handle_utility_command(command, &args, &config).await?;
        return Ok(());
    }

    let mut runtime = build_runtime(&args, config).context("initialize runtime")?;
    apply_runtime_flags(&args, &mut runtime);
    runtime.set_config_path(loaded_config.path);
    runtime.set_model_root(Some(resolve_model_root(args.model_root.as_ref())?));

    if args.once {
        if let Some(selected_text) = args.selected_text {
            runtime.start_command_recording(selected_text)?;
        } else {
            runtime.start_recording()?;
        }
        if args.record_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(args.record_ms)).await;
        }
        if let Some(partial) = runtime.partial_text() {
            info!(partial, "mock partial recognition");
        }
        let payload = runtime.stop_recording(None)?;
        println!("{}", payload.to_json_string()?);
    } else if args.serves_dbus() {
        trace_startup("enter dbus branch");
        let service = VinpstDbusService::new(runtime);
        service
            .start_remote_text_service()
            .await
            .context("start daemon-owned remote text service")?;
        let _connection = service
            .serve_on_session_bus()
            .await
            .context("serve vinpst D-Bus service")?;
        info!(
            bus = vinpst_protocol::dbus::SERVICE_BUS_NAME,
            object = vinpst_protocol::dbus::SERVICE_OBJECT_PATH,
            interface = vinpst_protocol::dbus::SERVICE_INTERFACE,
            "daemon D-Bus service is running"
        );
        trace_startup("dbus service owned; waiting for shutdown signal");
        let shutdown_reason =
            wait_for_shutdown_signal(args.upgrade.exit_when_executable_replaced).await?;
        service
            .shutdown_remote_text_service()
            .await
            .context("shutdown daemon-owned remote text service")?;
        if shutdown_reason == ShutdownReason::ExecutableReplaced {
            bail!("installed daemon executable was replaced");
        }
    } else {
        info!(
            status = %runtime.status(),
            uptime_ms = runtime.uptime().as_millis(),
            "mock daemon initialized; pass --dbus to expose the legacy D-Bus ABI"
        );
        if wait_for_shutdown_signal(args.upgrade.exit_when_executable_replaced).await?
            == ShutdownReason::ExecutableReplaced
        {
            bail!("installed daemon executable was replaced");
        }
    }

    Ok(())
}

async fn handle_utility_command(
    command: &Command,
    args: &Args,
    config: &VinpstConfig,
) -> anyhow::Result<()> {
    match command {
        Command::PrintConfig => {
            println!("{}", serde_json::to_string_pretty(&config.summary())?);
        }
        Command::AsrState => {
            println!(
                "{}",
                serde_json::to_string_pretty(&RuntimeState::configured_asr_state(config))?
            );
        }
        Command::TextAdapters => {
            println!(
                "{}",
                serde_json::to_string_pretty(&RuntimeState::configured_text_adapter_state(config))?
            );
        }
        Command::AudioDevices => {
            println!(
                "{}",
                serde_json::to_string_pretty(&audio_devices_summary(config)?)?
            );
        }
        Command::RuntimeStatus => {
            let mut runtime =
                build_runtime(args, config.clone()).context("initialize runtime status")?;
            apply_runtime_flags(args, &mut runtime);
            println!(
                "{}",
                serde_json::to_string_pretty(&runtime_status_summary(&runtime, config, args)?)?
            );
        }
        Command::RemoteTextServer { bind } => {
            run_remote_text_server(config, *bind).await?;
        }
    }
    Ok(())
}

async fn run_remote_text_server(config: &VinpstConfig, bind: IpAddr) -> anyhow::Result<()> {
    let settings = remote_text_settings(config)
        .context("resolve remote text service settings")?
        .context(
            "remote text service requires active provider `provider.vinpst.remote.streaming`",
        )?;
    let port = settings.port;
    let server = RemoteTextServer::bind(settings, SocketAddr::new(bind, port))
        .await
        .context("start remote text service")?;
    info!(
        address = %server.local_addr(),
        "remote text HTTP/WebSocket service is running"
    );
    tokio::signal::ctrl_c()
        .await
        .context("wait for remote text service shutdown signal")?;
    server
        .shutdown()
        .await
        .context("shutdown remote text service")
}

#[cfg(test)]
mod argument_semantics_tests {
    use clap::Parser;

    use super::{Args, AudioBackendArg};

    #[test]
    fn direct_launch_uses_service_defaults() {
        let args = Args::try_parse_from(["vinpst-daemon"]).expect("parse direct daemon launch");

        assert!(args.serves_dbus());
        assert!(args.uses_configured_backends());
        #[cfg(feature = "pipewire-backend")]
        assert_eq!(args.effective_audio_backend(), AudioBackendArg::Pipewire);
        #[cfg(not(feature = "pipewire-backend"))]
        assert_eq!(args.effective_audio_backend(), AudioBackendArg::Mock);
    }

    #[test]
    fn no_asr_keeps_direct_launch_in_service_mode() {
        let args = Args::try_parse_from(["vinpst-daemon", "--no-asr"])
            .expect("parse daemon launch with ASR disabled");

        assert!(args.no_asr);
        assert!(args.serves_dbus());
        assert!(args.uses_configured_backends());
    }

    #[test]
    fn explicit_test_modes_keep_deterministic_defaults() {
        let once =
            Args::try_parse_from(["vinpst-daemon", "--once"]).expect("parse one-shot daemon mode");
        assert!(!once.serves_dbus());
        assert!(!once.uses_configured_backends());
        assert_eq!(once.effective_audio_backend(), AudioBackendArg::Mock);

        let explicit_dbus = Args::try_parse_from(["vinpst-daemon", "--dbus"])
            .expect("parse explicit D-Bus test mode");
        assert!(explicit_dbus.serves_dbus());
        assert!(!explicit_dbus.uses_configured_backends());
        assert_eq!(
            explicit_dbus.effective_audio_backend(),
            AudioBackendArg::Mock
        );
    }
}

fn trace_startup(message: &str) {
    if std::env::var_os("VINPST_DAEMON_TRACE_STARTUP").is_some() {
        eprintln!("vinpst-daemon-startup: {message}");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShutdownReason {
    Signal,
    ExecutableReplaced,
}

#[cfg(unix)]
async fn wait_for_shutdown_signal(
    exit_when_executable_replaced: bool,
) -> anyhow::Result<ShutdownReason> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("install SIGTERM handler")?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result.context("wait for Ctrl-C")?;
            Ok(ShutdownReason::Signal)
        },
        _ = terminate.recv() => Ok(ShutdownReason::Signal),
        result = wait_for_executable_replacement(exit_when_executable_replaced) => {
            result?;
            Ok(ShutdownReason::ExecutableReplaced)
        },
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal(
    exit_when_executable_replaced: bool,
) -> anyhow::Result<ShutdownReason> {
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result.context("wait for Ctrl-C")?;
            Ok(ShutdownReason::Signal)
        },
        result = wait_for_executable_replacement(exit_when_executable_replaced) => {
            result?;
            Ok(ShutdownReason::ExecutableReplaced)
        },
    }
}

async fn wait_for_executable_replacement(enabled: bool) -> anyhow::Result<()> {
    if !enabled {
        std::future::pending::<()>().await;
        unreachable!("pending executable replacement watcher returned");
    }

    #[cfg(unix)]
    {
        let executable = std::env::current_exe().context("resolve running daemon executable")?;
        wait_for_executable_replacement_at(&executable, std::time::Duration::from_secs(1)).await
    }

    #[cfg(not(unix))]
    anyhow::bail!("executable replacement watching is only supported on Unix")
}

#[cfg(unix)]
async fn wait_for_executable_replacement_at(
    executable: &Path,
    interval: std::time::Duration,
) -> anyhow::Result<()> {
    loop {
        tokio::time::sleep(interval).await;
        match running_executable_was_replaced(executable) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => tracing::debug!(
                executable = %executable.display(),
                %error,
                "failed to inspect daemon executable replacement state"
            ),
        }
    }
}

#[cfg(unix)]
fn running_executable_was_replaced(executable: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let running = std::fs::metadata("/proc/self/exe")?;
    let installed = match std::fs::metadata(executable) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(running.dev() != installed.dev() || running.ino() != installed.ino())
}

fn runtime_status_summary(
    runtime: &RuntimeState,
    config: &VinpstConfig,
    args: &Args,
) -> anyhow::Result<serde_json::Value> {
    let mut summary = runtime.runtime_status_json();
    let object = summary
        .as_object_mut()
        .context("runtime status summary should be a JSON object")?;
    object.insert(
        "configured_backends".to_owned(),
        serde_json::json!(args.uses_configured_backends()),
    );
    object.insert(
        "audio_backend".to_owned(),
        serde_json::json!(args.effective_audio_backend().as_str()),
    );
    object.insert("audio".to_owned(), audio_devices_summary(config)?);
    Ok(summary)
}

fn audio_devices_summary(config: &VinpstConfig) -> anyhow::Result<serde_json::Value> {
    let capture_target = RuntimeState::configured_capture_target(config)?;
    let audio_report = enumerate_audio_devices();
    Ok(serde_json::json!({
        "ok": true,
        "capture_device": config.global.capture_device,
        "capture_target": capture_target_json(&capture_target),
        "recording": recording_summary(&capture_target),
        "backend": audio_devices_backend_name(),
        "live": audio_report.live,
        "devices": audio_report.devices,
        "enumeration_error": audio_report.enumeration_error,
    }))
}

struct AudioDevicesReport {
    devices: Vec<vinpst_audio::AudioDeviceInfo>,
    live: bool,
    enumeration_error: Option<String>,
}

#[cfg(feature = "pipewire-backend")]
fn enumerate_audio_devices() -> AudioDevicesReport {
    use vinpst_audio::AudioDeviceEnumerator as _;

    let mut enumerator = vinpst_audio::pipewire_backend::PipeWireDeviceEnumerator;
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

#[cfg(feature = "pipewire-backend")]
fn recording_summary(target: &CaptureTarget) -> serde_json::Value {
    let config = vinpst_audio::pipewire_backend::PipeWireStreamConfig::for_target(target.clone());
    serde_json::json!({
        "available": true,
        "status": "live-worker",
        "target": capture_target_json(&config.target),
        "format": config.format,
        "sample_rate_hz": config.pcm_spec.sample_rate_hz,
        "channels": config.pcm_spec.channels,
    })
}

#[cfg(not(feature = "pipewire-backend"))]
fn recording_summary(target: &CaptureTarget) -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "status": "feature-disabled",
        "target": capture_target_json(target),
    })
}

fn build_runtime(args: &Args, config: VinpstConfig) -> anyhow::Result<RuntimeState> {
    if let Some(audio_source) = input_audio_source(args)? {
        return if args.no_asr {
            let backend = unavailable_asr_backend();
            if args.uses_configured_backends() {
                RuntimeState::with_configured_text(config, backend, Box::new(audio_source))
                    .context("build configured runtime with ASR disabled and file input")
            } else {
                RuntimeState::with_backends(config, backend, Box::new(audio_source))
                    .context("build runtime with ASR disabled and file input")
            }
        } else if args.uses_configured_backends() {
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
        return if args.no_asr {
            let backend = unavailable_asr_backend();
            if args.uses_configured_backends() {
                RuntimeState::with_configured_audio_recorder(config, backend, audio_recorder)
                    .context(
                        "build configured runtime with ASR disabled and selected audio recorder",
                    )
            } else {
                RuntimeState::with_audio_recorder(config, backend, audio_recorder)
                    .context("build runtime with ASR disabled and selected audio recorder")
            }
        } else if args.uses_configured_backends() {
            RuntimeState::with_configured_audio_recorder_or_unavailable(config, audio_recorder)
                .context("build configured runtime with selected audio recorder")
        } else {
            let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
            RuntimeState::with_audio_recorder(config, Box::new(backend), audio_recorder)
                .context("build mock runtime with selected audio recorder")
        };
    }

    if args.no_asr {
        let backend = unavailable_asr_backend();
        if args.uses_configured_backends() {
            RuntimeState::with_configured_text(
                config,
                backend,
                Box::new(MockAudioSource::from_frames(Vec::new())),
            )
            .context("build configured runtime with ASR disabled")
        } else {
            RuntimeState::with_asr_backend(config, backend)
                .context("build runtime with ASR disabled")
        }
    } else if args.uses_configured_backends() {
        RuntimeState::with_configured_backends_or_unavailable(config)
            .context("build configured runtime")
    } else {
        RuntimeState::new(config).context("build mock runtime")
    }
}

fn unavailable_asr_backend() -> Box<dyn vinpst_asr::AsrBackend> {
    Box::new(UnavailableAsrBackend::new(ASR_DISABLED_REASON))
}

fn apply_runtime_flags(args: &Args, runtime: &mut RuntimeState) {
    if args.no_asr {
        runtime.disable_asr(ASR_DISABLED_REASON);
    }
}

#[cfg_attr(feature = "pipewire-backend", allow(clippy::unnecessary_wraps))]
fn selected_audio_recorder(args: &Args) -> anyhow::Result<Option<Box<dyn AudioRecorder>>> {
    match args.effective_audio_backend() {
        AudioBackendArg::Mock => Ok(None),
        AudioBackendArg::Pipewire => {
            #[cfg(feature = "pipewire-backend")]
            {
                Ok(Some(Box::new(
                    vinpst_audio::pipewire_backend::PipeWireAudioRecorder::new(),
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

fn resolve_model_root(explicit: Option<&PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.clone());
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("fcitx-vinpst").join("models"));
    }
    let home = std::env::var_os("HOME")
        .context("resolve installed model root: HOME is unset and XDG_DATA_HOME is unset")?;
    Ok(PathBuf::from(home)
        .join(".local/share")
        .join("fcitx-vinpst")
        .join("models"))
}

struct LoadedConfig {
    config: VinpstConfig,
    path: Option<PathBuf>,
}

fn load_config(path: Option<&PathBuf>) -> anyhow::Result<LoadedConfig> {
    if let Some(path) = path {
        return Ok(LoadedConfig {
            config: VinpstConfig::from_json_file(path)
                .with_context(|| format!("load daemon config `{}`", path.display()))?,
            path: Some(path.clone()),
        });
    }

    let default_path = default_user_config_path()?;
    if default_path.is_file() {
        return Ok(LoadedConfig {
            config: VinpstConfig::from_json_file(&default_path).with_context(|| {
                format!("load default daemon config `{}`", default_path.display())
            })?,
            path: Some(default_path),
        });
    }

    Ok(LoadedConfig {
        config: VinpstConfig::bundled_default().context("load bundled default config")?,
        path: None,
    })
}

fn default_user_config_path() -> anyhow::Result<PathBuf> {
    let config_home = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => {
            let home = std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .context("resolve default daemon config path: HOME is unset")?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(config_home.join("fcitx-vinpst").join("config.json"))
}

#[cfg(all(test, unix))]
mod executable_replacement_tests {
    use super::*;

    fn replace_with_new_inode(path: &Path, source: &Path) {
        let replacement = path.with_extension("replacement");
        std::fs::copy(source, &replacement).expect("copy replacement executable");
        std::fs::rename(replacement, path).expect("publish replacement executable");
    }

    #[test]
    fn executable_identity_detects_atomic_replacement() {
        let current = std::env::current_exe().expect("resolve test executable");
        let directory = tempfile::tempdir_in(current.parent().expect("test executable parent"))
            .expect("create executable replacement directory");
        let installed = directory.path().join("vinpst-daemon");
        std::fs::hard_link(&current, &installed).expect("link running executable");

        assert!(!running_executable_was_replaced(&installed).unwrap());
        replace_with_new_inode(&installed, &current);
        assert!(running_executable_was_replaced(&installed).unwrap());
    }

    #[tokio::test]
    async fn executable_watcher_returns_after_atomic_replacement() {
        let current = std::env::current_exe().expect("resolve test executable");
        let directory = tempfile::tempdir_in(current.parent().expect("test executable parent"))
            .expect("create executable watcher directory");
        let installed = directory.path().join("vinpst-daemon");
        std::fs::hard_link(&current, &installed).expect("link running executable");

        let watched_path = installed.clone();
        let watcher = tokio::spawn(async move {
            wait_for_executable_replacement_at(&watched_path, std::time::Duration::from_millis(5))
                .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        replace_with_new_inode(&installed, &current);

        tokio::time::timeout(std::time::Duration::from_secs(1), watcher)
            .await
            .expect("watcher timed out")
            .expect("watcher task panicked")
            .expect("watcher failed");
    }
}
