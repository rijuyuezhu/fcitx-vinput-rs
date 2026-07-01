//! Feature-gated `PipeWire` backend scaffolding.
//!
//! Device enumeration is live when a user `PipeWire` session is available.
//! The recorder owns a live worker thread that creates the `PipeWire` stream,
//! captures pinned `S16LE` PCM chunks, and returns the accumulated buffer when
//! stopped.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc,
    thread,
    time::Duration,
};

use crate::{
    AudioChunkCallback, AudioDeviceEnumerator, AudioDeviceInfo, AudioError, AudioRecorder,
    CaptureTarget, CapturedAudio, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ, PcmBuffer, PcmSpec,
};

const MEDIA_CLASS_AUDIO_SOURCE: &str = "Audio/Source";
const PW_KEY_MEDIA_CLASS: &str = "media.class";
const PW_KEY_NODE_NAME: &str = "node.name";
const PW_KEY_NODE_DESCRIPTION: &str = "node.description";
const RECORDING_WORKER_ITERATE_MS: u64 = 50;

/// `PipeWire` stream sample format requested by the future live recorder.
pub const RECORDING_FORMAT: &str = "S16LE";

/// `PipeWire` stream sample rate requested by the future live recorder.
pub const RECORDING_SAMPLE_RATE_HZ: u32 = DEFAULT_SAMPLE_RATE_HZ;

/// `PipeWire` stream channel count requested by the future live recorder.
pub const RECORDING_CHANNELS: u16 = DEFAULT_CHANNELS;

/// Returns the PCM spec that future `PipeWire` capture must deliver to ASR.
#[must_use]
pub const fn recording_pcm_spec() -> PcmSpec {
    PcmSpec {
        sample_rate_hz: RECORDING_SAMPLE_RATE_HZ,
        channels: RECORDING_CHANNELS,
    }
}

/// Planned `PipeWire` stream settings for a capture target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeWireStreamConfig {
    /// Capture target selected by config or UI.
    pub target: CaptureTarget,
    /// Signed PCM format requested from `PipeWire`.
    pub format: &'static str,
    /// PCM layout delivered to ASR.
    pub pcm_spec: PcmSpec,
}

impl PipeWireStreamConfig {
    /// Builds the default live stream configuration for a target.
    #[must_use]
    pub fn for_target(target: CaptureTarget) -> Self {
        Self {
            target,
            format: RECORDING_FORMAT,
            pcm_spec: recording_pcm_spec(),
        }
    }
}

/// Enables live `PipeWire` source enumeration tests when set in the environment.
pub const TEST_PIPEWIRE_ENUMERATE_ENV: &str = "VINPUT_TEST_PIPEWIRE_ENUMERATE";

/// Enables live `PipeWire` client context tests when set in the environment.
pub const TEST_PIPEWIRE_CONTEXT_ENV: &str = "VINPUT_TEST_PIPEWIRE_CONTEXT";

/// Enables live `PipeWire` recorder tests when set in the environment.
pub const TEST_PIPEWIRE_RECORD_ENV: &str = "VINPUT_TEST_PIPEWIRE_RECORD";

/// Returns whether a `PipeWire` live integration test gate is explicitly enabled.
#[must_use]
pub fn live_test_enabled(env_name: &str) -> bool {
    std::env::var_os(env_name).is_some()
}

/// Initialize the `PipeWire` client library.
pub fn initialize() {
    pipewire::init();
}

/// Probe that the optional `PipeWire` bindings link and initialize.
pub fn probe_client_linkage() {
    initialize();
}

/// Create the minimal `PipeWire` main loop and context objects.
///
/// This requires a usable `PipeWire` client configuration and is therefore
/// intended for explicit local integration checks, not default CI.
pub fn probe_client_context() -> Result<(), AudioError> {
    probe_client_linkage();
    let mainloop = pipewire::main_loop::MainLoopBox::new(None).map_err(pipewire_error)?;
    let _context =
        pipewire::context::ContextBox::new(mainloop.loop_(), None).map_err(pipewire_error)?;
    Ok(())
}

/// Convert a `PipeWire` registry global into audio-source metadata.
pub fn audio_device_from_global<P>(
    global: &pipewire::registry::GlobalObject<P>,
) -> Option<AudioDeviceInfo>
where
    P: AsRef<pipewire::spa::utils::dict::DictRef>,
{
    if global.type_ != pipewire::types::ObjectType::Node {
        return None;
    }
    let props = global.props.as_ref()?.as_ref();
    if props.get(PW_KEY_MEDIA_CLASS) != Some(MEDIA_CLASS_AUDIO_SOURCE) {
        return None;
    }
    let name = props.get(PW_KEY_NODE_NAME).unwrap_or_default();
    let description = props.get(PW_KEY_NODE_DESCRIPTION).unwrap_or_default();
    Some(AudioDeviceInfo::new(global.id, name, description))
}

