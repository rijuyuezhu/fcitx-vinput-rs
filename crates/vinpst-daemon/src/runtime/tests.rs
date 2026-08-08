use std::sync::{Arc, Mutex};

use super::{
    AsrReloadWorkerStep, RuntimeError, RuntimeState,
    output_ducker::{OutputDucker, OutputVolumeControl},
};
use vinpst_asr::{
    AsrBackend, AsrBackendFactory, AsrError, BackendDescriptor, MockAsrAudioLog, MockAsrAudioPush,
    MockAsrBackend, RecognitionContext, RecognitionEvent, RecognitionSession,
};
use vinpst_audio::{
    AudioChunkCallback, AudioError, AudioRecorder, CaptureTarget, CapturedAudio, MockAudioSource,
    PcmBuffer, PcmSpec, SourceAudioRecorder,
};
use vinpst_config::{AsrProviderConfig, AsrProviderKind, VinpstConfig};
use vinpst_protocol::{RecognitionPayload, ServiceStatus};
use vinpst_text::{AdapterRuntimePaths, TextError, TextFinisher, TextProcessor, TextRequest};

const RAW_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/raw.json"
));
const MENU_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/menu.json"
));
const SENTINEL_PAYLOAD_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/recognition/sentinel.json"
));

struct RecordingTextProcessor {
    calls: Arc<Mutex<Vec<String>>>,
}

impl TextProcessor for RecordingTextProcessor {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        self.calls
            .lock()
            .expect("text processor call log poisoned")
            .push(request.raw_text.to_owned());
        Ok(RecognitionPayload::raw(format!(
            "processed: {}",
            request.raw_text
        )))
    }
}

fn fixture_json(input: &str) -> &str {
    input.trim_end()
}

#[test]
fn shared_recognition_fixtures_roundtrip_in_daemon_tests() {
    for fixture in [RAW_PAYLOAD_JSON, MENU_PAYLOAD_JSON, SENTINEL_PAYLOAD_JSON] {
        let fixture = fixture_json(fixture);
        let payload = RecognitionPayload::from_json_str(fixture).unwrap();

        assert_eq!(payload.to_json_string().unwrap(), fixture);
    }
}

fn unique_adapter_runtime_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "vinpst-runtime-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ))
}

fn process_is_runnable(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|status| {
        status
            .lines()
            .find(|line| line.starts_with("State:"))
            .is_none_or(|line| !line.contains("Z (zombie)") && !line.contains("X (dead)"))
    })
}

fn wait_until_process_stops_running(pid: u32) {
    for _ in 0..100 {
        if !process_is_runnable(pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!process_is_runnable(pid), "process {pid} remained runnable");
}

fn config_with_sleep_adapter(adapter_id: &str) -> VinpstConfig {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: adapter_id.to_owned(),
        command: "sleep".to_owned(),
        args: vec!["30".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    config
}

#[derive(Debug)]
struct CapturedHttpRequest {
    head: String,
    body: String,
}

fn serve_single_http_response(
    response_body: String,
) -> (String, std::thread::JoinHandle<CapturedHttpRequest>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        let header_end = loop {
            let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
            assert_ne!(read, 0, "HTTP client closed before headers were complete");
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
        let headers = head
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
            .collect::<std::collections::HashMap<_, _>>();
        let body = if headers
            .get("transfer-encoding")
            .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
        {
            while !buffer[header_end..]
                .windows(5)
                .any(|window| window == b"0\r\n\r\n")
            {
                let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
                assert_ne!(
                    read, 0,
                    "HTTP client closed before chunked body was complete"
                );
                buffer.extend_from_slice(&chunk[..read]);
            }
            decode_chunked_http_body(&buffer[header_end..])
        } else {
            let content_length = headers
                .get("content-length")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while buffer.len() < header_end + content_length {
                let read = std::io::Read::read(&mut stream, &mut chunk).unwrap();
                assert_ne!(read, 0, "HTTP client closed before body was complete");
                buffer.extend_from_slice(&chunk[..read]);
            }
            String::from_utf8_lossy(&buffer[header_end..header_end + content_length]).into_owned()
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        std::io::Write::write_all(&mut stream, response.as_bytes()).unwrap();
        CapturedHttpRequest { head, body }
    });
    (base_url, handle)
}

fn decode_chunked_http_body(input: &[u8]) -> String {
    let mut position = 0;
    let mut decoded = Vec::new();
    while let Some(line_end) = input[position..]
        .windows(2)
        .position(|window| window == b"\r\n")
    {
        let line = String::from_utf8_lossy(&input[position..position + line_end]);
        let chunk_len = usize::from_str_radix(line.trim(), 16).unwrap();
        position += line_end + 2;
        if chunk_len == 0 {
            break;
        }
        decoded.extend_from_slice(&input[position..position + chunk_len]);
        position += chunk_len + 2;
    }
    String::from_utf8(decoded).unwrap()
}

struct ContextRecordingBackend {
    inner: MockAsrBackend,
    captured: Arc<Mutex<Option<RecognitionContext>>>,
}

impl ContextRecordingBackend {
    fn new(captured: Arc<Mutex<Option<RecognitionContext>>>) -> Self {
        Self {
            inner: MockAsrBackend::streaming("listening", "custom final"),
            captured,
        }
    }
}

impl AsrBackend for ContextRecordingBackend {
    fn describe(&self) -> BackendDescriptor {
        self.inner.describe()
    }

    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        *self.captured.lock().expect("context lock poisoned") = Some(context.clone());
        self.inner.create_session(context)
    }
}

struct EventRecordingBackend {
    inner: MockAsrBackend,
    events: Arc<Mutex<Vec<&'static str>>>,
    fail_creation: bool,
}

impl EventRecordingBackend {
    fn new(events: Arc<Mutex<Vec<&'static str>>>, fail_creation: bool) -> Self {
        Self {
            inner: MockAsrBackend::streaming("listening", "custom final"),
            events,
            fail_creation,
        }
    }
}

impl AsrBackend for EventRecordingBackend {
    fn describe(&self) -> BackendDescriptor {
        self.inner.describe()
    }

    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("session");
        if self.fail_creation {
            return Err(AsrError::Backend("test session creation failed".to_owned()));
        }
        self.inner.create_session(context)
    }
}

struct CancelTrackingBackend {
    inner: MockAsrBackend,
    cancelled: Arc<Mutex<bool>>,
}

impl CancelTrackingBackend {
    fn new(cancelled: Arc<Mutex<bool>>) -> Self {
        Self {
            inner: MockAsrBackend::streaming("listening", "custom final"),
            cancelled,
        }
    }
}

impl AsrBackend for CancelTrackingBackend {
    fn describe(&self) -> BackendDescriptor {
        self.inner.describe()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(CancelTrackingSession {
            cancelled: Arc::clone(&self.cancelled),
        }))
    }
}

struct CancelTrackingSession {
    cancelled: Arc<Mutex<bool>>,
}

impl RecognitionSession for CancelTrackingSession {
    fn push_audio(&mut self, _samples: &[i16]) -> Result<(), AsrError> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        *self.cancelled.lock().expect("cancel lock poisoned") = true;
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(vec![
            RecognitionEvent::FinalText {
                text: "tracked final".to_owned(),
            },
            RecognitionEvent::Completed,
        ])
    }
}

struct PushFailureBackend {
    inner: MockAsrBackend,
    cancelled: Arc<Mutex<bool>>,
}

impl PushFailureBackend {
    fn new(cancelled: Arc<Mutex<bool>>) -> Self {
        Self {
            inner: MockAsrBackend::streaming("listening", "custom final"),
            cancelled,
        }
    }
}

impl AsrBackend for PushFailureBackend {
    fn describe(&self) -> BackendDescriptor {
        self.inner.describe()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(PushFailureSession {
            cancelled: Arc::clone(&self.cancelled),
        }))
    }
}

struct PushFailureSession {
    cancelled: Arc<Mutex<bool>>,
}

impl RecognitionSession for PushFailureSession {
    fn push_audio(&mut self, _samples: &[i16]) -> Result<(), AsrError> {
        Err(AsrError::Backend("test push failed".to_owned()))
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        *self.cancelled.lock().expect("cancel lock poisoned") = true;
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(Vec::new())
    }
}

#[derive(Clone, Copy)]
enum SessionFailureStage {
    PartialPoll,
    Finish,
    FinalPoll,
    NoFinalText,
}

impl SessionFailureStage {
    fn message(self) -> &'static str {
        match self {
            Self::PartialPoll => "test partial poll failed",
            Self::Finish => "test finish failed",
            Self::FinalPoll => "test final poll failed",
            Self::NoFinalText => "recognition completed without final text",
        }
    }
}

