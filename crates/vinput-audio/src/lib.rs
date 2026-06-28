//! Pure PCM audio utilities used before the real `PipeWire` capture layer lands.
//!
//! This crate deliberately starts without `PipeWire`.  It owns typed PCM buffers
//! and deterministic transforms so audio behavior can be tested independently
//! from desktop/audio-server integration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "pipewire-backend")]
pub mod pipewire_backend;

/// Default sample rate used by the original daemon's ASR pipeline.
pub const DEFAULT_SAMPLE_RATE_HZ: u32 = 16_000;

/// Default channel count for mono ASR audio.
pub const DEFAULT_CHANNELS: u16 = 1;

/// Signed 16-bit PCM layout metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PcmSpec {
    /// Sample rate in hertz.
    pub sample_rate_hz: u32,
    /// Number of interleaved channels.
    #[serde(default = "default_channels")]
    pub channels: u16,
}

impl PcmSpec {
    /// Creates a mono signed 16-bit PCM spec.
    #[must_use]
    pub const fn mono_i16(sample_rate_hz: u32) -> Self {
        Self {
            sample_rate_hz,
            channels: DEFAULT_CHANNELS,
        }
    }

    /// Validates sample rate and channel count.
    pub fn validate(self) -> Result<Self, AudioError> {
        if self.sample_rate_hz == 0 {
            return Err(AudioError::InvalidSampleRate(self.sample_rate_hz));
        }
        if self.channels == 0 {
            return Err(AudioError::InvalidChannelCount(self.channels));
        }
        Ok(self)
    }
}

impl Default for PcmSpec {
    fn default() -> Self {
        Self::mono_i16(DEFAULT_SAMPLE_RATE_HZ)
    }
}

/// Mono signed 16-bit PCM buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PcmBuffer {
    spec: PcmSpec,
    samples: Vec<i16>,
}

const fn default_channels() -> u16 {
    DEFAULT_CHANNELS
}

/// Encodes signed 16-bit PCM samples as little-endian bytes.
#[must_use]
pub fn i16_samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}
impl PcmBuffer {
    /// Creates a mono PCM buffer with the given sample rate.
    pub fn new(sample_rate_hz: u32, samples: impl Into<Vec<i16>>) -> Result<Self, AudioError> {
        Self::with_spec(PcmSpec::mono_i16(sample_rate_hz), samples)
    }

    /// Creates a PCM buffer with explicit layout metadata.
    pub fn with_spec(spec: PcmSpec, samples: impl Into<Vec<i16>>) -> Result<Self, AudioError> {
        let spec = spec.validate()?;
        let samples = samples.into();
        if samples.len() % usize::from(spec.channels) != 0 {
            return Err(AudioError::UnalignedSamples {
                samples: samples.len(),
                channels: spec.channels,
            });
        }
        Ok(Self { spec, samples })
    }

    /// Decodes an uncompressed RIFF/WAVE signed 16-bit PCM buffer.
    pub fn from_wav_pcm16le_bytes(bytes: &[u8]) -> Result<Self, AudioError> {
        decode_wav_pcm16le(bytes)
    }

    /// Decodes raw signed 16-bit little-endian PCM bytes with explicit layout metadata.
    pub fn from_pcm16le_bytes(spec: PcmSpec, bytes: &[u8]) -> Result<Self, AudioError> {
        if !bytes.len().is_multiple_of(2) {
            return Err(AudioError::OddPcmByteCount(bytes.len()));
        }
        let samples = bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        Self::with_spec(spec, samples)
    }

    /// Creates a 16 kHz mono PCM buffer.
    pub fn at_default_rate(samples: impl Into<Vec<i16>>) -> Self {
        Self {
            spec: PcmSpec::default(),
            samples: samples.into(),
        }
    }

    /// Returns the PCM layout metadata.
    #[must_use]
    pub const fn spec(&self) -> PcmSpec {
        self.spec
    }