/// Feature-gated `PipeWire` device enumerator.
#[derive(Debug, Clone, Copy, Default)]
pub struct PipeWireDeviceEnumerator;

impl AudioDeviceEnumerator for PipeWireDeviceEnumerator {
    fn enumerate_audio_sources(&mut self) -> Result<Vec<AudioDeviceInfo>, AudioError> {
        enumerate_audio_sources()
    }
}

/// Feature-gated `PipeWire` recorder skeleton.
pub struct PipeWireAudioRecorder {
    stream_config: PipeWireStreamConfig,
    chunk_callback: Option<AudioChunkCallback>,
    worker: Option<PipeWireRecordingWorker>,
}

struct PipeWireRecordingWorker {
    stop_tx: mpsc::Sender<WorkerCommand>,
    join: thread::JoinHandle<Result<CapturedAudio, AudioError>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerCommand {
    Stop,
    Cancel,
}

impl PipeWireAudioRecorder {
    /// Creates a recorder placeholder for future live `PipeWire` capture.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stream_config: PipeWireStreamConfig::for_target(CaptureTarget::default()),
            chunk_callback: None,
            worker: None,
        }
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub fn target(&self) -> &CaptureTarget {
        &self.stream_config.target
    }

    /// Returns the planned stream configuration for the next live capture.
    #[must_use]
    pub fn stream_config(&self) -> &PipeWireStreamConfig {
        &self.stream_config
    }
}

impl Default for PipeWireAudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioRecorder for PipeWireAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        if self.worker.is_some() {
            return Err(AudioError::RecorderAlreadyRecording);
        }
        self.stream_config = PipeWireStreamConfig::for_target(target);
        let config = self.stream_config.clone();
        let callback = self.chunk_callback.take();
        let (stop_tx, stop_rx) = mpsc::channel();
        let (setup_tx, setup_rx) = mpsc::channel();
        let join =
            thread::spawn(move || run_recording_worker(&config, callback, &stop_rx, &setup_tx));

        match setup_rx.recv() {
            Ok(Ok(())) => {
                self.worker = Some(PipeWireRecordingWorker { stop_tx, join });
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = join.join();
                Err(AudioError::RecordingBackendUnavailable(format!(
                    "PipeWire recorder worker exited before setup: {error}"
                )))
            }
        }
    }

    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>) {
        self.chunk_callback = callback;
    }

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        let worker = self.worker.take().ok_or(AudioError::RecorderNotRecording)?;
        stop_recording_worker(worker, WorkerCommand::Stop)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        if let Some(worker) = self.worker.take() {
            let _captured = stop_recording_worker(worker, WorkerCommand::Cancel)?;
        }
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.worker.is_some()
    }
}

/// Enumerate available `PipeWire` audio sources.
pub fn enumerate_audio_sources() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    probe_client_linkage();

    let mainloop = pipewire::main_loop::MainLoopRc::new(None).map_err(pipewire_error)?;
    let context = pipewire::context::ContextRc::new(&mainloop, None).map_err(pipewire_error)?;
    let core = context.connect_rc(None).map_err(pipewire_error)?;
    let registry = core.get_registry_rc().map_err(pipewire_error)?;

    let devices = Rc::new(RefCell::new(Vec::new()));
    let devices_for_registry = Rc::clone(&devices);
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            if let Some(device) = audio_device_from_global(global) {
                devices_for_registry.borrow_mut().push(device);
            }
        })
        .register();

    let pending_sync = Rc::new(Cell::new(None));
    let pending_sync_for_core = Rc::clone(&pending_sync);
    let mainloop_for_core = mainloop.clone();
    let _core_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id == pipewire::core::PW_ID_CORE && pending_sync_for_core.get() == Some(seq.seq()) {
                mainloop_for_core.quit();
            }
        })
        .register();

    let sync = core.sync(0).map_err(pipewire_error)?;
    pending_sync.set(Some(sync.seq()));
    mainloop.run();

    let result = devices.borrow().clone();
    Ok(result)
}

fn stop_recording_worker(
    worker: PipeWireRecordingWorker,
    command: WorkerCommand,
) -> Result<CapturedAudio, AudioError> {
    let _ = worker.stop_tx.send(command);
    match worker.join.join() {
        Ok(result) => result,
        Err(_) => Err(AudioError::RecordingBackendUnavailable(
            "PipeWire recorder worker panicked".to_owned(),
        )),
    }
}