struct SessionFailureBackend {
    inner: MockAsrBackend,
    cancelled: Arc<Mutex<bool>>,
    stage: SessionFailureStage,
}

impl SessionFailureBackend {
    fn new(cancelled: Arc<Mutex<bool>>, stage: SessionFailureStage) -> Self {
        Self {
            inner: MockAsrBackend::streaming("listening", "custom final"),
            cancelled,
            stage,
        }
    }
}

impl AsrBackend for SessionFailureBackend {
    fn describe(&self) -> BackendDescriptor {
        self.inner.describe()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(SessionFailureSession {
            cancelled: Arc::clone(&self.cancelled),
            stage: self.stage,
            poll_count: 0,
        }))
    }
}

struct SessionFailureSession {
    cancelled: Arc<Mutex<bool>>,
    stage: SessionFailureStage,
    poll_count: usize,
}

impl RecognitionSession for SessionFailureSession {
    fn push_audio(&mut self, _samples: &[i16]) -> Result<(), AsrError> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if matches!(self.stage, SessionFailureStage::Finish) {
            return Err(AsrError::Backend(self.stage.message().to_owned()));
        }
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        *self.cancelled.lock().expect("cancel lock poisoned") = true;
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        let poll_index = self.poll_count;
        self.poll_count += 1;
        if matches!(self.stage, SessionFailureStage::PartialPoll) && poll_index == 0 {
            return Err(AsrError::Backend(self.stage.message().to_owned()));
        }
        if matches!(self.stage, SessionFailureStage::FinalPoll) && poll_index == 1 {
            return Err(AsrError::Backend(self.stage.message().to_owned()));
        }
        if matches!(self.stage, SessionFailureStage::NoFinalText) && poll_index == 1 {
            return Ok(vec![RecognitionEvent::Completed]);
        }
        Ok(vec![RecognitionEvent::PartialText {
            text: "partial before failure".to_owned(),
        }])
    }
}

struct EventRecordingRecorder {
    events: Arc<Mutex<Vec<&'static str>>>,
    captured: CapturedAudio,
    recording: bool,
    chunk_callback: Option<AudioChunkCallback>,
}

impl EventRecordingRecorder {
    fn new(events: Arc<Mutex<Vec<&'static str>>>, captured: CapturedAudio) -> Self {
        Self {
            events,
            captured,
            recording: false,
            chunk_callback: None,
        }
    }
}

impl AudioRecorder for EventRecordingRecorder {
    fn begin_recording(&mut self, _target: CaptureTarget) -> Result<(), AudioError> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("begin");
        self.recording = true;
        Ok(())
    }

    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>) {
        self.chunk_callback = callback;
    }

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        if !self.recording {
            return Err(AudioError::RecorderNotRecording);
        }
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("stop");
        self.recording = false;
        if let Some(callback) = &mut self.chunk_callback {
            callback(&self.captured.pcm);
        }
        Ok(self.captured.clone())
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("cancel");
        self.recording = false;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

struct StopFailureRecorder {
    events: Arc<Mutex<Vec<&'static str>>>,
    recording: bool,
}

impl StopFailureRecorder {
    fn new(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
        Self {
            events,
            recording: false,
        }
    }
}

impl AudioRecorder for StopFailureRecorder {
    fn begin_recording(&mut self, _target: CaptureTarget) -> Result<(), AudioError> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("begin");
        self.recording = true;
        Ok(())
    }

    fn set_chunk_callback(&mut self, _callback: Option<AudioChunkCallback>) {}

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("stop-error");
        Err(AudioError::RecordingBackendUnavailable(
            "test stop failed".to_owned(),
        ))
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push("cancel");
        self.recording = false;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

struct BeginFailureRecorder;

impl AudioRecorder for BeginFailureRecorder {
    fn begin_recording(&mut self, _target: CaptureTarget) -> Result<(), AudioError> {
        Err(AudioError::RecordingBackendUnavailable(
            "test recorder unavailable".to_owned(),
        ))
    }

    fn set_chunk_callback(&mut self, _callback: Option<AudioChunkCallback>) {}

    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        Err(AudioError::RecorderNotRecording)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        Ok(())
    }

    fn is_recording(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct TrackingVolumeState {
    current: f64,
    writes: Vec<f64>,
}

struct TrackingVolumeControl {
    state: Arc<Mutex<TrackingVolumeState>>,
}

impl OutputVolumeControl for TrackingVolumeControl {
    fn read_default_sink_volume(&mut self) -> Option<f64> {
        Some(
            self.state
                .lock()
                .expect("tracking volume lock poisoned")
                .current,
        )
    }

    fn set_default_sink_volume(&mut self, volume: f64) -> bool {
        self.state
            .lock()
            .expect("tracking volume lock poisoned")
            .writes
            .push(volume);
        true
    }
}

fn install_tracking_output_ducker(
    runtime: &mut RuntimeState,
    current: f64,
) -> Arc<Mutex<TrackingVolumeState>> {
    let state = Arc::new(Mutex::new(TrackingVolumeState {
        current,
        writes: Vec::new(),
    }));
    runtime.output_ducker = OutputDucker::with_control(Box::new(TrackingVolumeControl {
        state: Arc::clone(&state),
    }));
    state
}

#[test]
fn duplicate_start_is_rejected_while_recording() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config).unwrap();

    runtime.start_recording().unwrap();
    let error = runtime
        .start_command_recording("selected text")
        .unwrap_err();

    assert!(matches!(
        error,
        super::RuntimeError::Busy(ServiceStatus::Recording)
    ));
    assert_eq!(runtime.status(), ServiceStatus::Recording);
    assert!(runtime.partial_text().is_none());
    let report = runtime.stop_recording_report(None).unwrap();
    assert_eq!(report.partial_text.as_deref(), Some("mock partial"));
    assert_eq!(report.payload.commit_text, "mock recognition result");
}

#[test]
fn stop_while_idle_is_rejected_without_state_changes() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config).unwrap();

    let error = runtime.stop_recording(None).unwrap_err();

    assert!(matches!(
        error,
        super::RuntimeError::NotRecording(ServiceStatus::Idle)
    ));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
}

#[test]
fn normal_recording_mock_roundtrip_returns_to_idle() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config).unwrap();
    runtime.start_recording().unwrap();
    assert_eq!(runtime.status(), ServiceStatus::Recording);
    assert!(runtime.partial_text().is_none());
    let report = runtime.stop_recording_report(None).unwrap();
    assert_eq!(report.partial_text.as_deref(), Some("mock partial"));
    assert_eq!(report.payload.commit_text, "mock recognition result");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn stop_exposes_postprocessing_before_text_finishing() {
    let config = VinpstConfig::bundled_default().unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let processor = RecordingTextProcessor {
        calls: Arc::clone(&calls),
    };
    let mut runtime = RuntimeState::with_components(
        config,
        Box::new(MockAsrBackend::buffered("custom final")),
        Box::new(super::default_mock_audio_source()),
        Box::new(processor),
    )
    .unwrap();

    runtime.start_recording().unwrap();
    let pending = runtime.begin_stop_recording(None).unwrap();

    assert_eq!(runtime.status(), ServiceStatus::Postprocessing);
    assert!(
        calls
            .lock()
            .expect("text processor call log poisoned")
            .is_empty()
    );

    let report = runtime.finish_stop_recording(pending).unwrap();

    assert_eq!(report.payload.commit_text, "processed: custom final");
    assert_eq!(
        *calls.lock().expect("text processor call log poisoned"),
        vec!["custom final"]
    );
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn default_mock_audio_source_supports_two_roundtrips() {
    assert_eq!(super::DEFAULT_MOCK_AUDIO_FRAMES, 4);
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config).unwrap();

    runtime.start_recording().unwrap();
    assert_eq!(
        runtime.stop_recording(None).unwrap().commit_text,
        "mock recognition result"
    );
    runtime.start_command_recording("selected text").unwrap();
    assert_eq!(
        runtime.stop_recording(None).unwrap().commit_text,
        "mock command result for: selected text"
    );
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[cfg(not(feature = "sherpa-onnx-backend"))]
#[test]
fn configured_asr_state_reports_default_backend_without_runtime() {
    let config = VinpstConfig::bundled_default().unwrap();
    let state = RuntimeState::configured_asr_state(&config);
    assert_eq!(state.target_provider_id, "sherpa-onnx");
    assert!(!state.has_effective_backend);
    assert!(state.last_error.contains("sherpa-onnx runtime"));
}

#[test]
fn configured_text_adapters_index_runtime_config() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "vinpst-postprocess".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([("TOKEN".to_owned(), "secret".to_owned())]),
        working_dir: Some("/tmp/adapter-work".to_owned()),
        extra: std::collections::HashMap::default(),
    });
    let runtime = RuntimeState::new(config).unwrap();
    let registry = runtime.configured_text_adapters();

    assert_eq!(registry.len(), 1);
    assert!(registry.contains_command_adapter("cmd-adapter"));
    assert!(!registry.contains_command_adapter("missing"));
    assert_eq!(
        registry
            .command_adapter("cmd-adapter")
            .map(vinpst_text::CommandTextAdapter::command),
        Some("vinpst-postprocess")
    );
    assert_eq!(
        runtime.single_configured_text_adapter_id().as_deref(),
        Some("cmd-adapter")
    );

    let state = runtime.configured_text_adapter_state_for_runtime();
    assert_eq!(state.adapter_count, 1);
    assert_eq!(state.adapter_ids, ["cmd-adapter"]);
    assert_eq!(state.single_adapter_id.as_deref(), Some("cmd-adapter"));
    assert_eq!(state.adapters[0].kind, "command");
    assert_eq!(state.adapters[0].args_count, 1);
    assert_eq!(state.adapters[0].env_count, 1);
    assert!(!state.adapters[0].is_running);
    assert_eq!(state.adapters[0].pid, None);
    assert!(state.adapters[0].has_working_dir);
}