    /// Returns the sample rate.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.spec.sample_rate_hz
    }

    /// Returns the channel count.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.spec.channels
    }

    /// Returns the raw samples.
    #[must_use]
    pub fn samples(&self) -> &[i16] {
        &self.samples
    }

    /// Returns mutable raw samples.
    #[must_use]
    pub fn samples_mut(&mut self) -> &mut [i16] {
        &mut self.samples
    }

    /// Returns true when no samples are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Returns the number of raw i16 samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns the number of PCM frames.
    #[must_use]
    pub fn frame_len(&self) -> usize {
        self.samples.len() / usize::from(self.spec.channels)
    }

    /// Returns duration in milliseconds, rounded down.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        let frames = u64::try_from(self.frame_len()).unwrap_or(u64::MAX);
        frames.saturating_mul(1000) / u64::from(self.spec.sample_rate_hz)
    }

    /// Returns the peak absolute amplitude as an `i16`-range value.
    #[must_use]
    pub fn peak_abs(&self) -> i16 {
        self.samples
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap_or(0)
            .min(i16::MAX as u16)
            .cast_signed()
    }

    /// Returns a copy with gain applied using saturating i16 conversion.
    #[must_use]
    pub fn with_gain(&self, gain: f32) -> Self {
        let mut next = self.clone();
        next.apply_gain(gain);
        next
    }

    /// Applies gain in place using saturating i16 conversion.
    pub fn apply_gain(&mut self, gain: f32) {
        if !gain.is_finite() {
            return;
        }
        for sample in &mut self.samples {
            *sample = scale_sample(*sample, gain);
        }
    }

    /// Returns a copy normalized to a target peak.
    #[must_use]
    pub fn normalized_to_peak(&self, target_peak: i16) -> Self {
        let mut next = self.clone();
        next.normalize_to_peak(target_peak);
        next
    }

    /// Normalizes in place to a target peak.
    pub fn normalize_to_peak(&mut self, target_peak: i16) {
        let current_peak = self.peak_abs();
        if current_peak == 0 || target_peak <= 0 {
            return;
        }
        let gain = f32::from(target_peak) / f32::from(current_peak);
        self.apply_gain(gain);
    }

    /// Returns whether all samples are below or equal to the silence threshold.
    #[must_use]
    pub fn is_silent(&self, threshold_abs: i16) -> bool {
        let threshold = threshold_abs.unsigned_abs();
        self.samples
            .iter()
            .all(|sample| sample.unsigned_abs() <= threshold)
    }

    /// Returns a copy with leading and trailing silent frames removed.
    #[must_use]
    pub fn trimmed_silence(&self, threshold_abs: i16) -> Self {
        let threshold = threshold_abs.unsigned_abs();
        let channels = usize::from(self.spec.channels);
        let start_frame = self
            .samples
            .chunks_exact(channels)
            .position(|frame| frame.iter().any(|sample| sample.unsigned_abs() > threshold));
        let Some(start_frame) = start_frame else {
            return Self {
                spec: self.spec,
                samples: Vec::new(),
            };
        };
        let end_frame = self
            .samples
            .chunks_exact(channels)
            .rposition(|frame| frame.iter().any(|sample| sample.unsigned_abs() > threshold))
            .expect("start frame exists, so end frame exists");
        let start = start_frame * channels;
        let end = (end_frame + 1) * channels;
        Self {
            spec: self.spec,
            samples: self.samples[start..end].to_vec(),
        }
    }
}

/// Deterministic audio processing policy applied before ASR delivery.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioProcessingOptions {
    /// Absolute threshold used to trim quiet leading/trailing regions.
    pub silence_threshold_abs: i16,
    /// Optional target peak for normalization.
    #[serde(default)]
    pub normalize_to_peak: Option<i16>,
    /// Gain multiplier applied after optional normalization.
    pub input_gain: f32,
}

impl AudioProcessingOptions {
    /// Creates processing options.
    #[must_use]
    pub const fn new(
        silence_threshold_abs: i16,
        normalize_to_peak: Option<i16>,
        input_gain: f32,
    ) -> Self {
        Self {
            silence_threshold_abs,
            normalize_to_peak,
            input_gain,
        }
    }

    /// Applies trim, optional normalization, and gain in deterministic order.
    #[must_use]
    pub fn process(&self, pcm: &PcmBuffer) -> PcmBuffer {
        let mut processed = pcm.trimmed_silence(self.silence_threshold_abs);
        if let Some(target_peak) = self.normalize_to_peak {
            processed.normalize_to_peak(target_peak);
        }
        processed.apply_gain(self.input_gain);
        processed
    }
}

/// PCM buffer plus capture metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CapturedAudio {
    /// Captured PCM samples.
    pub pcm: PcmBuffer,
    /// Optional source name, such as a `PipeWire` node or test fixture id.
    #[serde(default)]
    pub source_name: Option<String>,
}

impl CapturedAudio {
    /// Creates captured audio without source metadata.
    #[must_use]
    pub fn anonymous(pcm: PcmBuffer) -> Self {
        Self {
            pcm,
            source_name: None,
        }
    }

    /// Creates captured audio with a source name.
    #[must_use]
    pub fn named(pcm: PcmBuffer, source_name: impl Into<String>) -> Self {
        Self {
            pcm,
            source_name: Some(source_name.into()),
        }
    }

    /// Returns captured duration in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.pcm.duration_ms()
    }
}

/// Capture target selected by config or UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CaptureTarget {
    /// Use the default desktop audio source.
    #[default]
    Default,
    /// Use a concrete backend target object such as a `PipeWire` node name.
    Object(String),
}

impl CaptureTarget {
    /// Parses a config value such as `default` or a backend object id.
    pub fn from_config_value(value: &str) -> Result<Self, AudioError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(AudioError::InvalidCaptureTarget(value.to_owned()));
        }
        if trimmed == "default" {
            return Ok(Self::Default);
        }
        Ok(Self::Object(trimmed.to_owned()))
    }

    /// Returns the backend object value for non-default targets.
    #[must_use]
    pub fn target_object(&self) -> Option<&str> {
        match self {
            Self::Default => None,
            Self::Object(value) => Some(value),
        }
    }
}

