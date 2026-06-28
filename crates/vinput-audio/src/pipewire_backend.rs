//! Feature-gated `PipeWire` backend scaffolding.
//!
//! This module intentionally starts with linkage probing only. Real recording,
//! context creation, and device enumeration should implement the crate-level
//! `AudioRecorder` and `AudioDeviceEnumerator` contracts once the `PipeWire`
//! event-loop ownership model is fixed.

use crate::AudioError;

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

fn pipewire_error(error: impl std::fmt::Display) -> AudioError {
    AudioError::DeviceEnumerationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
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