#[test]
fn text_adapter_state_json_redacts_env_and_working_dir_values() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "vinpst-postprocess".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::from([(
            "SECRET_TOKEN".to_owned(),
            "dummy-secret-token".to_owned(),
        )]),
        working_dir: Some("/tmp/vinpst-secret-workdir".to_owned()),
        extra: std::collections::HashMap::default(),
    });
    let state = RuntimeState::configured_text_adapter_state(&config);

    let json = serde_json::to_string(&state).unwrap();

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["adapters"][0]["env_count"], 1);
    assert_eq!(value["adapters"][0]["has_working_dir"], true);
    assert!(!json.contains("SECRET_TOKEN"));
    assert!(!json.contains("dummy-secret-token"));
    assert!(!json.contains("/tmp/vinpst-secret-workdir"));
}

#[test]
fn configured_capture_target_defaults_to_backend_default() {
    let config = VinpstConfig::bundled_default().unwrap();
    let runtime = RuntimeState::new(config.clone()).unwrap();

    assert_eq!(
        RuntimeState::configured_capture_target(&config).unwrap(),
        CaptureTarget::Default
    );
    assert_eq!(
        runtime.capture_target_for_runtime().unwrap(),
        CaptureTarget::Default
    );
}

#[test]
fn configured_capture_target_preserves_concrete_target_object() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.global.capture_device = "  alsa_input.usb-mic  ".to_owned();
    let runtime = RuntimeState::new(config.clone()).unwrap();
    let expected = CaptureTarget::Object("alsa_input.usb-mic".to_owned());

    assert_eq!(
        RuntimeState::configured_capture_target(&config).unwrap(),
        expected
    );
    assert_eq!(runtime.capture_target_for_runtime().unwrap(), expected);
}

#[test]
fn configured_text_adapter_state_preserves_multiple_config_order() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "first".to_owned(),
        command: "first-helper".to_owned(),
        args: Vec::new(),
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "second".to_owned(),
        command: "second-helper".to_owned(),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });

    let state = RuntimeState::configured_text_adapter_state(&config);
    assert_eq!(state.adapter_count, 2);
    assert_eq!(state.adapter_ids, ["first", "second"]);
    assert!(state.single_adapter_id.is_none());
    assert_eq!(state.adapters[0].args_count, 0);
    assert_eq!(state.adapters[1].args_count, 1);
}

