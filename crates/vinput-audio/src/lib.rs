//! Pure PCM audio utilities used before the real `PipeWire` capture layer lands.
//!
//! This crate deliberately starts without `PipeWire`.  It owns typed PCM buffers
//! and deterministic transforms so audio behavior can be tested independently
//! from desktop/audio-server integration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
    /// Empty mock buffer list.
    #[error("no more buffers")]
    SourceExhausted,
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
        AudioError, CapturedAudio, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ, PcmBuffer, PcmSpec,
    };

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
    fn captured_audio_reports_pcm_duration_and_source() {
        let captured =
            CapturedAudio::named(PcmBuffer::new(1_000, vec![0; 250]).unwrap(), "fixture");
        assert_eq!(captured.duration_ms(), 250);
        assert_eq!(captured.source_name.as_deref(), Some("fixture"));
    }

    #[test]
    fn mock_audio_source_returns_frames_in_order() {
        use super::{AudioSource, CapturedAudio, MockAudioSource};

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