fn run_recording_worker(
    config: &PipeWireStreamConfig,
    callback: Option<AudioChunkCallback>,
    stop_rx: &mpsc::Receiver<WorkerCommand>,
    setup_tx: &mpsc::Sender<Result<(), AudioError>>,
) -> Result<CapturedAudio, AudioError> {
    match run_recording_worker_inner(config, callback, stop_rx, setup_tx) {
        Ok(captured) => Ok(captured),
        Err(error) => {
            let _ = setup_tx.send(Err(AudioError::RecordingBackendUnavailable(
                error.to_string(),
            )));
            Err(error)
        }
    }
}

fn run_recording_worker_inner(
    config: &PipeWireStreamConfig,
    callback: Option<AudioChunkCallback>,
    stop_rx: &mpsc::Receiver<WorkerCommand>,
    setup_tx: &mpsc::Sender<Result<(), AudioError>>,
) -> Result<CapturedAudio, AudioError> {
    use pipewire::{properties::properties, spa};

    probe_client_linkage();
    let mainloop = pipewire::main_loop::MainLoopRc::new(None)
        .map_err(|error| pipewire_recording_error(config, error))?;
    let context = pipewire::context::ContextRc::new(&mainloop, None)
        .map_err(|error| pipewire_recording_error(config, error))?;
    let core = context
        .connect_rc(None)
        .map_err(|error| pipewire_recording_error(config, error))?;

    let mut props = properties! {
        *pipewire::keys::MEDIA_TYPE => "Audio",
        *pipewire::keys::MEDIA_CATEGORY => "Capture",
        *pipewire::keys::MEDIA_ROLE => "Speech",
    };
    if let Some(target) = config.target.target_object() {
        props.insert("target.object", target.to_owned());
    }

    let stream = pipewire::stream::StreamBox::new(&core, "vinput-capture", props)
        .map_err(|error| pipewire_recording_error(config, error))?;
    let samples = Rc::new(RefCell::new(Vec::new()));
    let callback = Rc::new(RefCell::new(callback));
    let samples_for_process = Rc::clone(&samples);
    let callback_for_process = Rc::clone(&callback);
    let pcm_spec = config.pcm_spec;

    let _listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, ()| {
            capture_stream_buffer(
                stream,
                pcm_spec,
                &samples_for_process,
                &callback_for_process,
            );
        })
        .register()
        .map_err(|error| pipewire_recording_error(config, error))?;

    let param_values = pipewire_recording_param_values(config)?;
    let params = [spa::pod::Pod::from_bytes(&param_values).ok_or_else(|| {
        pipewire_recording_error(config, "serialize PipeWire recording stream format")
    })?];
    let mut param_refs = [params[0]];
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pipewire::stream::StreamFlags::AUTOCONNECT
                | pipewire::stream::StreamFlags::MAP_BUFFERS
                | pipewire::stream::StreamFlags::RT_PROCESS,
            &mut param_refs,
        )
        .map_err(|error| pipewire_recording_error(config, error))?;

    let _ = setup_tx.send(Ok(()));
    let command =
        loop {
            match stop_rx.try_recv() {
                Ok(command) => break command,
                Err(mpsc::TryRecvError::Empty) => {
                    mainloop.loop_().iterate(pipewire::loop_::Timeout::Finite(
                        Duration::from_millis(RECORDING_WORKER_ITERATE_MS),
                    ));
                }
                Err(mpsc::TryRecvError::Disconnected) => break WorkerCommand::Cancel,
            }
        };

    let _ = stream.disconnect();
    let pcm = PcmBuffer::with_spec(config.pcm_spec, samples.borrow().clone())?;
    let captured = CapturedAudio::named(pcm, pipewire_capture_source_name(config));
    match command {
        WorkerCommand::Stop | WorkerCommand::Cancel => Ok(captured),
    }
}

