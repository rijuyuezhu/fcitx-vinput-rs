//! Feature-gated `PipeWire` backend scaffolding.
//!
//! This module intentionally starts with linkage and context probing only. Real
//! recording and device enumeration should implement the crate-level
//! `AudioRecorder` and `AudioDeviceEnumerator` contracts once the `PipeWire`
//! event-loop ownership model is fixed.

use crate::AudioError;

/// Initialize the `PipeWire` client library.
pub fn initialize() {
    pipewire::init();
}

/// Create the minimal `PipeWire` main loop and context objects.
///
/// This proves that the optional `PipeWire` bindings and system development
/// headers link correctly without connecting to the user's session daemon.
pub fn probe_client_context() -> Result<(), AudioError> {
    initialize();
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
    fn pipewire_probe_creates_client_context() {
        super::probe_client_context().unwrap();
    }
}
