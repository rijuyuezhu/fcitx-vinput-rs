//! Feature-gated `PipeWire` backend scaffolding.
//!
//! This module intentionally starts with linkage probing only. Real recording,
//! context creation, and device enumeration should implement the crate-level
//! `AudioRecorder` and `AudioDeviceEnumerator` contracts once the `PipeWire`
//! event-loop ownership model is fixed.

use crate::{AudioDeviceInfo, AudioError};

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
    fn pipewire_probe_creates_client_context_when_enabled() {
        if std::env::var_os("VINPUT_TEST_PIPEWIRE_CONTEXT").is_none() {
            return;
        }
        super::probe_client_context().unwrap();
    }
}