fn capture_stream_buffer(
    stream: &pipewire::stream::Stream,
    pcm_spec: PcmSpec,
    samples: &Rc<RefCell<Vec<i16>>>,
    callback: &Rc<RefCell<Option<AudioChunkCallback>>>,
) {
    let Some(mut buffer) = stream.dequeue_buffer() else {
        return;
    };
    let Some(data) = buffer.datas_mut().first_mut() else {
        return;
    };
    let chunk = data.chunk();
    let offset = chunk.offset() as usize;
    let size = chunk.size() as usize;
    let Some(bytes) = data.data() else {
        return;
    };
    let Some(end) = offset.checked_add(size) else {
        return;
    };
    let Some(bytes) = bytes.get(offset..end) else {
        return;
    };
    let chunk_samples = bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    if chunk_samples.is_empty() {
        return;
    }
    samples.borrow_mut().extend_from_slice(&chunk_samples);
    if let Some(callback) = callback.borrow_mut().as_mut()
        && let Ok(pcm) = PcmBuffer::with_spec(pcm_spec, chunk_samples)
    {
        callback(&pcm);
    }
}

fn pipewire_recording_param_values(config: &PipeWireStreamConfig) -> Result<Vec<u8>, AudioError> {
    use pipewire::spa;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::S16LE);
    audio_info.set_rate(config.pcm_spec.sample_rate_hz);
    audio_info.set_channels(u32::from(config.pcm_spec.channels));
    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map(|serialized| serialized.0.into_inner())
    .map_err(|error| pipewire_recording_error(config, error))
}

fn pipewire_capture_source_name(config: &PipeWireStreamConfig) -> String {
    match &config.target {
        CaptureTarget::Default => "pipewire:default".to_owned(),
        CaptureTarget::Object(value) => format!("pipewire:{value}"),
    }
}

fn pipewire_recording_error(
    config: &PipeWireStreamConfig,
    error: impl std::fmt::Display,
) -> AudioError {
    let target = match &config.target {
        CaptureTarget::Default => "default".to_owned(),
        CaptureTarget::Object(value) => format!("object `{value}`"),
    };
    AudioError::RecordingBackendUnavailable(format!(
        "PipeWire recorder stream setup failed \
         (target: {target}, format: {}, sample_rate_hz: {}, channels: {}): {error}",
        config.format, config.pcm_spec.sample_rate_hz, config.pcm_spec.channels
    ))
}

