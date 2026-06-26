# D-Bus service milestone

This milestone introduces the first real daemon-side D-Bus boundary while keeping the runtime mocked.

## What exists now

- `crates/vinput-daemon/src/lib.rs` exports daemon library modules for tests and future integration.
- `crates/vinput-daemon/src/runtime.rs` remains the deterministic mock state machine.
- `crates/vinput-daemon/src/dbus_service.rs` wraps `RuntimeState` in a `zbus` interface named `org.fcitx.Vinput.Service`.
- `vinput-daemon --dbus` registers the legacy bus/object/interface on the session bus.

The service exposes the legacy method names:

- `StartRecording`
- `StartCommandRecording`
- `StopRecording`
- `GetStatus`
- `GetAsrBackendState`
- `ReloadAsrBackend`
- `StartAdapter`
- `StopAdapter`
- `Notify`

It also declares the legacy signal names:

- `RecognitionResult`
- `RecognitionPartial`
- `StatusChanged`
- `DaemonNotification`

## Current test coverage

The first tests call the service facade directly and assert that the mock D-Bus methods exercise the same state transitions and JSON payloads as the runtime:

- idle → recording → stop → idle
- command recording with selected text context
- ASR backend state JSON parsing

A later milestone should add `dbus-run-session` integration tests with a `zbus` client proxy. That should happen before the C++ addon is pointed at the Rust daemon.

## Compatibility rule

The C++ frontend should not know whether the backend is C++ or Rust. Any service method rename, object path change, status string change, or recognition payload shape change must be caught by `vinput-protocol` tests before it reaches D-Bus integration.
