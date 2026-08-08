//! Capture-device discovery and user-facing selector values.

use std::fmt;

use iced::Task;
use vinpst_audio::AudioDeviceInfo;

use crate::{App, Message, blocking_task};

/// One capture-device choice shown in the Control page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDeviceChoice {
    pub(crate) value: String,
    label: String,
}

impl fmt::Display for CaptureDeviceChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Asynchronous `PipeWire` device discovery state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum AudioDeviceState {
    #[default]
    Loading,
    Ready(Vec<CaptureDeviceChoice>),
    Failed(String),
}

pub(crate) fn load_capture_devices(
    current: &str,
    default_label: &str,
) -> Result<Vec<CaptureDeviceChoice>, String> {
    vinpst_audio::pipewire_backend::enumerate_audio_sources()
        .map(|devices| capture_device_choices(current, default_label, &devices))
        .map_err(|error| format!("Could not list audio capture devices: {error}"))
}

pub(crate) fn capture_device_choices(
    current: &str,
    default_label: &str,
    devices: &[AudioDeviceInfo],
) -> Vec<CaptureDeviceChoice> {
    let mut choices = vec![CaptureDeviceChoice {
        value: "default".to_owned(),
        label: default_label.to_owned(),
    }];
    choices.extend(devices.iter().map(|device| CaptureDeviceChoice {
        value: device.name.clone(),
        label: if device.description.trim().is_empty() {
            device.name.clone()
        } else {
            device.description.clone()
        },
    }));
    if current != "default" && !choices.iter().any(|choice| choice.value == current) {
        choices.push(CaptureDeviceChoice {
            value: current.to_owned(),
            label: current.to_owned(),
        });
    }
    choices
}

impl App {
    pub(super) fn begin_audio_device_refresh(&mut self) -> Task<Message> {
        let current = self.draft.as_ref().map_or_else(
            || "default".to_owned(),
            |draft| draft.capture_device.clone(),
        );
        let default_label = self.locale.default_capture_device().to_owned();
        self.audio_devices = AudioDeviceState::Loading;
        blocking_task::perform(
            "vinpst-gui-audio-devices",
            move || load_capture_devices(&current, &default_label),
            |result| {
                Message::AudioDevicesLoaded(result.unwrap_or_else(|failure| {
                    Err(format!(
                        "Audio-device worker stopped unexpectedly: {failure}"
                    ))
                }))
            },
        )
    }

    pub(super) fn finish_audio_device_refresh(
        &mut self,
        result: Result<Vec<CaptureDeviceChoice>, String>,
    ) {
        self.audio_devices = result.map_or_else(AudioDeviceState::Failed, AudioDeviceState::Ready);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_match_upstream_default_description_and_unknown_preservation() {
        let devices = vec![
            AudioDeviceInfo::new(1, "alsa_input.usb", "USB Microphone"),
            AudioDeviceInfo::new(2, "virtual-source", ""),
        ];

        let choices = capture_device_choices("missing-source", "Default", &devices);

        assert_eq!(choices[0].value, "default");
        assert_eq!(choices[0].to_string(), "Default");
        assert_eq!(choices[1].value, "alsa_input.usb");
        assert_eq!(choices[1].to_string(), "USB Microphone");
        assert_eq!(choices[2].to_string(), "virtual-source");
        assert_eq!(choices[3].value, "missing-source");
    }
}