fn pipewire_error(error: impl std::fmt::Display) -> AudioError {
    AudioError::DeviceEnumerationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use pipewire::spa::static_dict;

    fn global_with_props(
        id: u32,
        type_: pipewire::types::ObjectType,
        props: Option<&pipewire::spa::utils::dict::DictRef>,
    ) -> pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef> {
        pipewire::registry::GlobalObject {
            id,
            permissions: pipewire::permissions::PermissionFlags::empty(),
            type_,
            version: 0,
            props,
        }
    }

    #[test]
    fn pipewire_global_maps_audio_source_metadata() {
        let props = static_dict! {
            "media.class" => "Audio/Source",
            "node.name" => "alsa_input.usb-mic",
            "node.description" => "USB Microphone",
        };
        let global = global_with_props(42, pipewire::types::ObjectType::Node, Some(&props));

        let device = super::audio_device_from_global(&global).unwrap();
        assert_eq!(device.id, 42);
        assert_eq!(device.name, "alsa_input.usb-mic");
        assert_eq!(device.description, "USB Microphone");
    }

    #[test]
    fn pipewire_global_ignores_non_source_nodes() {
        let sink_props = static_dict! {
            "media.class" => "Audio/Sink",
            "node.name" => "alsa_output.speaker",
            "node.description" => "Speakers",
        };
        let source_props = static_dict! {
            "media.class" => "Audio/Source",
            "node.name" => "alsa_input.usb-mic",
        };
        let sink = global_with_props(7, pipewire::types::ObjectType::Node, Some(&sink_props));
        let device = global_with_props(8, pipewire::types::ObjectType::Device, Some(&source_props));
        let missing_props = global_with_props(9, pipewire::types::ObjectType::Node, None);

        assert_eq!(super::audio_device_from_global(&sink), None);
        assert_eq!(super::audio_device_from_global(&device), None);
        assert_eq!(super::audio_device_from_global(&missing_props), None);
    }

    #[test]
    fn pipewire_global_defaults_missing_name_fields() {
        let props = static_dict! {
            "media.class" => "Audio/Source",
        };
        let global = global_with_props(13, pipewire::types::ObjectType::Node, Some(&props));

        let device = super::audio_device_from_global(&global).unwrap();
        assert_eq!(device.id, 13);
        assert_eq!(device.name, "");
        assert_eq!(device.description, "");
    }

    #[test]
    fn pipewire_probe_initializes_client_library() {
        super::probe_client_linkage();
    }

    #[test]
    fn pipewire_live_test_env_gates_are_explicit() {
        assert_eq!(
            super::TEST_PIPEWIRE_ENUMERATE_ENV,
            "VINPUT_TEST_PIPEWIRE_ENUMERATE"
        );
        assert_eq!(
            super::TEST_PIPEWIRE_CONTEXT_ENV,
            "VINPUT_TEST_PIPEWIRE_CONTEXT"
        );
        assert_eq!(
            super::TEST_PIPEWIRE_RECORD_ENV,
            "VINPUT_TEST_PIPEWIRE_RECORD"
        );
        assert!(!super::TEST_PIPEWIRE_ENUMERATE_ENV.is_empty());
        assert!(!super::TEST_PIPEWIRE_CONTEXT_ENV.is_empty());
        assert!(!super::TEST_PIPEWIRE_RECORD_ENV.is_empty());
    }

    #[test]
    fn pipewire_recording_pcm_policy_matches_asr_default() {
        assert_eq!(super::RECORDING_FORMAT, "S16LE");
        assert_eq!(
            super::RECORDING_SAMPLE_RATE_HZ,
            super::DEFAULT_SAMPLE_RATE_HZ
        );
        assert_eq!(super::RECORDING_CHANNELS, super::DEFAULT_CHANNELS);
        assert_eq!(
            super::recording_pcm_spec(),
            super::PcmSpec::mono_i16(super::DEFAULT_SAMPLE_RATE_HZ)
        );
    }

    #[test]
    fn pipewire_stream_config_preserves_target_and_pcm_policy() {
        let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
            "alsa_input.test".to_owned(),
        ));

        assert_eq!(
            config.target,
            super::CaptureTarget::Object("alsa_input.test".to_owned())
        );
        assert_eq!(config.format, super::RECORDING_FORMAT);
        assert_eq!(config.pcm_spec, super::recording_pcm_spec());
    }

    #[test]
    fn pipewire_recorder_tracks_idle_state_and_stream_plan() {
        let mut recorder = super::PipeWireAudioRecorder::new();

        super::AudioRecorder::set_chunk_callback(&mut recorder, None);

        assert_eq!(recorder.target(), &super::CaptureTarget::Default);
        assert_eq!(
            recorder.stream_config(),
            &super::PipeWireStreamConfig::for_target(super::CaptureTarget::Default)
        );
        assert!(!super::AudioRecorder::is_recording(&recorder));
        assert_eq!(
            super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap_err(),
            super::AudioError::RecorderNotRecording
        );
        super::AudioRecorder::cancel_recording(&mut recorder).unwrap();
    }

    #[test]
    fn pipewire_recording_params_encode_requested_audio_policy() {
        let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
            "alsa_input.usb-mic".to_owned(),
        ));
        let values = super::pipewire_recording_param_values(&config).unwrap();
        let pod = pipewire::spa::pod::Pod::from_bytes(&values).unwrap();
        let mut audio_info = pipewire::spa::param::audio::AudioInfoRaw::new();
        audio_info.parse(pod).unwrap();

        assert_eq!(
            audio_info.format(),
            pipewire::spa::param::audio::AudioFormat::S16LE
        );
        assert_eq!(audio_info.rate(), super::RECORDING_SAMPLE_RATE_HZ);
        assert_eq!(audio_info.channels(), u32::from(super::RECORDING_CHANNELS));
        assert_eq!(
            super::pipewire_capture_source_name(&config),
            "pipewire:alsa_input.usb-mic"
        );
    }

    #[test]
    fn pipewire_recorder_live_capture_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_RECORD_ENV) {
            return;
        }
        let mut recorder = super::PipeWireAudioRecorder::new();
        super::AudioRecorder::begin_recording(&mut recorder, super::CaptureTarget::Default)
            .unwrap();

        assert!(super::AudioRecorder::is_recording(&recorder));
        std::thread::sleep(std::time::Duration::from_millis(100));
        let captured = super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap();

        assert!(!super::AudioRecorder::is_recording(&recorder));
        assert_eq!(captured.pcm.spec(), super::recording_pcm_spec());
        assert_eq!(captured.source_name.as_deref(), Some("pipewire:default"));
    }

    #[test]
    fn pipewire_enumerator_lists_sources_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_ENUMERATE_ENV) {
            return;
        }
        let mut enumerator = super::PipeWireDeviceEnumerator;
        let _devices =
            super::AudioDeviceEnumerator::enumerate_audio_sources(&mut enumerator).unwrap();
    }

    #[test]
    fn pipewire_probe_creates_client_context_when_enabled() {
        if !super::live_test_enabled(super::TEST_PIPEWIRE_CONTEXT_ENV) {
            return;
        }
        super::probe_client_context().unwrap();
    }
}
