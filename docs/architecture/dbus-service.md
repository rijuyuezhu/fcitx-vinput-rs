# D-Bus service milestone

This milestone introduces the first real daemon-side D-Bus boundary while keeping the runtime mocked.

## What exists now

- `crates/vinput-daemon/src/lib.rs` exports daemon library modules for tests and future integration.
- `crates/vinput-daemon/src/runtime.rs` remains the deterministic mock state machine.
- `crates/vinput-daemon/src/dbus_service.rs` wraps `RuntimeState` in a `zbus` interface named `org.fcitx.Vinput.Service`.
- `vinput-daemon --dbus` registers the legacy bus/object/interface on the session bus.
- `crates/vinput-daemon/tests/dbus_integration.rs` uses `zbus::Proxy` under `dbus-run-session` to exercise real bus calls.

The service exposes compatibility and diagnostic method names:

- `StartRecording`
- `StartCommandRecording`
- `StopRecording`
- `GetStatus`
- `GetAsrBackendState`
- `GetTextAdapterState`
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

Unit tests call the service facade directly and assert that the mock D-Bus methods exercise the same state transitions and JSON payloads as the runtime:

- idle → recording → stop → idle
- command recording with selected text context
- ASR backend state JSON parsing

The optional integration test runs through a real session bus:

```sh
dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration
```

That test starts the Rust service, builds a `zbus::Proxy`, calls legacy methods by their exact wire names, and parses the returned recognition payload JSON.

## Compatibility rule

The C++ frontend should not know whether the backend is C++ or Rust. Any service method rename, object path change, status string change, or recognition payload shape change must be caught by `vinput-protocol` tests before it reaches D-Bus integration.
