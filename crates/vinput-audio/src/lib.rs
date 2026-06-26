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

/// Mono signed 16-bit PCM buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PcmBuffer {
    sample_rate_hz: u32,
    samples: Vec<i16>,
}

impl PcmBuffer {
    /// Creates a new PCM buffer.
    pub fn new(sample_rate_hz: u32, samples: impl Into<Vec<i16>>) -> Result<Self, AudioError> {
        if sample_rate_hz == 0 {
            return Err(AudioError::InvalidSampleRate(sample_rate_hz));
        }
        Ok(Self {
            sample_rate_hz,
            samples: samples.into(),
        })
    }

    /// Creates a 16 kHz PCM buffer.
    pub fn at_default_rate(samples: impl Into<Vec<i16>>) -> Self {
        Self {
            sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
            samples: samples.into(),
        }
    }

    /// Returns the sample rate.
    #[must_use]
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
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

    /// Returns the number of samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns duration in milliseconds, rounded down.
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        let len = u64::try_from(self.samples.len()).unwrap_or(u64::MAX);
        len.saturating_mul(1000) / u64::from(self.sample_rate_hz)
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

    /// Returns a copy with leading and trailing silence removed.
    #[must_use]
    pub fn trimmed_silence(&self, threshold_abs: i16) -> Self {
        let threshold = threshold_abs.unsigned_abs();
        let start = self
            .samples
            .iter()
            .position(|sample| sample.unsigned_abs() > threshold);
        let Some(start) = start else {
            return Self {
                sample_rate_hz: self.sample_rate_hz,
                samples: Vec::new(),
            };
        };
        let end = self
            .samples
            .iter()
            .rposition(|sample| sample.unsigned_abs() > threshold)
            .expect("start exists, so end exists");
        Self {
            sample_rate_hz: self.sample_rate_hz,
            samples: self.samples[start..=end].to_vec(),
        }
    }
}

/// Audio helper errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AudioError {
    /// Sample rate must not be zero.
    #[error("invalid sample rate: {0}")]
    InvalidSampleRate(u32),
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
    use super::{AudioError, DEFAULT_SAMPLE_RATE_HZ, PcmBuffer};

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
        assert_eq!(
            PcmBuffer::at_default_rate(vec![0]).sample_rate_hz(),
            DEFAULT_SAMPLE_RATE_HZ
        );
    }

    #[test]
    fn gain_saturates_to_i16_range() {
        let pcm = PcmBuffer::at_default_rate(vec![20_000, -20_000]).with_gain(2.0);
        assert_eq!(pcm.samples(), &[i16::MAX, i16::MIN]);
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
    }

    #[test]
    fn trim_removes_leading_and_trailing_silence() {
        let pcm = PcmBuffer::at_default_rate(vec![0, 1, 5, -6, 1, 0]).trimmed_silence(1);
        assert_eq!(pcm.samples(), &[5, -6]);
    }

    #[test]
    fn trim_all_silence_returns_empty_buffer() {
        let pcm = PcmBuffer::at_default_rate(vec![0, 1, -1]).trimmed_silence(1);
        assert!(pcm.is_empty());
        assert_eq!(pcm.sample_rate_hz(), DEFAULT_SAMPLE_RATE_HZ);
    }
}
