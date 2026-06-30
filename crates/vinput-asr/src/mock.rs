//! Deterministic ASR backend for tests and early daemon wiring.

use std::sync::{Arc, Mutex};

use vinput_audio::{PcmBuffer, PcmSpec};

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};

/// Deterministic ASR backend for tests and early daemon wiring.
#[derive(Debug, Clone)]
pub struct MockAsrBackend {
    descriptor: BackendDescriptor,
    final_text: String,
    partial_text: Option<String>,
    final_timing: MockFinalTiming,
    audio_log: Option<MockAsrAudioLog>,
}

/// One observed audio push into a mock ASR session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockAsrAudioPush {
    /// Number of raw interleaved i16 samples accepted in this push.
    pub sample_len: usize,
    /// PCM layout metadata when audio was pushed through `push_pcm`.
    pub pcm_spec: Option<PcmSpec>,
}

/// Shared audio push log for deterministic mock ASR tests.
#[derive(Debug, Clone, Default)]
pub struct MockAsrAudioLog {
    pushes: Arc<Mutex<Vec<MockAsrAudioPush>>>,
}

impl MockAsrAudioLog {
    /// Creates an empty audio push log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of recorded audio pushes.
    #[must_use]
    pub fn records(&self) -> Vec<MockAsrAudioPush> {
        self.pushes.lock().expect("mock audio log poisoned").clone()
    }

    fn record(&self, push: MockAsrAudioPush) {
        self.pushes
            .lock()
            .expect("mock audio log poisoned")
            .push(push);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockFinalTiming {
    OnFinish,
    Early,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MockSessionState {
    Active,
    Finished,
    Cancelled,
}

impl MockAsrBackend {
    /// Creates a buffered mock backend with fixed final text.
    #[must_use]
    pub fn buffered(final_text: impl Into<String>) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                "mock",
                "mock-buffered",
                "Mock buffered ASR",
                BackendCapabilities::buffered(),
            ),
            final_text: final_text.into(),
            partial_text: None,
            final_timing: MockFinalTiming::OnFinish,
            audio_log: None,
        }
    }

    /// Creates a streaming mock backend with fixed partial and final text.
    #[must_use]
    pub fn streaming(partial_text: impl Into<String>, final_text: impl Into<String>) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                "mock",
                "mock-streaming",
                "Mock streaming ASR",
                BackendCapabilities::streaming(),
            ),
            final_text: final_text.into(),
            partial_text: Some(partial_text.into()),
            final_timing: MockFinalTiming::OnFinish,
            audio_log: None,
        }
    }

    /// Attaches a shared log that records every audio push into mock sessions.
    #[must_use]
    pub fn with_audio_log(mut self, audio_log: MockAsrAudioLog) -> Self {
        self.audio_log = Some(audio_log);
        self
    }

    /// Creates a streaming mock backend that emits its final text before the session is closed.
    #[must_use]
    pub fn streaming_with_early_final(
        partial_text: impl Into<String>,
        final_text: impl Into<String>,
    ) -> Self {
        Self {
            final_timing: MockFinalTiming::Early,
            ..Self::streaming(partial_text, final_text)
        }
    }
}

impl AsrBackend for MockAsrBackend {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(MockRecognitionSession {
            final_text: self.final_text.clone(),
            partial_text: self.partial_text.clone(),
            final_timing: self.final_timing,
            audio_log: self.audio_log.clone(),
            accepted_samples: 0,
            state: MockSessionState::Active,
            partial_sent: false,
            final_sent: false,
            events: Vec::new(),
        }))
    }
}

#[derive(Debug)]
struct MockRecognitionSession {
    final_text: String,
    partial_text: Option<String>,
    final_timing: MockFinalTiming,
    audio_log: Option<MockAsrAudioLog>,
    accepted_samples: usize,
    state: MockSessionState,
    partial_sent: bool,
    final_sent: bool,
    events: Vec<RecognitionEvent>,
}

impl MockRecognitionSession {
    fn accept_audio(&mut self, samples: &[i16], pcm_spec: Option<PcmSpec>) -> Result<(), AsrError> {
        match self.state {
            MockSessionState::Active => {}
            MockSessionState::Finished => return Err(AsrError::AlreadyFinished),
            MockSessionState::Cancelled => return Err(AsrError::Cancelled),
        }
        self.accepted_samples += samples.len();
        if let Some(audio_log) = &self.audio_log {
            audio_log.record(MockAsrAudioPush {
                sample_len: samples.len(),
                pcm_spec,
            });
        }
        if !self.partial_sent
            && let Some(text) = &self.partial_text
        {
            self.events
                .push(RecognitionEvent::PartialText { text: text.clone() });
            self.partial_sent = true;
        }
        if self.final_timing == MockFinalTiming::Early && !self.final_sent {
            self.events.push(RecognitionEvent::FinalText {
                text: self.final_text.clone(),
            });
            self.final_sent = true;
        }
        Ok(())
    }
}

impl RecognitionSession for MockRecognitionSession {
    fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        self.accept_audio(pcm.samples(), Some(pcm.spec()))
    }

    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        self.accept_audio(samples, None)
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        match self.state {
            MockSessionState::Active => {}
            MockSessionState::Finished => return Err(AsrError::AlreadyFinished),
            MockSessionState::Cancelled => return Err(AsrError::Cancelled),
        }
        self.state = MockSessionState::Finished;
        if !self.final_sent {
            self.events.push(RecognitionEvent::FinalText {
                text: self.final_text.clone(),
            });
            self.final_sent = true;
        }
        self.events.push(RecognitionEvent::Completed);
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.state = MockSessionState::Cancelled;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}