/// Desktop audio source discovered by a capture backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AudioDeviceInfo {
    /// Backend-local device id, such as a `PipeWire` node id.
    pub id: u32,
    /// Stable backend object name used as a capture target.
    pub name: String,
    /// Human-readable device description.
    pub description: String,
}

impl AudioDeviceInfo {
    /// Creates audio device metadata.
    #[must_use]
    pub fn new(id: u32, name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.into(),
        }
    }

    /// Returns this device as a concrete capture target.
    #[must_use]
    pub fn capture_target(&self) -> CaptureTarget {
        CaptureTarget::Object(self.name.clone())
    }
}

/// Device enumeration contract for desktop capture backends.
pub trait AudioDeviceEnumerator: Send {
    /// List available audio sources in backend discovery order.
    fn enumerate_audio_sources(&mut self) -> Result<Vec<AudioDeviceInfo>, AudioError>;
}

/// Deterministic device enumerator for tests and CLI/UI wiring.
#[derive(Debug, Clone, Default)]
pub struct MockAudioDeviceEnumerator {
    devices: Vec<AudioDeviceInfo>,
}

impl MockAudioDeviceEnumerator {
    /// Creates a mock enumerator from a static device list.
    #[must_use]
    pub fn new(devices: impl Into<Vec<AudioDeviceInfo>>) -> Self {
        Self {
            devices: devices.into(),
        }
    }
}

impl AudioDeviceEnumerator for MockAudioDeviceEnumerator {
    fn enumerate_audio_sources(&mut self) -> Result<Vec<AudioDeviceInfo>, AudioError> {
        Ok(self.devices.clone())
    }
}

/// Callback used by streaming capture backends to forward PCM chunks.
pub type AudioChunkCallback = Box<dyn FnMut(&PcmBuffer) + Send>;

/// Stateful recorder contract mirroring the legacy daemon capture lifecycle.
pub trait AudioRecorder: Send {
    /// Begin a recording session for the selected target.
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError>;

    /// Install or clear a callback for chunks observed while recording.
    fn set_chunk_callback(&mut self, callback: Option<AudioChunkCallback>);

    /// Stop recording and return the accumulated PCM buffer.
    fn stop_and_get_buffer(&mut self) -> Result<CapturedAudio, AudioError>;

    /// Stop recording and discard any accumulated audio.
    fn cancel_recording(&mut self) -> Result<(), AudioError>;

    /// Return whether a recording session is active.
    fn is_recording(&self) -> bool;
}

/// Deterministic stateful recorder for tests and runtime wiring.
pub struct MockAudioRecorder {
    recordings: Vec<CapturedAudio>,
    next: usize,
    recording: bool,
    target: CaptureTarget,
    chunk_callback: Option<AudioChunkCallback>,
}

impl MockAudioRecorder {
    /// Creates a mock recorder from a sequence of completed recordings.
    #[must_use]
    pub fn from_recordings(recordings: impl Into<Vec<CapturedAudio>>) -> Self {
        Self {
            recordings: recordings.into(),
            next: 0,
            recording: false,
            target: CaptureTarget::default(),
            chunk_callback: None,
        }
    }

    /// Creates a mock recorder that returns one completed recording.
    #[must_use]
    pub fn once(recording: CapturedAudio) -> Self {
        Self::from_recordings(vec![recording])
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub const fn target(&self) -> &CaptureTarget {
        &self.target
    }
}

impl AudioRecorder for MockAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        if self.recording {
            return Err(AudioError::RecorderAlreadyRecording);
        }
        self.target = target;
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
        self.recording = false;
        let recording = self
            .recordings
            .get(self.next)
            .cloned()
            .ok_or(AudioError::SourceExhausted)?;
        self.next += 1;
        if let Some(callback) = &mut self.chunk_callback {
            callback(&recording.pcm);
        }
        Ok(recording)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.recording = false;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

/// Compatibility adapter that exposes a stateful recorder as a one-shot source.
pub struct RecorderAudioSource<R> {
    recorder: R,
    target: CaptureTarget,
}

impl<R> RecorderAudioSource<R> {
    /// Creates a compatibility source for the given recorder and target.
    #[must_use]
    pub fn new(recorder: R, target: CaptureTarget) -> Self {
        Self { recorder, target }
    }

    /// Returns the wrapped recorder.
    #[must_use]
    pub const fn recorder(&self) -> &R {
        &self.recorder
    }

    /// Returns the wrapped recorder mutably.
    #[must_use]
    pub const fn recorder_mut(&mut self) -> &mut R {
        &mut self.recorder
    }

    /// Consumes the adapter and returns the wrapped recorder.
    #[must_use]
    pub fn into_recorder(self) -> R {
        self.recorder
    }
}

impl<R: AudioRecorder> AudioSource for RecorderAudioSource<R> {
    fn read_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        self.recorder.begin_recording(self.target.clone())?;
        match self.recorder.stop_and_get_buffer() {
            Ok(captured) => Ok(captured),
            Err(error) => {
                let _ = self.recorder.cancel_recording();
                Err(error)
            }
        }
    }
}