#[test]
fn dropping_runtime_cleans_up_supervised_adapter() {
    let runtime_dir = unique_adapter_runtime_dir("drop-cleanup");
    let pid_path = runtime_dir.join("cmd-adapter.pid");
    let mut runtime = RuntimeState::new(config_with_sleep_adapter("cmd-adapter"))
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    assert!(!runtime.is_text_adapter_running("cmd-adapter"));
    assert_eq!(runtime.text_adapter_pid("cmd-adapter"), None);
    let pid = runtime.start_text_adapter("cmd-adapter").unwrap();
    assert!(pid_path.exists());
    let state = runtime.configured_text_adapter_state_for_runtime();
    assert!(state.adapters[0].is_running);
    assert_eq!(state.adapters[0].pid, Some(pid));

    drop(runtime);

    assert!(!pid_path.exists());
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn configured_reload_stops_adapter_removed_from_config() {
    let runtime_dir = unique_adapter_runtime_dir("reload-removal");
    let pid_path = runtime_dir.join("cmd-adapter.pid");
    let config = config_with_sleep_adapter("cmd-adapter");
    let mut runtime = RuntimeState::new(config.clone())
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    let pid = runtime.start_text_adapter("cmd-adapter").unwrap();
    assert!(process_is_runnable(pid));
    assert!(pid_path.exists());

    let mut updated = config;
    updated.llm.adapters.clear();
    updated.validate().unwrap();
    assert!(runtime.queue_configured_asr_reload(updated).unwrap());

    assert!(!runtime.is_text_adapter_running("cmd-adapter"));
    assert_eq!(runtime.text_adapter_pid("cmd-adapter"), None);
    assert_eq!(
        runtime
            .configured_text_adapter_state_for_runtime()
            .adapter_count,
        0
    );
    assert!(!pid_path.exists());
    wait_until_process_stops_running(pid);
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn configured_reload_restarts_adapter_whose_definition_changed() {
    let runtime_dir = unique_adapter_runtime_dir("reload-update");
    let config = config_with_sleep_adapter("cmd-adapter");
    let mut runtime = RuntimeState::new(config.clone())
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    let old_pid = runtime.start_text_adapter("cmd-adapter").unwrap();
    let mut updated = config;
    updated.llm.adapters[0].args = vec!["60".to_owned()];
    updated.validate().unwrap();
    assert!(runtime.queue_configured_asr_reload(updated).unwrap());

    let new_pid = runtime
        .text_adapter_pid("cmd-adapter")
        .expect("restarted adapter pid");
    assert_ne!(new_pid, old_pid);
    assert!(process_is_runnable(new_pid));
    wait_until_process_stops_running(old_pid);
    assert_eq!(
        runtime.configured_text_adapter_state_for_runtime().adapters[0].args_count,
        1
    );
    runtime.stop_text_adapter("cmd-adapter").unwrap();
    wait_until_process_stops_running(new_pid);
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn configured_reload_restarts_adapter_when_only_managed_revision_changes() {
    let runtime_dir = unique_adapter_runtime_dir("reload-revision");
    let config = config_with_sleep_adapter("cmd-adapter");
    let mut runtime = RuntimeState::new(config.clone())
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    let old_pid = runtime.start_text_adapter("cmd-adapter").unwrap();
    let mut updated = config;
    updated.llm.adapters[0].extra.insert(
        "x-vinpst-managed-script-sha256".to_owned(),
        serde_json::json!("revision-2"),
    );
    updated.validate().unwrap();
    assert!(runtime.queue_configured_asr_reload(updated).unwrap());

    let new_pid = runtime
        .text_adapter_pid("cmd-adapter")
        .expect("revision restart pid");
    assert_ne!(new_pid, old_pid);
    assert!(process_is_runnable(new_pid));
    wait_until_process_stops_running(old_pid);
    runtime.stop_text_adapter("cmd-adapter").unwrap();
    wait_until_process_stops_running(new_pid);
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn configured_reload_rolls_back_prior_stops_when_later_stop_fails() {
    let runtime_dir = unique_adapter_runtime_dir("reload-rollback");
    let mut config = config_with_sleep_adapter("first-adapter");
    config.llm.adapters.push(
        config_with_sleep_adapter("second-adapter")
            .llm
            .adapters
            .remove(0),
    );
    let mut runtime = RuntimeState::new(config.clone())
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    let old_pid = runtime.start_text_adapter("first-adapter").unwrap();
    let invalid_pid_path = runtime_dir.join("second-adapter.pid");
    std::fs::write(&invalid_pid_path, "{not-json").unwrap();
    let mut updated = config;
    updated.llm.adapters.clear();
    updated.validate().unwrap();

    let error = runtime
        .queue_configured_asr_reload(updated)
        .expect_err("later invalid pid must reject reload");
    assert!(error.to_string().contains("pid file is invalid"));
    assert_eq!(
        runtime
            .configured_text_adapter_state_for_runtime()
            .adapter_count,
        2
    );
    let restored_pid = runtime
        .text_adapter_pid("first-adapter")
        .expect("rolled-back adapter pid");
    assert_ne!(restored_pid, old_pid);
    assert!(process_is_runnable(restored_pid));
    wait_until_process_stops_running(old_pid);

    std::fs::remove_file(invalid_pid_path).unwrap();
    runtime.stop_text_adapter("first-adapter").unwrap();
    wait_until_process_stops_running(restored_pid);
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn configured_reload_rolls_back_from_preserved_script_when_replacement_start_fails() {
    let runtime_dir = unique_adapter_runtime_dir("reload-start-rollback");
    let script_path = runtime_dir.join("adapter.sh");
    let rollback_path = vinpst_registry::managed_script_rollback_path(&script_path);
    std::fs::create_dir_all(&runtime_dir).unwrap();
    std::fs::write(
        &script_path,
        "sleep 30
",
    )
    .unwrap();
    std::fs::write(
        &rollback_path,
        "sleep 30
",
    )
    .unwrap();
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "/bin/sh".to_owned(),
        args: vec![script_path.to_string_lossy().into_owned()],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::from([(
            vinpst_config::MANAGED_SCRIPT_REVISION_KEY.to_owned(),
            serde_json::json!("revision-1"),
        )]),
    });
    let mut runtime = RuntimeState::new(config.clone())
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    let old_pid = runtime.start_text_adapter("cmd-adapter").unwrap();
    std::fs::write(
        &script_path,
        "exit 99
",
    )
    .unwrap();
    let mut updated = config;
    updated.llm.adapters[0].command = "/definitely/missing/vinpst-adapter".to_owned();
    updated.llm.adapters[0].extra.insert(
        vinpst_config::MANAGED_SCRIPT_REVISION_KEY.to_owned(),
        serde_json::json!("revision-2"),
    );
    updated.llm.adapters[0].extra.insert(
        vinpst_config::MANAGED_SCRIPT_ROLLBACK_REVISION_KEY.to_owned(),
        serde_json::json!("revision-1"),
    );
    updated.validate().unwrap();

    let error = runtime
        .queue_configured_asr_reload(updated)
        .expect_err("replacement start must reject reload");
    assert!(error.to_string().contains("failed to spawn text adapter"));
    assert_eq!(runtime.config.llm.adapters[0].command, "/bin/sh");
    let restored_pid = runtime
        .text_adapter_pid("cmd-adapter")
        .expect("restored old adapter pid");
    assert_ne!(restored_pid, old_pid);
    assert!(process_is_runnable(restored_pid));
    wait_until_process_stops_running(old_pid);
    let cmdline = std::fs::read(format!("/proc/{restored_pid}/cmdline")).unwrap();
    assert!(
        cmdline
            .windows(rollback_path.as_os_str().as_encoded_bytes().len())
            .any(|window| window == rollback_path.as_os_str().as_encoded_bytes())
    );

    runtime.stop_text_adapter("cmd-adapter").unwrap();
    wait_until_process_stops_running(restored_pid);
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn refresh_text_adapters_reaps_exited_processes_and_descendants() {
    let runtime_dir = unique_adapter_runtime_dir("refresh-exited");
    let pid_path = runtime_dir.join("cmd-adapter.pid");
    let child_pid_path = runtime_dir.join("child.pid");
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "/bin/sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"sleep 30 & echo $! > "$CHILD_PID""#.to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "CHILD_PID".to_owned(),
            child_pid_path.to_string_lossy().into_owned(),
        )]),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    let mut runtime = RuntimeState::new(config)
        .unwrap()
        .with_adapter_runtime_paths(AdapterRuntimePaths::new(runtime_dir.clone()));

    runtime.start_text_adapter("cmd-adapter").unwrap();
    assert!(pid_path.exists());
    for _ in 0..100 {
        if child_pid_path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let child_pid = std::fs::read_to_string(&child_pid_path)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();

    let mut exited = Vec::new();
    for _ in 0..20 {
        exited = runtime.refresh_text_adapters();
        if !exited.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert_eq!(exited, ["cmd-adapter".to_owned()]);
    assert!(!runtime.is_text_adapter_running("cmd-adapter"));
    assert_eq!(runtime.text_adapter_pid("cmd-adapter"), None);
    assert!(!pid_path.exists());
    wait_until_process_stops_running(child_pid);
    let _ = std::fs::remove_dir_all(runtime_dir);
}

#[test]
fn configured_asr_builds_mock_provider() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let runtime = RuntimeState::with_configured_asr(config).unwrap();
    assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");
}

fn config_with_mock_asr() -> VinpstConfig {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("mock-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    config
}

#[test]
fn background_asr_reload_reports_pending_and_physical_preparation() {
    let initial = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::with_asr_backend(
        initial,
        Box::new(MockAsrBackend::buffered("injected final")),
    )
    .unwrap();

    assert!(
        runtime
            .queue_configured_asr_reload(config_with_mock_asr())
            .unwrap()
    );
    let pending = runtime.asr_backend_state();
    assert!(pending.reload_in_progress);
    assert_eq!(pending.target_provider_id, "mock");
    assert_eq!(pending.effective_model_id, "mock-buffered");

    let AsrReloadWorkerStep::Prepare(request) = runtime.next_asr_reload_worker_step() else {
        panic!("queued reload should enter physical preparation");
    };
    assert!(runtime.asr_backend_state().reload_in_progress);
    let prepared = request.prepare().unwrap();
    runtime.complete_prepared_asr_reload(prepared);

    let ready = runtime.asr_backend_state();
    assert!(!ready.reload_in_progress);
    assert_eq!(ready.effective_provider_id, "mock");
    assert_eq!(ready.effective_model_id, "mock-streaming");
    assert!(matches!(
        runtime.next_asr_reload_worker_step(),
        AsrReloadWorkerStep::Stop
    ));
}

#[test]
fn background_asr_reload_discards_stale_prepared_generation() {
    let initial = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::with_asr_backend(
        initial,
        Box::new(MockAsrBackend::buffered("injected final")),
    )
    .unwrap();

    assert!(
        runtime
            .queue_configured_asr_reload(config_with_mock_asr())
            .unwrap()
    );
    let AsrReloadWorkerStep::Prepare(first) = runtime.next_asr_reload_worker_step() else {
        panic!("first queued reload should enter preparation");
    };
    let first = first.prepare().unwrap();

    let mut latest = config_with_mock_asr();
    latest.global.default_language = "en-US".to_owned();
    assert!(!runtime.queue_configured_asr_reload(latest).unwrap());
    runtime.complete_prepared_asr_reload(first);
    let stale = runtime.asr_backend_state();
    assert_eq!(stale.effective_model_id, "mock-buffered");
    assert!(stale.reload_in_progress);

    let AsrReloadWorkerStep::Prepare(latest) = runtime.next_asr_reload_worker_step() else {
        panic!("latest queued reload should enter preparation");
    };
    runtime.complete_prepared_asr_reload(latest.prepare().unwrap());
    let ready = runtime.asr_backend_state();
    assert_eq!(ready.effective_model_id, "mock-streaming");
    assert!(!ready.reload_in_progress);
}

#[test]
fn background_asr_reload_failure_notifies_only_for_current_generation() {
    let initial = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::with_asr_backend(
        initial,
        Box::new(MockAsrBackend::buffered("injected final")),
    )
    .unwrap();

    assert!(
        runtime
            .queue_configured_asr_reload(config_with_mock_asr())
            .unwrap()
    );
    let AsrReloadWorkerStep::Prepare(first) = runtime.next_asr_reload_worker_step() else {
        panic!("first queued reload should enter preparation");
    };

    let mut latest = config_with_mock_asr();
    latest.global.default_language = "en-US".to_owned();
    assert!(!runtime.queue_configured_asr_reload(latest).unwrap());
    let error = RuntimeError::BackgroundTask("stale failure".to_owned());
    assert!(
        runtime
            .fail_prepared_asr_reload(first.generation(), &error)
            .is_none()
    );
    assert!(runtime.asr_backend_state().last_error.is_empty());

    let AsrReloadWorkerStep::Prepare(latest) = runtime.next_asr_reload_worker_step() else {
        panic!("latest queued reload should enter preparation");
    };
    let message = runtime
        .fail_prepared_asr_reload(latest.generation(), &error)
        .expect("current generation should produce one notification message");
    assert!(message.contains("Failed to reload ASR backend."));
    assert_eq!(runtime.asr_backend_state().last_error, message);
}

#[test]
fn reload_asr_backend_keeps_injected_backend() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("helper".to_owned()),
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::new(config).unwrap();

    let state = runtime.reload_asr_backend().unwrap();
    assert_eq!(state.effective_provider_id, "mock");
    assert_eq!(state.target_provider_id, "cmd");
    assert_eq!(state.target_model_id, "cmd-model");
}

#[test]
fn command_line_disabled_asr_survives_configured_reloads() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec!["-c".to_owned(), "printf disabled".to_owned()],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::new(config.clone()).unwrap();
    runtime.disable_asr("ASR disabled by command line.");

    let state = runtime.asr_backend_state();
    assert!(!state.has_effective_backend);
    assert_eq!(state.target_provider_id, "cmd");
    assert_eq!(state.target_model_id, "cmd-model");
    assert_eq!(state.last_error, "ASR disabled by command line.");

    let state = runtime.reload_configured_asr_backend().unwrap();
    assert!(!state.has_effective_backend);
    assert_eq!(state.last_error, "ASR disabled by command line.");

    let mut updated = config;
    updated.global.default_language = "en-US".to_owned();
    assert!(!runtime.queue_configured_asr_reload(updated).unwrap());
    assert!(matches!(
        runtime.next_asr_reload_worker_step(),
        AsrReloadWorkerStep::Stop
    ));
    assert_eq!(
        runtime.asr_backend_state().last_error,
        "ASR disabled by command line."
    );
}

#[test]
fn reload_asr_backend_is_deferred_while_recording() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: None,
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("helper".to_owned()),
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::new(config).unwrap();

    runtime.start_recording().unwrap();
    let state = runtime.reload_asr_backend().unwrap();

    assert_eq!(runtime.status(), ServiceStatus::Recording);
    assert!(state.reload_in_progress);
    assert!(runtime.asr_backend_state().reload_in_progress);
    assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");
    assert!(matches!(
        runtime.stop_recording(None),
        Ok(payload) if payload.commit_text == "mock recognition result"
    ));
    let state = runtime.asr_backend_state();
    assert!(!state.reload_in_progress);
    assert_eq!(state.effective_provider_id, "mock");
    assert_eq!(state.target_provider_id, "cmd");
    assert!(state.last_error.is_empty());
}

