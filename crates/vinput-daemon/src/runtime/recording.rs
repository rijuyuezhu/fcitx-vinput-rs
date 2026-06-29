//! Recording lifecycle, ASR event draining, and stop-time text finishing.

use vinput_asr::{RecognitionContext, RecognitionEvent, RecognitionSession, events_to_payload};
use vinput_audio::{AudioProcessingOptions, PcmBuffer};
use vinput_protocol::{RecognitionPayload, ServiceStatus};
use vinput_text::TextRequest;

use super::{MOCK_SILENCE_THRESHOLD, RuntimeError, RuntimeState, StopRecordingReport};

impl RuntimeState {
    /// Starts normal recording.
    pub fn start_recording(&mut self) -> Result<(), RuntimeError> {
        self.start_recording_internal(self.config.scenes.active_scene.clone(), None)
    }

    /// Starts command-mode recording.
    pub fn start_command_recording(
        &mut self,
        selected_text: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        self.start_recording_internal(
            vinput_config::COMMAND_SCENE_ID.to_owned(),
            Some(selected_text.into()),
        )
    }

    /// Stops recording and returns a deterministic mock result payload.
    pub fn stop_recording(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<RecognitionPayload, RuntimeError> {
        Ok(self.stop_recording_report(scene_id)?.payload)
    }

    /// Stops recording and returns final payload plus stop-time ASR metadata.
    pub fn stop_recording_report(
        &mut self,
        scene_id: Option<&str>,
    ) -> Result<StopRecordingReport, RuntimeError> {
        if self.status != ServiceStatus::Recording {
            return Err(RuntimeError::NotRecording(self.status));
        }

        self.status = ServiceStatus::Inferring;
        let scene = scene_id
            .map(ToOwned::to_owned)
            .or_else(|| self.current_scene.clone())
            .unwrap_or_else(|| self.config.scenes.active_scene.clone());

        let result = (|| {
            let mut session = self
                .active_session
                .take()
                .ok_or(RuntimeError::MissingAsrSession)?;
            let pcm = match self.stop_and_process_recording() {
                Ok(pcm) => pcm,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            };
            if let Err(error) = session.push_pcm(&pcm) {
                let _ = session.cancel();
                return Err(RuntimeError::Asr(error));
            }
            let mut events = match self.drain_pending_events(&mut *session) {
                Ok(events) => events,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(error);
                }
            };
            if let Err(error) = session.finish() {
                let _ = session.cancel();
                return Err(RuntimeError::Asr(error));
            }
            match session.poll_events() {
                Ok(new_events) => events.extend(new_events),
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Asr(error));
                }
            }
            let partial_text = latest_partial_text(&events).or_else(|| self.partial_text.clone());
            let raw_payload = match events_to_payload(&events) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Asr(error));
                }
            };
            let scene_definition = self.scene_definition(&scene);
            let payload = match self.text_processor.finish(&TextRequest {
                raw_text: &raw_payload.commit_text,
                scene: &scene_definition,
                selected_text: self.selected_text.as_deref(),
            }) {
                Ok(payload) => payload,
                Err(error) => {
                    let _ = session.cancel();
                    return Err(RuntimeError::Finish(error));
                }
            };
            Ok(StopRecordingReport {
                payload,
                partial_text,
            })
        })();

        if result.is_err() && self.audio_recorder.is_recording() {
            let _ = self.audio_recorder.cancel_recording();
        }
        self.audio_recorder.set_chunk_callback(None);
        self.reset_to_idle();
        result
    }

    fn start_recording_internal(
        &mut self,
        scene_id: String,
        selected_text: Option<String>,
    ) -> Result<(), RuntimeError> {
        self.ensure_idle()?;
        let capture_target = self.capture_target_for_runtime()?;
        let context = self.recognition_context(&scene_id, selected_text.as_deref());
        let mut session = self
            .asr_backend
            .create_session(context)
            .map_err(RuntimeError::Asr)?;
        if let Err(error) = self.audio_recorder.begin_recording(capture_target) {
            let _ = session.cancel();
            return Err(RuntimeError::Audio(error));
        }
        self.status = ServiceStatus::Recording;
        self.current_scene = Some(scene_id);
        self.selected_text = selected_text;
        self.active_session = Some(session);
        Ok(())
    }

    fn drain_pending_events(
        &mut self,
        session: &mut dyn RecognitionSession,
    ) -> Result<Vec<RecognitionEvent>, RuntimeError> {
        let mut events = Vec::new();
        for event in session.poll_events().map_err(RuntimeError::Asr)? {
            if let vinput_asr::RecognitionEvent::PartialText { text } = &event {
                self.partial_text = Some(text.clone());
            }
            events.push(event);
        }
        Ok(events)
    }

    fn recognition_context(
        &self,
        scene_id: &str,
        selected_text: Option<&str>,
    ) -> RecognitionContext {
        if scene_id == vinput_config::COMMAND_SCENE_ID {
            RecognitionContext::command(
                scene_id.to_owned(),
                Some(self.config.global.default_language.clone()),
                selected_text.unwrap_or_default().to_owned(),
            )
        } else {
            RecognitionContext::normal(
                scene_id.to_owned(),
                Some(self.config.global.default_language.clone()),
            )
        }
    }

    fn stop_and_process_recording(&mut self) -> Result<PcmBuffer, RuntimeError> {
        let captured = self
            .audio_recorder
            .stop_and_get_buffer()
            .map_err(RuntimeError::Audio)?;
        Ok(self.process_captured_pcm(&captured.pcm))
    }

    fn process_captured_pcm(&self, pcm: &PcmBuffer) -> PcmBuffer {
        self.audio_processing_options().process(pcm)
    }

    fn audio_processing_options(&self) -> AudioProcessingOptions {
        AudioProcessingOptions::new(
            MOCK_SILENCE_THRESHOLD,
            self.config.asr.normalize_audio.then_some(16_000),
            self.config.asr.input_gain,
        )
    }

    fn scene_definition(&self, scene_id: &str) -> vinput_config::SceneDefinition {
        self.config
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == scene_id)
            .cloned()
            .unwrap_or_else(|| vinput_config::SceneDefinition {
                id: scene_id.to_owned(),
                label: scene_id.to_owned(),
                prompt: None,
                provider_id: None,
                model: None,
                candidate_count: 0,
                timeout_ms: None,
                context_lines: 0,
            })
    }

    fn reset_to_idle(&mut self) {
        self.status = ServiceStatus::Idle;
        self.current_scene = None;
        self.selected_text = None;
        self.partial_text = None;
        self.active_session = None;
        self.apply_pending_asr_backend_reload();
    }
}

fn latest_partial_text(events: &[RecognitionEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| match event {
        RecognitionEvent::PartialText { text } => Some(text.clone()),
        RecognitionEvent::FinalText { .. }
        | RecognitionEvent::Error { .. }
        | RecognitionEvent::Completed => None,
    })
}