/// Audio source abstraction used before a concrete desktop backend is wired in.
pub trait AudioSource: Send {
    /// Read one PCM buffer.
    fn read_buffer(&mut self) -> Result<CapturedAudio, AudioError>;
}

/// Deterministic audio source for runtime wiring and tests.
#[derive(Debug, Clone)]
pub struct MockAudioSource {
    frames: Vec<CapturedAudio>,
    next: usize,
}

impl MockAudioSource {
    /// Creates a mock source from a sequence of buffers.
    #[must_use]
    pub fn from_frames(frames: impl Into<Vec<CapturedAudio>>) -> Self {
        Self {
            frames: frames.into(),
            next: 0,
        }
    }

    /// Creates a mock source that returns one buffer.
    #[must_use]
    pub fn once(frame: CapturedAudio) -> Self {
        Self::from_frames(vec![frame])
    }
}

impl AudioSource for MockAudioSource {
    fn read_buffer(&mut self) -> Result<CapturedAudio, AudioError> {
        let frame = self
            .frames
            .get(self.next)
            .cloned()
            .ok_or(AudioError::SourceExhausted)?;
        self.next += 1;
        Ok(frame)
    }
}

/// Compatibility recorder backed by a one-shot [`AudioSource`].
pub struct SourceAudioRecorder {
    source: Box<dyn AudioSource>,
    recording: bool,
    target: CaptureTarget,
    chunk_callback: Option<AudioChunkCallback>,
}

impl SourceAudioRecorder {
    /// Creates a stateful recorder facade for an existing audio source.
    #[must_use]
    pub fn new(source: Box<dyn AudioSource>) -> Self {
        Self {
            source,
            recording: false,
            target: CaptureTarget::default(),
            chunk_callback: None,
        }
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub const fn target(&self) -> &CaptureTarget {
        &self.target
    }
}

impl AudioRecorder for SourceAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        if self.recording {
            return Err(AudioError::RecorderAlreadyRecording);
        }
        self.target = target;
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
        self.recording = false;
        let captured = self.source.read_buffer()?;
        if let Some(callback) = &mut self.chunk_callback {
            callback(&captured.pcm);
        }
        Ok(captured)
    }

    fn cancel_recording(&mut self) -> Result<(), AudioError> {
        self.recording = false;
        Ok(())
    }

    fn is_recording(&self) -> bool {
        self.recording
    }
}

/// Audio helper errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AudioError {
    /// Sample rate must not be zero.
    #[error("invalid sample rate: {0}")]
    InvalidSampleRate(u32),
    /// Channel count must not be zero.
    #[error("invalid channel count: {0}")]
    InvalidChannelCount(u16),
    /// Raw sample count must contain complete interleaved frames.
    #[error("sample count {samples} is not aligned to channel count {channels}")]
    UnalignedSamples {
        /// Raw sample count.
        samples: usize,
        /// Configured channel count.
        channels: u16,
    },
    /// Raw PCM input must contain complete little-endian i16 samples.
    #[error("PCM input contains an odd number of bytes: {0}")]
    OddPcmByteCount(usize),
    /// RIFF/WAVE input was not uncompressed signed 16-bit PCM.
    #[error("invalid WAV file: {0}")]
    InvalidWav(String),
    /// Empty mock buffer list.
    #[error("no more buffers")]
    SourceExhausted,
    /// Capture target is blank after trimming.
    #[error("invalid capture target: {0:?}")]
    InvalidCaptureTarget(String),
    /// Recorder was asked to begin while already recording.
    #[error("recorder is already recording")]
    RecorderAlreadyRecording,
    /// Recorder was asked to stop while idle.
    #[error("recorder is not recording")]
    RecorderNotRecording,
    /// Audio recording backend is linked but not usable yet.
    #[error("audio recording backend is unavailable: {0}")]
    RecordingBackendUnavailable(String),
    /// Audio device enumeration failed.
    #[error("audio device enumeration failed: {0}")]
    DeviceEnumerationFailed(String),
}

fn decode_wav_pcm16le(bytes: &[u8]) -> Result<PcmBuffer, AudioError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid_wav("missing RIFF/WAVE header"));
    }

    let mut format: Option<PcmSpec> = None;
    let mut data: Option<&[u8]> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_len = read_le_u32(&bytes[offset + 4..offset + 8])? as usize;
        let chunk_start = offset + 8;
        let chunk_end = chunk_start
            .checked_add(chunk_len)
            .ok_or_else(|| invalid_wav("chunk size overflow"))?;
        if chunk_end > bytes.len() {
            return Err(invalid_wav("chunk extends past end of file"));
        }
        let chunk = &bytes[chunk_start..chunk_end];
        match chunk_id {
            b"fmt " => format = Some(parse_wav_fmt(chunk)?),
            b"data" => data = Some(chunk),
            _ => {}
        }
        offset = chunk_end + (chunk_len % 2);
    }

    let spec = format.ok_or_else(|| invalid_wav("missing fmt chunk"))?;
    let data = data.ok_or_else(|| invalid_wav("missing data chunk"))?;
    if data.len() % 2 != 0 {
        return Err(invalid_wav("data chunk has an odd byte count"));
    }
    let samples = data
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    PcmBuffer::with_spec(spec, samples)
}