#[test]
fn reload_configured_asr_backend_is_deferred_and_applied_when_idle() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; printf '%s\n' 'runtime deferred command final'".to_owned(),
        ],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::new(config).unwrap();

    runtime.start_recording().unwrap();
    let state = runtime.reload_configured_asr_backend().unwrap();

    assert_eq!(runtime.status(), ServiceStatus::Recording);
    assert!(state.reload_in_progress);
    assert_eq!(state.effective_provider_id, "mock");
    assert!(matches!(
        runtime.stop_recording(None),
        Ok(payload) if payload.commit_text == "mock recognition result"
    ));
    let state = runtime.asr_backend_state();
    assert!(!state.reload_in_progress);
    assert_eq!(state.effective_provider_id, "cmd");
    assert_eq!(state.effective_model_id, "cmd-model");
    assert!(state.last_error.is_empty());

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();
    assert_eq!(payload.commit_text.trim(), "runtime deferred command final");
}

#[test]
fn queued_configured_reload_updates_idle_runtime_config_and_text_policy() {
    let config = VinpstConfig::bundled_default().unwrap();
    let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
    let audio = super::default_mock_audio_source();
    let mut runtime =
        RuntimeState::with_configured_text(config.clone(), Box::new(backend), Box::new(audio))
            .unwrap();

    let mut updated = config;
    updated.global.capture_device = "virtual.source".to_owned();
    updated.scenes.active_scene = "reload-scene".to_owned();
    updated
        .scenes
        .definitions
        .push(vinpst_config::SceneDefinition {
            id: "reload-scene".to_owned(),
            label: "Reload scene".to_owned(),
            prompt: Some("polish text".to_owned()),
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    updated.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "reload-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s\n' '{"text":"reloaded configured final"}'"#.to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    updated.validate().unwrap();

    assert!(runtime.queue_configured_asr_reload(updated).unwrap());
    assert_eq!(runtime.scene_state().0, "reload-scene");
    assert_eq!(runtime.capture_device(), "virtual.source");

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();
    assert_eq!(payload.commit_text, "reloaded configured final");
}

#[test]
fn queued_configured_reload_defers_scene_changes_until_busy_runtime_is_idle() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config.clone()).unwrap();
    runtime.start_recording().unwrap();

    let mut updated = config;
    updated.scenes.active_scene = vinpst_config::COMMAND_SCENE_ID.to_owned();
    updated.validate().unwrap();

    assert!(runtime.queue_configured_asr_reload(updated).unwrap());
    assert_eq!(runtime.scene_state().0, vinpst_config::RAW_SCENE_ID);
    runtime.stop_recording(None).unwrap();
    assert!(matches!(
        runtime.next_asr_reload_worker_step(),
        AsrReloadWorkerStep::Prepare(_)
    ));
    assert_eq!(runtime.scene_state().0, vinpst_config::COMMAND_SCENE_ID);
}

#[test]
fn deferred_configured_asr_reload_failure_keeps_previous_backend() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("mock-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::new(config).unwrap();

    runtime.start_recording().unwrap();
    runtime.config.asr.active_provider = "missing".to_owned();
    let state = runtime.reload_configured_asr_backend().unwrap();

    assert!(state.reload_in_progress);
    assert_eq!(state.effective_provider_id, "mock");
    assert!(matches!(
        runtime.stop_recording(None),
        Ok(payload) if payload.commit_text == "mock recognition result"
    ));
    let state = runtime.asr_backend_state();
    assert!(!state.reload_in_progress);
    assert_eq!(state.effective_provider_id, "mock");
    assert!(
        state
            .last_error
            .contains("Failed to apply deferred ASR backend reload."),
        "unexpected deferred reload error: {}",
        state.last_error
    );
}

#[test]
fn configured_command_asr_provider_runs_process_helper() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; printf '%s
' 'runtime command final'"
                .to_owned(),
        ],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();

    assert_eq!(payload.commit_text, "runtime command final");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn configured_remote_asr_provider_posts_wav_and_returns_text() {
    let response_body = serde_json::json!({"text": "remote runtime final"}).to_string();
    let (base_url, handle) = serve_single_http_response(response_body);
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "remote".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "remote".to_owned(),
        kind: AsrProviderKind::Remote,
        timeout_ms: Some(2_000),
        model: Some("remote-runtime-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::from([
            ("VINPST_ASR_API_KEY".to_owned(), "remote-secret".to_owned()),
            ("VINPST_ASR_LANGUAGE".to_owned(), "zh".to_owned()),
            ("VINPST_ASR_PROMPT".to_owned(), "runtime prompt".to_owned()),
        ]),
        endpoint: Some(format!("{base_url}/v1")),
    });
    let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();

    assert_eq!(payload.commit_text, "remote runtime final");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    let captured = handle.join().unwrap();
    assert!(
        captured
            .head
            .starts_with("POST /v1/audio/transcriptions HTTP/1.1")
    );
    assert!(
        captured
            .head
            .to_ascii_lowercase()
            .contains("authorization: bearer remote-secret")
    );
    assert!(
        captured
            .head
            .to_ascii_lowercase()
            .contains("content-type: multipart/form-data; boundary=")
    );
    assert!(captured.body.contains("RIFF"));
    assert!(captured.body.contains("name=\"model\""));
    assert!(captured.body.contains("remote-runtime-model"));
    assert!(captured.body.contains("name=\"language\""));
    assert!(captured.body.contains("zh"));
    assert!(captured.body.contains("name=\"prompt\""));
    assert!(captured.body.contains("runtime prompt"));
}

