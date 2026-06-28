//! Feature-gated `PipeWire` backend scaffolding.
//!
//! This module intentionally starts with linkage probing only. Real recording,
//! context creation, and device enumeration should implement the crate-level
//! `AudioRecorder` and `AudioDeviceEnumerator` contracts once the `PipeWire`
//! event-loop ownership model is fixed.

use std::{cell::Cell, cell::RefCell, rc::Rc};

use crate::{
    AudioChunkCallback, AudioDeviceEnumerator, AudioDeviceInfo, AudioError, AudioRecorder,
    CaptureTarget, CapturedAudio,
};

const MEDIA_CLASS_AUDIO_SOURCE: &str = "Audio/Source";
const PW_KEY_MEDIA_CLASS: &str = "media.class";
const PW_KEY_NODE_NAME: &str = "node.name";
const PW_KEY_NODE_DESCRIPTION: &str = "node.description";

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
    target: CaptureTarget,
}

impl PipeWireAudioRecorder {
    /// Creates a recorder placeholder for future live `PipeWire` capture.
    #[must_use]
    pub fn new() -> Self {
        Self {
            target: CaptureTarget::default(),
        }
    }

    /// Returns the last target passed to `begin_recording`.
    #[must_use]
    pub const fn target(&self) -> &CaptureTarget {
        &self.target
    }
}

impl Default for PipeWireAudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioRecorder for PipeWireAudioRecorder {
    fn begin_recording(&mut self, target: CaptureTarget) -> Result<(), AudioError> {
        probe_client_linkage();
        self.target = target;
        Err(AudioError::RecordingBackendUnavailable(
            "PipeWire recorder stream is not implemented yet".to_owned(),
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
    fn pipewire_recorder_reports_unavailable_without_live_stream() {
        let mut recorder = super::PipeWireAudioRecorder::new();

        super::AudioRecorder::set_chunk_callback(&mut recorder, None);
        let error = super::AudioRecorder::begin_recording(
            &mut recorder,
            super::CaptureTarget::Object("alsa_input.usb-mic".to_owned()),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            super::AudioError::RecordingBackendUnavailable(message)
                if message.contains("PipeWire recorder stream")
        ));
        assert_eq!(
            recorder.target(),
            &super::CaptureTarget::Object("alsa_input.usb-mic".to_owned())
        );
        assert!(!super::AudioRecorder::is_recording(&recorder));
        assert_eq!(
            super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap_err(),
            super::AudioError::RecorderNotRecording
        );
        super::AudioRecorder::cancel_recording(&mut recorder).unwrap();
    }

    #[test]
    fn pipewire_enumerator_lists_sources_when_enabled() {
        if std::env::var_os("VINPUT_TEST_PIPEWIRE_ENUMERATE").is_none() {
            return;
        }
        let mut enumerator = super::PipeWireDeviceEnumerator;
        let _devices =
            super::AudioDeviceEnumerator::enumerate_audio_sources(&mut enumerator).unwrap();
    }

    #[test]
    fn pipewire_probe_creates_client_context_when_enabled() {
        if std::env::var_os("VINPUT_TEST_PIPEWIRE_CONTEXT").is_none() {
            return;
        }
        super::probe_client_context().unwrap();
    }
}