fn parse_wav_fmt(chunk: &[u8]) -> Result<PcmSpec, AudioError> {
    if chunk.len() < 16 {
        return Err(invalid_wav("fmt chunk is too short"));
    }
    let format_tag = read_le_u16(&chunk[0..2])?;
    let channels = read_le_u16(&chunk[2..4])?;
    let sample_rate_hz = read_le_u32(&chunk[4..8])?;
    let byte_rate = read_le_u32(&chunk[8..12])?;
    let block_align = read_le_u16(&chunk[12..14])?;
    let bits_per_sample = read_le_u16(&chunk[14..16])?;
    if format_tag != 1 {
        return Err(invalid_wav("only PCM format tag 1 is supported"));
    }
    if bits_per_sample != 16 {
        return Err(invalid_wav("only 16-bit samples are supported"));
    }
    let spec = PcmSpec {
        sample_rate_hz,
        channels,
    }
    .validate()?;
    let expected_block_align = spec
        .channels
        .checked_mul(bits_per_sample / 8)
        .ok_or_else(|| invalid_wav("block align overflow"))?;
    if block_align != expected_block_align {
        return Err(invalid_wav("block align does not match channel count"));
    }
    let expected_byte_rate = spec
        .sample_rate_hz
        .checked_mul(u32::from(expected_block_align))
        .ok_or_else(|| invalid_wav("byte rate overflow"))?;
    if byte_rate != expected_byte_rate {
        return Err(invalid_wav("byte rate does not match sample format"));
    }
    Ok(spec)
}

fn read_le_u16(bytes: &[u8]) -> Result<u16, AudioError> {
    let raw: [u8; 2] = bytes
        .try_into()
        .map_err(|_| invalid_wav("expected 2-byte little-endian integer"))?;
    Ok(u16::from_le_bytes(raw))
}

fn read_le_u32(bytes: &[u8]) -> Result<u32, AudioError> {
    let raw: [u8; 4] = bytes
        .try_into()
        .map_err(|_| invalid_wav("expected 4-byte little-endian integer"))?;
    Ok(u32::from_le_bytes(raw))
}

fn invalid_wav(message: impl Into<String>) -> AudioError {
    AudioError::InvalidWav(message.into())
}