#[test]
fn configured_streaming_command_asr_reports_stop_partial() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd.streaming".to_owned();
    config.asr.providers.push(AsrProviderConfig {
            id: "cmd.streaming".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: Some(1_000),
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("sh".to_owned()),
            args: vec![
                "-c".to_owned(),
                r#"cat >/dev/null; printf '%s
' '{"type":"partial","text":"runtime partial"}' '{"type":"partial","text":"runtime partial"}' '{"type":"final","text":"runtime streaming final"}' '{"type":"closed"}'"#.to_owned(),
            ],
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
    let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

    runtime.start_recording().unwrap();
    let report = runtime.stop_recording_report(None).unwrap();

    assert_eq!(report.partial_text.as_deref(), Some("runtime partial"));
    assert_eq!(report.payload.commit_text, "runtime streaming final");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
}

#[test]
fn configured_command_asr_provider_forwards_runtime_pcm_metadata() {
    let mut capture_path = std::env::temp_dir();
    capture_path.push(format!(
        "vinpst-runtime-command-asr-request-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));

    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r#"cat > "$ASR_REQUEST"; printf '%s
' 'runtime command final'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::from([(
            "ASR_REQUEST".to_owned(),
            capture_path.to_string_lossy().into_owned(),
        )]),
        endpoint: None,
    });
    let backend = AsrBackendFactory::build_active(&config.asr).unwrap();
    let pcm = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        },
        vec![16, -32, 48, -64],
    )
    .unwrap();
    let audio = CapturedAudio::named(pcm, "fixture");
    let mut runtime = RuntimeState::with_backends(
        config,
        backend,
        Box::new(MockAudioSource::from_frames(vec![audio.clone(), audio])),
    )
    .unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();
    assert_eq!(payload.commit_text, "runtime command final");

    let bytes = std::fs::read(&capture_path).unwrap();
    std::fs::remove_file(&capture_path).unwrap();
    let expected_samples = [4000_i16, -8000, 12000, -16000];
    let expected_bytes = expected_samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(bytes, expected_bytes);
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn configured_legacy_command_asr_report_has_no_stop_partial() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_000),
        model: Some("cmd-model".to_owned()),
        hotwords_file: None,
        command: Some("sh".to_owned()),
        args: vec![
            "-c".to_owned(),
            r"cat >/dev/null; printf '%s
' 'runtime final'"
                .to_owned(),
        ],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::with_configured_asr(config).unwrap();

    runtime.start_recording().unwrap();
    let report = runtime.stop_recording_report(None).unwrap();

    assert_eq!(report.payload.commit_text, "runtime final");
    assert!(report.partial_text.is_none());
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
}

#[test]
fn configured_asr_state_preserves_command_provider_metadata() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "cmd".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "cmd".to_owned(),
        kind: AsrProviderKind::Command,
        timeout_ms: Some(1_500),
        model: Some("cmd-model".to_owned()),
        hotwords_file: Some("/tmp/hotwords.txt".to_owned()),
        command: Some("helper".to_owned()),
        args: vec!["--json".to_owned()],
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let runtime = RuntimeState::with_configured_asr(config).unwrap();

    let state = runtime.asr_backend_state();
    assert!(state.has_effective_backend);
    assert_eq!(state.target_provider_id, "cmd");
    assert_eq!(state.target_model_id, "cmd-model");
    assert_eq!(state.effective_provider_id, "cmd");
    assert_eq!(state.effective_model_id, "cmd-model");
}

#[test]
fn reload_configured_asr_backend_swaps_to_configured_provider() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("mock-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    let mut runtime = RuntimeState::new(config).unwrap();
    assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");

    let state = runtime.reload_configured_asr_backend().unwrap();
    assert_eq!(state.effective_provider_id, "mock");
    assert_eq!(state.effective_model_id, "mock-streaming");
    assert_eq!(state.target_model_id, "mock-model");
    assert!(state.has_effective_backend);
}

#[test]
fn configured_runtime_starts_without_a_usable_asr_backend() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::with_configured_backends_or_unavailable(config).unwrap();

    let state = runtime.asr_backend_state();
    assert_eq!(state.target_provider_id, "sherpa-onnx");
    assert!(state.target_model_id.is_empty());
    assert!(!state.has_effective_backend);
    assert!(!state.last_error.is_empty());

    let error = runtime.start_recording().unwrap_err();
    assert!(matches!(error, super::RuntimeError::Asr(_)));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn configured_recorder_runtime_starts_without_a_usable_asr_backend() {
    let config = VinpstConfig::bundled_default().unwrap();
    let recorder = Box::new(SourceAudioRecorder::new(Box::new(
        super::default_mock_audio_source(),
    )));
    let mut runtime =
        RuntimeState::with_configured_audio_recorder_or_unavailable(config, recorder).unwrap();

    let state = runtime.asr_backend_state();
    assert_eq!(state.target_provider_id, "sherpa-onnx");
    assert!(state.target_model_id.is_empty());
    assert!(!state.has_effective_backend);
    assert!(!state.last_error.is_empty());

    let error = runtime.start_recording().unwrap_err();
    assert!(matches!(error, super::RuntimeError::Asr(_)));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn unavailable_configured_runtime_recovers_after_successful_reload() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::with_configured_backends_or_unavailable(config).unwrap();
    runtime.config.asr.active_provider = "mock".to_owned();
    runtime.config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: Some("mock-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });

    let state = runtime.reload_configured_asr_backend().unwrap();
    assert_eq!(state.effective_provider_id, "mock");
    assert_eq!(state.effective_model_id, "mock-streaming");
    assert!(state.has_effective_backend);
    assert!(state.last_error.is_empty());

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();
    assert_eq!(payload.commit_text, "mock recognition result");
}

#[test]
fn reload_configured_asr_backend_reports_build_errors_without_swapping() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config).unwrap();
    let before = runtime.asr_backend_state();

    let Err(error) = runtime.reload_configured_asr_backend() else {
        panic!("default unsupported configured ASR should fail to build");
    };

    assert!(matches!(error, super::RuntimeError::Asr(_)));
    let after = runtime.asr_backend_state();
    assert_eq!(after.effective_provider_id, before.effective_provider_id);
    assert_eq!(after.effective_model_id, before.effective_model_id);
}

#[test]
fn runtime_asr_state_preserves_configured_target_metadata() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "remote".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "remote".to_owned(),
        kind: AsrProviderKind::Remote,
        timeout_ms: None,
        model: Some("cloud-model".to_owned()),
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: Some("https://asr.example.test".to_owned()),
    });
    let runtime = RuntimeState::new(config).unwrap();

    let state = runtime.asr_backend_state();
    assert_eq!(state.target_provider_id, "remote");
    assert_eq!(state.target_model_id, "cloud-model");
    assert_eq!(state.effective_provider_id, "mock");
    assert_eq!(state.effective_model_id, "mock-streaming");
    assert_eq!(state.remote_endpoints, ["https://asr.example.test"]);
}

#[test]
fn configured_asr_reports_default_backend_as_unsupported() {
    let config = VinpstConfig::bundled_default().unwrap();
    let Err(error) = RuntimeState::with_configured_asr(config) else {
        panic!("default backend should be unsupported in current prototype");
    };
    assert!(matches!(error, super::RuntimeError::Asr(_)));
}

#[test]
fn asr_push_failure_cancels_session_and_returns_to_idle() {
    let config = VinpstConfig::bundled_default().unwrap();
    let cancelled = Arc::new(Mutex::new(false));
    let backend = PushFailureBackend::new(Arc::clone(&cancelled));
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = EventRecordingRecorder::new(
        Arc::clone(&events),
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 96, -96, 0])),
    );
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();

    runtime.start_recording().unwrap();
    let error = runtime.stop_recording(None).unwrap_err();

    assert!(matches!(
        error,
        super::RuntimeError::Asr(AsrError::Backend(message))
            if message == "test push failed"
    ));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
    assert!(*cancelled.lock().expect("cancel lock poisoned"));
    assert_eq!(
        *events.lock().expect("events lock poisoned"),
        vec!["begin", "stop"]
    );
}

#[test]
fn asr_stop_result_failures_cancel_session_and_return_to_idle() {
    for stage in [
        SessionFailureStage::PartialPoll,
        SessionFailureStage::Finish,
        SessionFailureStage::FinalPoll,
        SessionFailureStage::NoFinalText,
    ] {
        let config = VinpstConfig::bundled_default().unwrap();
        let cancelled = Arc::new(Mutex::new(false));
        let expected_message = stage.message();
        let backend = SessionFailureBackend::new(Arc::clone(&cancelled), stage);
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = EventRecordingRecorder::new(
            Arc::clone(&events),
            CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 96, -96, 0])),
        );
        let mut runtime =
            RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder))
                .unwrap();

        runtime.start_recording().unwrap();
        let error = runtime.stop_recording(None).unwrap_err();

        assert!(matches!(
            error,
            super::RuntimeError::Asr(AsrError::Backend(message))
                if message == expected_message
        ));
        assert_eq!(runtime.status(), ServiceStatus::Idle);
        assert!(runtime.partial_text().is_none());
        assert!(*cancelled.lock().expect("cancel lock poisoned"));
        assert_eq!(
            *events.lock().expect("events lock poisoned"),
            vec!["begin", "stop"]
        );
    }
}

#[test]
fn recorder_stop_failure_cancels_and_returns_to_idle() {
    let config = VinpstConfig::bundled_default().unwrap();
    let cancelled = Arc::new(Mutex::new(false));
    let backend = CancelTrackingBackend::new(Arc::clone(&cancelled));
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = StopFailureRecorder::new(Arc::clone(&events));
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();

    runtime.start_recording().unwrap();
    let error = runtime.stop_recording(None).unwrap_err();

    assert!(matches!(
        error,
        super::RuntimeError::Audio(AudioError::RecordingBackendUnavailable(message))
            if message == "test stop failed"
    ));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
    assert!(*cancelled.lock().expect("cancel lock poisoned"));
    assert_eq!(
        *events.lock().expect("events lock poisoned"),
        vec!["begin", "stop-error", "cancel"]
    );
}

