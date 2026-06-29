//! Recognition event to legacy payload conversion.

use vinput_protocol::{Candidate, CandidateSource, RecognitionPayload};

use crate::{AsrError, RecognitionEvent};

/// Converts recognition events into a legacy result payload.
pub fn events_to_payload(events: &[RecognitionEvent]) -> Result<RecognitionPayload, AsrError> {
    let final_text = events.iter().find_map(|event| match event {
        RecognitionEvent::FinalText { text } => Some(text.as_str()),
        RecognitionEvent::Error { message } => Some(message.as_str()),
        RecognitionEvent::PartialText { .. } | RecognitionEvent::Completed => None,
    });

    match final_text {
        Some(text) => Ok(RecognitionPayload {
            commit_text: text.to_owned(),
            candidates: vec![Candidate::new(text, CandidateSource::Raw)],
        }),
        None => Err(AsrError::Backend(
            "recognition completed without final text".to_owned(),
        )),
    }
}