fn scale_sample(sample: i16, gain: f32) -> i16 {
    let scaled = f32::from(sample) * gain;
    if scaled.is_nan() {
        return sample;
    }
    let rounded = scaled.round();
    if rounded <= f32::from(i16::MIN) {
        i16::MIN
    } else if rounded >= f32::from(i16::MAX) {
        i16::MAX
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            rounded as i16
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioDeviceEnumerator, AudioDeviceInfo, AudioError, AudioRecorder, AudioSource,
        CaptureTarget, CapturedAudio, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ,
        MockAudioDeviceEnumerator, MockAudioRecorder, MockAudioSource, PcmBuffer, PcmSpec,
        RecorderAudioSource, SourceAudioRecorder,
    };

    fn wav_pcm16le_bytes(sample_rate_hz: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let mut data = Vec::new();
        for sample in samples {
            data.extend_from_slice(&sample.to_le_bytes());
        }
        let data_len = u32::try_from(data.len()).expect("test data should fit in u32");
        let byte_rate = sample_rate_hz * u32::from(channels) * 2;
        let block_align = channels * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate_hz.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.extend_from_slice(&data);
        wav
    }

    #[test]
    fn encodes_i16_samples_as_little_endian_bytes() {
        assert_eq!(
            super::i16_samples_to_le_bytes(&[0x1234, -2]),
            vec![0x34, 0x12, 0xfe, 0xff]
        );
    }

    #[test]
    fn decodes_raw_pcm16le_bytes_with_explicit_spec() {
        let pcm = PcmBuffer::from_pcm16le_bytes(
            PcmSpec {
                sample_rate_hz: 8_000,
                channels: 2,
            },
            &[0x34, 0x12, 0xfe, 0xff],
        )
        .unwrap();
        assert_eq!(pcm.sample_rate_hz(), 8_000);
        assert_eq!(pcm.channels(), 2);
        assert_eq!(pcm.samples(), &[0x1234, -2]);
    }

    #[test]
    fn raw_pcm16le_rejects_odd_byte_count() {
        assert_eq!(
            PcmBuffer::from_pcm16le_bytes(PcmSpec::default(), &[0]).unwrap_err(),
            AudioError::OddPcmByteCount(1)
        );
    }

    #[test]
    fn rejects_zero_sample_rate() {
        assert_eq!(
            PcmBuffer::new(0, vec![1]).unwrap_err(),
            AudioError::InvalidSampleRate(0)
        );
    }

    #[test]
    fn reports_duration_at_sample_rate() {
        let pcm = PcmBuffer::new(1_000, vec![0; 1_500]).unwrap();
        assert_eq!(pcm.duration_ms(), 1_500);
        assert_eq!(pcm.frame_len(), 1_500);
        assert_eq!(
            PcmBuffer::at_default_rate(vec![0]).sample_rate_hz(),
            DEFAULT_SAMPLE_RATE_HZ
        );
        assert_eq!(
            PcmBuffer::at_default_rate(vec![0]).channels(),
            DEFAULT_CHANNELS
        );
    }

    #[test]
    fn multi_channel_duration_counts_frames_not_samples() {
        let pcm = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 1_000,
                channels: 2,
            },
            vec![0; 2_000],
        )
        .unwrap();
        assert_eq!(pcm.len(), 2_000);
        assert_eq!(pcm.frame_len(), 1_000);
        assert_eq!(pcm.duration_ms(), 1_000);
    }

    #[test]
    fn pcm_spec_rejects_zero_channels() {
        let error = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
                channels: 0,
            },
            vec![1],
        )
        .unwrap_err();
        assert_eq!(error, AudioError::InvalidChannelCount(0));
    }

    #[test]
    fn pcm_buffer_rejects_unaligned_multi_channel_samples() {
        let error = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
                channels: 2,
            },
            vec![1, 2, 3],
        )
        .unwrap_err();
        assert_eq!(
            error,
            AudioError::UnalignedSamples {
                samples: 3,
                channels: 2,
            }
        );
    }

    #[test]
    fn pcm_spec_deserialization_defaults_to_mono() {
        let spec: PcmSpec = serde_json::from_str(r#"{"sample_rate_hz":16000}"#).unwrap();
        assert_eq!(spec.sample_rate_hz, DEFAULT_SAMPLE_RATE_HZ);
        assert_eq!(spec.channels, DEFAULT_CHANNELS);
    }

    #[test]
    fn pcm_buffer_preserves_explicit_spec() {
        let spec = PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        };
        let pcm = PcmBuffer::with_spec(spec, vec![1, -1]).unwrap();
        assert_eq!(pcm.spec(), spec);
        assert_eq!(pcm.sample_rate_hz(), 48_000);
        assert_eq!(pcm.channels(), 2);
    }

    #[test]
    fn wav_pcm16le_parser_preserves_metadata_and_samples() {
        let bytes = wav_pcm16le_bytes(48_000, 2, &[1_000, -1_000, 2_000, -2_000]);
        let pcm = PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap();

        assert_eq!(pcm.sample_rate_hz(), 48_000);
        assert_eq!(pcm.channels(), 2);
        assert_eq!(pcm.samples(), &[1_000, -1_000, 2_000, -2_000]);
    }

    #[test]
    fn wav_pcm16le_parser_skips_unknown_padded_chunks() {
        let bytes = wav_pcm16le_bytes(16_000, 1, &[100, -100]);
        let riff_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let mut with_junk = Vec::new();
        with_junk.extend_from_slice(&bytes[..12]);
        with_junk.extend_from_slice(b"JUNK");
        with_junk.extend_from_slice(&3_u32.to_le_bytes());
        with_junk.extend_from_slice(b"abc");
        with_junk.push(0);
        with_junk.extend_from_slice(&bytes[12..]);
        with_junk[4..8].copy_from_slice(&(riff_size + 12).to_le_bytes());

        let pcm = PcmBuffer::from_wav_pcm16le_bytes(&with_junk).unwrap();
        assert_eq!(pcm.sample_rate_hz(), 16_000);
        assert_eq!(pcm.channels(), 1);
        assert_eq!(pcm.samples(), &[100, -100]);
    }

    #[test]
    fn wav_pcm16le_parser_rejects_inconsistent_layout_metadata() {
        let mut bytes = wav_pcm16le_bytes(16_000, 2, &[1, -1]);
        bytes[32..34].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
            AudioError::InvalidWav("block align does not match channel count".to_owned())
        );

        let mut bytes = wav_pcm16le_bytes(16_000, 2, &[1, -1]);
        bytes[28..32].copy_from_slice(&16_000_u32.to_le_bytes());
        assert_eq!(
            PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
            AudioError::InvalidWav("byte rate does not match sample format".to_owned())
        );
    }

    #[test]
    fn wav_pcm16le_parser_rejects_unsupported_format() {
        let mut bytes = wav_pcm16le_bytes(16_000, 1, &[1]);
        bytes[20..22].copy_from_slice(&3_u16.to_le_bytes());
        assert_eq!(
            PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
            AudioError::InvalidWav("only PCM format tag 1 is supported".to_owned())
        );

        let mut bytes = wav_pcm16le_bytes(16_000, 1, &[1]);
        bytes[34..36].copy_from_slice(&24_u16.to_le_bytes());
        assert_eq!(
            PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
            AudioError::InvalidWav("only 16-bit samples are supported".to_owned())
        );
    }

    #[test]
    fn gain_saturates_to_i16_range() {
        let pcm = PcmBuffer::at_default_rate(vec![20_000, -20_000]).with_gain(2.0);
        assert_eq!(pcm.samples(), &[i16::MAX, i16::MIN]);
    }

    #[test]
    fn non_finite_gain_is_ignored() {
        let original = PcmBuffer::at_default_rate(vec![100, -100]);
        assert_eq!(original.with_gain(f32::NAN), original);
        assert_eq!(original.with_gain(f32::INFINITY), original);
    }

    #[test]
    fn normalization_scales_peak() {
        let pcm = PcmBuffer::at_default_rate(vec![0, 1_000, -2_000]).normalized_to_peak(10_000);
        assert_eq!(pcm.samples(), &[0, 5_000, -10_000]);
        assert_eq!(pcm.peak_abs(), 10_000);
    }

    #[test]
    fn silence_detection_uses_absolute_threshold() {
        assert!(PcmBuffer::at_default_rate(vec![0, 2, -2]).is_silent(2));
        assert!(!PcmBuffer::at_default_rate(vec![0, 3]).is_silent(2));
        assert!(PcmBuffer::at_default_rate(vec![0, 2, -2]).is_silent(-2));
    }

    #[test]
    fn trim_removes_leading_and_trailing_silence() {
        let pcm = PcmBuffer::at_default_rate(vec![0, 1, 5, -6, 1, 0]).trimmed_silence(1);
        assert_eq!(pcm.samples(), &[5, -6]);
        let negative_threshold =
            PcmBuffer::at_default_rate(vec![0, 1, 5, -6, 1, 0]).trimmed_silence(-1);
        assert_eq!(negative_threshold.samples(), &[5, -6]);
    }

    #[test]
    fn multi_channel_trim_preserves_complete_frames() {
        let pcm = PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
                channels: 2,
            },
            vec![0, 0, 1, 2, 5, 0, 0, 0],
        )
        .unwrap()
        .trimmed_silence(2);

        assert_eq!(pcm.samples(), &[5, 0]);
        assert_eq!(pcm.frame_len(), 1);
        assert_eq!(pcm.channels(), 2);
    }

    #[test]
    fn trim_all_silence_returns_empty_buffer() {
        let pcm = PcmBuffer::at_default_rate(vec![0, 1, -1]).trimmed_silence(1);
        assert!(pcm.is_empty());
        assert_eq!(pcm.sample_rate_hz(), DEFAULT_SAMPLE_RATE_HZ);
    }

    #[test]
    fn processing_options_apply_trim_normalize_then_gain() {
        let pcm = PcmBuffer::at_default_rate(vec![0, 1, 10, -20, 1, 0]);
        let options = super::AudioProcessingOptions::new(1, Some(10_000), 0.5);
        let processed = options.process(&pcm);
        assert_eq!(processed.samples(), &[2_500, -5_000]);
    }

    #[test]
    fn processing_options_preserve_multi_channel_spec_and_frames() {
        let spec = PcmSpec {
            sample_rate_hz: 48_000,
            channels: 2,
        };
        let pcm = PcmBuffer::with_spec(spec, vec![0, 0, 10, -10, 0, 0]).unwrap();
        let options = super::AudioProcessingOptions::new(1, None, 2.0);
        let processed = options.process(&pcm);

        assert_eq!(processed.spec(), spec);
        assert_eq!(processed.samples(), &[20, -20]);
        assert_eq!(processed.frame_len(), 1);
    }

    #[test]
    fn captured_audio_reports_pcm_duration_and_source() {
        let captured =
            CapturedAudio::named(PcmBuffer::new(1_000, vec![0; 250]).unwrap(), "fixture");
        assert_eq!(captured.duration_ms(), 250);
        assert_eq!(captured.source_name.as_deref(), Some("fixture"));
    }

    #[test]
    fn capture_target_parses_config_values() {
        assert_eq!(
            CaptureTarget::from_config_value("default").unwrap(),
            CaptureTarget::Default
        );
        assert_eq!(
            CaptureTarget::from_config_value("  alsa_input.usb-mic  ").unwrap(),
            CaptureTarget::Object("alsa_input.usb-mic".to_owned())
        );
        assert_eq!(
            CaptureTarget::from_config_value("  ").unwrap_err(),
            AudioError::InvalidCaptureTarget("  ".to_owned())
        );
        assert_eq!(
            CaptureTarget::Object("node".to_owned()).target_object(),
            Some("node")
        );
        assert_eq!(CaptureTarget::Default.target_object(), None);
    }

    #[test]
    fn audio_device_info_maps_to_capture_target() {
        let device = AudioDeviceInfo::new(42, "alsa_input.usb-mic", "USB Microphone");

        assert_eq!(device.id, 42);
        assert_eq!(device.name, "alsa_input.usb-mic");
        assert_eq!(device.description, "USB Microphone");
        assert_eq!(
            device.capture_target(),
            CaptureTarget::Object("alsa_input.usb-mic".to_owned())
        );
    }

    #[test]
    fn mock_audio_device_enumerator_preserves_backend_order() {
        let devices = vec![
            AudioDeviceInfo::new(7, "first", "First source"),
            AudioDeviceInfo::new(8, "second", "Second source"),
        ];
        let mut enumerator = MockAudioDeviceEnumerator::new(devices.clone());

        assert_eq!(enumerator.enumerate_audio_sources().unwrap(), devices);
        assert_eq!(
            MockAudioDeviceEnumerator::default()
                .enumerate_audio_sources()
                .unwrap(),
            Vec::new()
        );
    }

    #[test]
    fn mock_audio_recorder_tracks_legacy_lifecycle() {
        use std::sync::{Arc, Mutex};

        let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![1, -1]), "fixture");
        let seen_chunk = Arc::new(Mutex::new(Vec::<i16>::new()));
        let seen_chunk_for_callback = Arc::clone(&seen_chunk);
        let mut recorder = MockAudioRecorder::once(captured.clone());

        assert!(!recorder.is_recording());
        assert_eq!(
            recorder.stop_and_get_buffer().unwrap_err(),
            AudioError::RecorderNotRecording
        );
        recorder
            .begin_recording(CaptureTarget::Object("mic".to_owned()))
            .unwrap();
        assert_eq!(recorder.target(), &CaptureTarget::Object("mic".to_owned()));
        assert!(recorder.is_recording());
        assert_eq!(
            recorder
                .begin_recording(CaptureTarget::Default)
                .unwrap_err(),
            AudioError::RecorderAlreadyRecording
        );
        recorder.set_chunk_callback(Some(Box::new(move |pcm| {
            *seen_chunk_for_callback.lock().unwrap() = pcm.samples().to_vec();
        })));

        assert_eq!(recorder.stop_and_get_buffer().unwrap(), captured);
        assert!(!recorder.is_recording());
        assert_eq!(*seen_chunk.lock().unwrap(), vec![1, -1]);
    }

    #[test]
    fn mock_audio_recorder_cancel_discards_active_recording() {
        let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![7]), "fixture");
        let mut recorder = MockAudioRecorder::once(captured.clone());

        recorder.begin_recording(CaptureTarget::Default).unwrap();
        recorder.cancel_recording().unwrap();
        assert!(!recorder.is_recording());
        assert_eq!(
            recorder.stop_and_get_buffer().unwrap_err(),
            AudioError::RecorderNotRecording
        );

        recorder.begin_recording(CaptureTarget::Default).unwrap();
        assert_eq!(recorder.stop_and_get_buffer().unwrap(), captured);
    }

    #[test]
    fn recorder_audio_source_bridges_stateful_recorder() {
        let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![9, -9]), "fixture");
        let recorder = MockAudioRecorder::once(captured.clone());
        let mut source = RecorderAudioSource::new(
            recorder,
            CaptureTarget::Object("alsa_input.usb-mic".to_owned()),
        );

        assert_eq!(source.read_buffer().unwrap(), captured);
        assert_eq!(
            source.recorder().target(),
            &CaptureTarget::Object("alsa_input.usb-mic".to_owned())
        );
        assert!(!source.recorder().is_recording());
    }

    #[test]
    fn recorder_audio_source_cancels_after_stop_error() {
        let recorder = MockAudioRecorder::from_recordings(Vec::new());
        let mut source = RecorderAudioSource::new(recorder, CaptureTarget::Default);

        assert_eq!(
            source.read_buffer().unwrap_err(),
            AudioError::SourceExhausted
        );
        assert!(!source.recorder().is_recording());
    }

    #[test]
    fn source_audio_recorder_wraps_audio_source_lifecycle() {
        use std::sync::{Arc, Mutex};

        let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![3, -3]), "fixture");
        let seen_chunk = Arc::new(Mutex::new(Vec::<i16>::new()));
        let seen_chunk_for_callback = Arc::clone(&seen_chunk);
        let source = MockAudioSource::once(captured.clone());
        let mut recorder = SourceAudioRecorder::new(Box::new(source));

        assert_eq!(
            recorder.stop_and_get_buffer().unwrap_err(),
            AudioError::RecorderNotRecording
        );
        recorder
            .begin_recording(CaptureTarget::Object("mic".to_owned()))
            .unwrap();
        assert_eq!(recorder.target(), &CaptureTarget::Object("mic".to_owned()));
        assert!(recorder.is_recording());
        recorder.set_chunk_callback(Some(Box::new(move |pcm| {
            *seen_chunk_for_callback.lock().unwrap() = pcm.samples().to_vec();
        })));

        assert_eq!(recorder.stop_and_get_buffer().unwrap(), captured);
        assert!(!recorder.is_recording());
        assert_eq!(*seen_chunk.lock().unwrap(), vec![3, -3]);
    }

    #[test]
    fn mock_audio_source_returns_frames_in_order() {
        let first = CapturedAudio::named(PcmBuffer::at_default_rate(vec![1]), "first");
        let second = CapturedAudio::named(PcmBuffer::at_default_rate(vec![2]), "second");
        let mut source = MockAudioSource::from_frames(vec![first.clone(), second.clone()]);
        assert_eq!(source.read_buffer().unwrap(), first);
        assert_eq!(source.read_buffer().unwrap(), second);
        assert_eq!(
            source.read_buffer().unwrap_err(),
            AudioError::SourceExhausted
        );
    }
}