#[test]
fn output_ducking_restores_after_normal_stop() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.global.duck_output_while_recording = true;
    config.global.duck_output_volume = 0.25;
    let mut runtime = RuntimeState::new(config).unwrap();
    let volume = install_tracking_output_ducker(&mut runtime, 0.8);

    runtime.start_recording().unwrap();
    runtime.stop_recording_report(None).unwrap();

    let volume = volume.lock().expect("tracking volume lock poisoned");
    assert_eq!(volume.writes.len(), 2);
    assert!((volume.writes[0] - 0.2).abs() < f64::EPSILON);
    assert!((volume.writes[1] - 0.8).abs() < f64::EPSILON);
}

#[test]
fn output_ducking_restores_after_stop_failure() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.global.duck_output_while_recording = true;
    config.global.duck_output_volume = 0.5;
    let backend = MockAsrBackend::streaming("partial", "final");
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = StopFailureRecorder::new(events);
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();
    let volume = install_tracking_output_ducker(&mut runtime, 0.6);

    runtime.start_recording().unwrap();
    assert!(runtime.stop_recording_report(None).is_err());

    let volume = volume.lock().expect("tracking volume lock poisoned");
    assert_eq!(volume.writes.len(), 2);
    assert!((volume.writes[0] - 0.3).abs() < f64::EPSILON);
    assert!((volume.writes[1] - 0.6).abs() < f64::EPSILON);
}

#[test]
fn output_ducking_restores_when_recording_runtime_is_dropped() {
    let volume = {
        let mut config = VinpstConfig::bundled_default().unwrap();
        config.global.duck_output_while_recording = true;
        config.global.duck_output_volume = 0.25;
        let mut runtime = RuntimeState::new(config).unwrap();
        let volume = install_tracking_output_ducker(&mut runtime, 1.0);
        runtime.start_recording().unwrap();
        assert_eq!(
            volume
                .lock()
                .expect("tracking volume lock poisoned")
                .writes
                .len(),
            1
        );
        volume
    };

    let volume = volume.lock().expect("tracking volume lock poisoned");
    assert_eq!(volume.writes.len(), 2);
    assert!((volume.writes[0] - 0.25).abs() < f64::EPSILON);
    assert!((volume.writes[1] - 1.0).abs() < f64::EPSILON);
}

#[test]
fn recorder_begin_failure_leaves_runtime_idle() {
    let config = VinpstConfig::bundled_default().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let backend = ContextRecordingBackend::new(Arc::clone(&captured));
    let mut runtime = RuntimeState::with_audio_recorder(
        config,
        Box::new(backend),
        Box::new(BeginFailureRecorder),
    )
    .unwrap();

    let error = runtime.start_recording().unwrap_err();

    assert!(matches!(
        error,
        super::RuntimeError::Audio(AudioError::RecordingBackendUnavailable(message))
            if message == "test recorder unavailable"
    ));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
    assert!(captured.lock().expect("context lock poisoned").is_none());
    assert!(matches!(
        runtime.stop_recording(None).unwrap_err(),
        super::RuntimeError::NotRecording(ServiceStatus::Idle)
    ));
}

#[test]
fn capture_begins_before_asr_session_creation() {
    let config = VinpstConfig::bundled_default().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let backend = EventRecordingBackend::new(Arc::clone(&events), false);
    let recorder = EventRecordingRecorder::new(
        Arc::clone(&events),
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![1, -1])),
    );
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();

    runtime.start_recording().unwrap();
    assert_eq!(
        *events.lock().expect("events lock poisoned"),
        vec!["begin", "session"]
    );
    runtime.stop_recording(None).unwrap();
}

#[test]
fn session_creation_failure_cancels_started_capture() {
    let config = VinpstConfig::bundled_default().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let backend = EventRecordingBackend::new(Arc::clone(&events), true);
    let recorder = EventRecordingRecorder::new(
        Arc::clone(&events),
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![1, -1])),
    );
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();

    let error = runtime.start_recording().unwrap_err();

    assert!(matches!(
        error,
        super::RuntimeError::Asr(AsrError::Backend(message))
            if message == "test session creation failed"
    ));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert_eq!(
        *events.lock().expect("events lock poisoned"),
        vec!["begin", "session", "cancel"]
    );
}

#[test]
fn injected_asr_backend_drives_normal_result() {
    let config = VinpstConfig::bundled_default().unwrap();
    let backend = MockAsrBackend::streaming("listening", "custom final");
    let mut runtime = RuntimeState::with_asr_backend(config, Box::new(backend)).unwrap();
    runtime.start_recording().unwrap();
    assert!(runtime.partial_text().is_none());
    let report = runtime.stop_recording_report(None).unwrap();
    assert_eq!(report.partial_text.as_deref(), Some("listening"));
    assert_eq!(report.payload.commit_text, "custom final");
}

#[test]
fn early_final_event_is_preserved_until_payload_conversion() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::with_asr_backend(
        config,
        Box::new(MockAsrBackend::streaming_with_early_final(
            "early partial",
            "early final",
        )),
    )
    .unwrap();

    runtime.start_recording().unwrap();
    let report = runtime.stop_recording_report(None).unwrap();

    assert_eq!(report.partial_text.as_deref(), Some("early partial"));
    assert_eq!(report.payload.commit_text, "early final");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
}

#[test]
fn runtime_pushes_processed_pcm_with_metadata_to_buffered_asr_session() {
    let config = VinpstConfig::bundled_default().unwrap();
    let audio_log = MockAsrAudioLog::new();
    let backend = MockAsrBackend::buffered("custom final").with_audio_log(audio_log.clone());
    let source = MockAudioSource::once(CapturedAudio::anonymous(
        PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 48_000,
                channels: 2,
            },
            vec![0, 0, 12, -12, 20, -20, 0, 0],
        )
        .unwrap(),
    ));
    let mut runtime =
        RuntimeState::with_backends(config, Box::new(backend), Box::new(source)).unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();

    assert_eq!(payload.commit_text, "custom final");
    assert_eq!(
        audio_log.records(),
        vec![MockAsrAudioPush {
            sample_len: 4,
            pcm_spec: Some(PcmSpec {
                sample_rate_hz: 48_000,
                channels: 2,
            }),
        }]
    );
}

#[test]
fn runtime_streams_pcm_in_legacy_sized_batches_without_replaying_final_buffer() {
    let config = VinpstConfig::bundled_default().unwrap();
    let audio_log = MockAsrAudioLog::new();
    let backend =
        MockAsrBackend::streaming("listening", "custom final").with_audio_log(audio_log.clone());
    let source = MockAudioSource::once(CapturedAudio::anonymous(PcmBuffer::at_default_rate(
        vec![64; 1_700],
    )));
    let recorder = SourceAudioRecorder::new(Box::new(source))
        .with_chunk_frames(300)
        .unwrap();
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();

    runtime.start_recording().unwrap();
    let report = runtime.stop_recording_report(None).unwrap();

    assert_eq!(report.partial_text.as_deref(), Some("listening"));
    assert_eq!(report.payload.commit_text, "custom final");
    assert_eq!(
        audio_log.records(),
        vec![
            MockAsrAudioPush {
                sample_len: 800,
                pcm_spec: Some(PcmSpec::default()),
            },
            MockAsrAudioPush {
                sample_len: 800,
                pcm_spec: Some(PcmSpec::default()),
            },
            MockAsrAudioPush {
                sample_len: 100,
                pcm_spec: Some(PcmSpec::default()),
            },
        ]
    );
}

#[test]
fn injected_audio_source_is_used_by_runtime() {
    let config = VinpstConfig::bundled_default().unwrap();
    let backend = MockAsrBackend::streaming("listening", "custom final");
    let source = MockAudioSource::from_frames(vec![
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 32, -32, 0])),
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 64, -64, 0])),
    ]);
    let mut runtime =
        RuntimeState::with_backends(config, Box::new(backend), Box::new(source)).unwrap();
    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();
    assert_eq!(payload.commit_text, "custom final");
}

#[test]
fn injected_audio_recorder_uses_start_stop_lifecycle() {
    let config = VinpstConfig::bundled_default().unwrap();
    let backend = MockAsrBackend::streaming("listening", "custom final");
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = EventRecordingRecorder::new(
        Arc::clone(&events),
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 96, -96, 0])),
    );
    let mut runtime =
        RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder)).unwrap();

    runtime.start_recording().unwrap();
    assert_eq!(*events.lock().expect("events lock poisoned"), vec!["begin"]);
    let report = runtime.stop_recording_report(None).unwrap();

    assert_eq!(report.partial_text.as_deref(), Some("listening"));
    assert_eq!(report.payload.commit_text, "custom final");
    assert_eq!(
        *events.lock().expect("events lock poisoned"),
        vec!["begin", "stop"]
    );
}

#[test]
fn dropping_recording_runtime_cancels_asr_session() {
    let config = VinpstConfig::bundled_default().unwrap();
    let cancelled = Arc::new(Mutex::new(false));
    let backend = CancelTrackingBackend::new(Arc::clone(&cancelled));
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorder = EventRecordingRecorder::new(
        Arc::clone(&events),
        CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![0, 96, -96, 0])),
    );

    {
        let mut runtime =
            RuntimeState::with_audio_recorder(config, Box::new(backend), Box::new(recorder))
                .unwrap();
        runtime.start_recording().unwrap();
    }

    assert!(*cancelled.lock().expect("cancel lock poisoned"));
    assert_eq!(
        *events.lock().expect("events lock poisoned"),
        vec!["begin", "cancel"]
    );
}

#[test]
fn command_recording_passes_context_to_asr_backend() {
    let config = VinpstConfig::bundled_default().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let backend = ContextRecordingBackend::new(Arc::clone(&captured));
    let mut runtime = RuntimeState::with_asr_backend(config, Box::new(backend)).unwrap();

    runtime.start_command_recording("selected text").unwrap();

    let context = captured
        .lock()
        .expect("context lock poisoned")
        .clone()
        .expect("ASR backend should receive context");
    assert!(context.command_mode);
    assert_eq!(context.scene_id, vinpst_config::COMMAND_SCENE_ID);
    assert_eq!(context.language.as_deref(), Some("zh"));
    assert_eq!(context.selected_text.as_deref(), Some("selected text"));
}

#[test]
fn configured_text_adapter_processes_prompted_scene() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.active_scene = "needs-adapter".to_owned();
    config
        .scenes
        .definitions
        .push(vinpst_config::SceneDefinition {
            id: "needs-adapter".to_owned(),
            label: "Needs adapter".to_owned(),
            prompt: Some("polish text".to_owned()),
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s
' '{"text":"configured final"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
    let audio = super::default_mock_audio_source();
    let mut runtime =
        RuntimeState::with_configured_text(config, Box::new(backend), Box::new(audio)).unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();

    assert_eq!(payload.commit_text, "configured final");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
}

#[test]
fn configured_text_openai_provider_processes_prompted_scene_over_http() {
    let response_body = serde_json::json!({
        "choices": [{
            "message": {
                "content": serde_json::json!({"candidates": ["http polished"]}).to_string()
            }
        }]
    })
    .to_string();
    let (base_url, handle) = serve_single_http_response(response_body);
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.active_scene = "needs-provider".to_owned();
    config.llm.providers.push(vinpst_config::LlmProviderConfig {
        id: "openai".to_owned(),
        base_url,
        api_key: "secret-token".to_owned(),
        model: Some("provider-model".to_owned()),
        extra_body: serde_json::json!({}),
        extra: std::collections::HashMap::new(),
    });
    config
        .scenes
        .definitions
        .push(vinpst_config::SceneDefinition {
            id: "needs-provider".to_owned(),
            label: "Needs provider".to_owned(),
            prompt: Some("Polish: {{ asr }}".to_owned()),
            provider_id: Some("openai".to_owned()),
            model: Some("scene-model".to_owned()),
            candidate_count: 1,
            timeout_ms: Some(2_000),
            context_lines: 0,
        });
    let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
    let audio = super::default_mock_audio_source();
    let mut runtime =
        RuntimeState::with_configured_text(config, Box::new(backend), Box::new(audio)).unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();

    assert_eq!(payload.commit_text, "http polished");
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    let captured = handle.join().unwrap();
    assert!(captured.head.starts_with("POST /chat/completions HTTP/1.1"));
    assert!(
        captured
            .head
            .to_ascii_lowercase()
            .contains("authorization: bearer secret-token")
    );
    let posted: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
    assert_eq!(posted["model"], "scene-model");
    let content = posted["messages"][0]["content"].as_str().unwrap();
    assert!(content.contains("Polish: mock recognition result"));
    assert!(content.contains("Return EXACTLY 1 candidate"));
}

#[test]
fn configured_backends_process_prompted_scene_with_mock_asr() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.asr.active_provider = "mock".to_owned();
    config.asr.providers.push(AsrProviderConfig {
        id: "mock".to_owned(),
        kind: AsrProviderKind::Local,
        timeout_ms: None,
        model: None,
        hotwords_file: None,
        command: None,
        args: Vec::new(),
        env: std::collections::HashMap::new(),
        endpoint: None,
    });
    config.scenes.active_scene = "needs-adapter".to_owned();
    config
        .scenes
        .definitions
        .push(vinpst_config::SceneDefinition {
            id: "needs-adapter".to_owned(),
            label: "Needs adapter".to_owned(),
            prompt: Some("polish text".to_owned()),
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    config.llm.adapters.push(vinpst_config::LlmAdapterConfig {
        id: "cmd-adapter".to_owned(),
        command: "sh".to_owned(),
        args: vec![
            "-c".to_owned(),
            r#"cat >/dev/null; printf '%s
' '{"text":"configured backend final"}'"#
                .to_owned(),
        ],
        env: std::collections::HashMap::default(),
        working_dir: None,
        extra: std::collections::HashMap::default(),
    });
    let mut runtime = RuntimeState::with_configured_backends(config).unwrap();

    runtime.start_recording().unwrap();
    let payload = runtime.stop_recording(None).unwrap();

    assert_eq!(runtime.asr_backend_state().effective_provider_id, "mock");
    assert_eq!(payload.commit_text, "configured backend final");
}

#[test]
fn timeout_scene_finish_error_returns_runtime_to_idle() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.active_scene = "timeout-scene".to_owned();
    config
        .scenes
        .definitions
        .push(vinpst_config::SceneDefinition {
            id: "timeout-scene".to_owned(),
            label: "Timeout scene".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 0,
            timeout_ms: Some(2500),
            context_lines: 0,
        });
    let backend = MockAsrBackend::streaming("mock partial", "mock recognition result");
    let audio = super::default_mock_audio_source();
    let mut runtime = RuntimeState::with_components(
        config,
        Box::new(backend),
        Box::new(audio),
        Box::new(TextFinisher::new()),
    )
    .unwrap();

    runtime.start_recording().unwrap();
    let error = runtime.stop_recording(None).unwrap_err();
    let message = error.to_string();

    assert!(matches!(error, super::RuntimeError::Finish(_)));
    assert!(message.contains("text adapter backend"));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
}

#[test]
fn failed_text_finishing_returns_runtime_to_idle() {
    let mut config = VinpstConfig::bundled_default().unwrap();
    config.scenes.active_scene = "needs-adapter".to_owned();
    config.llm.providers.push(vinpst_config::LlmProviderConfig {
        id: "openai".to_owned(),
        base_url: "https://example.invalid/v1".to_owned(),
        api_key: String::new(),
        model: None,
        extra_body: serde_json::json!({}),
        extra: std::collections::HashMap::new(),
    });
    config
        .scenes
        .definitions
        .push(vinpst_config::SceneDefinition {
            id: "needs-adapter".to_owned(),
            label: "Needs adapter".to_owned(),
            prompt: Some("polish text".to_owned()),
            provider_id: Some("openai".to_owned()),
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    let cancelled = Arc::new(Mutex::new(false));
    let backend = CancelTrackingBackend::new(Arc::clone(&cancelled));
    let audio = super::default_mock_audio_source();
    let mut runtime = RuntimeState::with_components(
        config,
        Box::new(backend),
        Box::new(audio),
        Box::new(TextFinisher::new()),
    )
    .unwrap();

    runtime.start_recording().unwrap();
    let error = runtime.stop_recording(None).unwrap_err();

    assert!(matches!(error, super::RuntimeError::Finish(_)));
    assert_eq!(runtime.status(), ServiceStatus::Idle);
    assert!(runtime.partial_text().is_none());
    assert!(*cancelled.lock().expect("cancel lock poisoned"));
}

#[test]
fn command_recording_uses_selected_text_context() {
    let config = VinpstConfig::bundled_default().unwrap();
    let mut runtime = RuntimeState::new(config).unwrap();
    runtime.start_command_recording("hello").unwrap();
    let payload = runtime.stop_recording(None).unwrap();
    assert_eq!(payload.commit_text, "mock command result for: hello");
}
