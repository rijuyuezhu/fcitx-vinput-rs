# D-Bus service contract

`vinput-daemon` exposes the legacy daemon D-Bus ABI while the backend implementation is rewritten in Rust. The service must remain compatible with the existing C++ Fcitx5 frontend.

## What exists now

- `crates/vinput-protocol/src/dbus.rs` owns shared wire constants, method names, and signal names.
- `crates/vinput-daemon/src/dbus_service.rs` wraps `RuntimeState` in a `zbus` interface named `org.fcitx.Vinput.Service`.
- `vinput-daemon --dbus` registers the legacy bus/object/interface on the session bus.
- `crates/vinput-daemon/tests/dbus_integration.rs` exercises real bus calls under `dbus-run-session`.
- The default runtime still uses deterministic mock ASR/text/audio seams, while explicit configured paths can exercise configured command ASR/text seams. This is not full backend parity.

## Wire names to preserve

- Bus name: `org.fcitx.Vinput`
- Object path: `/org/fcitx/Vinput`
- Service interface: `org.fcitx.Vinput.Service`
- Fcitx bus: `org.fcitx.Fcitx5`
- Frontend notifier object: `/org/fcitx/Fcitx5/Vinput`
- Frontend notifier interface: `org.fcitx.Fcitx5.Vinput1`

## Service methods

Preserve these legacy method names and payload shapes:

- `StartRecording`
- `StartCommandRecording`
- `StopRecording`
- `GetStatus`
- `GetAsrBackendState`
- `ReloadAsrBackend`
- `StartAdapter`
- `StopAdapter`

`GetTextAdapterState` is a Rust diagnostic extension. It can remain available, but it is not part of the original C++ daemon vtable and should be documented as an extension whenever listed.

## Signals

Preserve these signal names and payload shapes:

- `RecognitionResult(s)`
- `RecognitionPartial(s)`
- `StatusChanged(s)`
- `DaemonNotification(ssss)`, carrying code, subject, detail, and raw message.

## Test coverage

Unit tests call the service facade directly and assert runtime transitions and JSON payloads. The optional integration test runs through a real session bus:

```sh
dbus-run-session -- cargo test -p vinput-daemon --features dbus-integration --test dbus_integration
```

That test starts the Rust service, builds a `zbus::Proxy`, calls legacy methods by their exact wire names, and parses returned recognition payload JSON.

`vinput-cli protocol` serializes method and signal names from `vinput-protocol`, so smoke commands and service tests read the same member list.

## Known compatibility gaps

These are next-phase refactor/fix items, not optional cleanups:

- D-Bus errors should preserve the legacy operation failure name `org.fcitx.Vinput.Error.OperationFailed` instead of exposing only generic failure errors.
- `ReloadAsrBackend` should match legacy busy behavior: return success while recording/inferring, mark reload pending, and apply it when the runtime returns to idle.
- Status ordering must stay covered by tests, especially when a future real post-processing phase is wired.

## Compatibility rule

The frontend should not need to know whether the daemon is C++ or Rust. Any service method rename, object path change, status string change, signal shape change, recognition payload shape change, or D-Bus error behavior change must be pinned by compatibility tests before it reaches runtime code.
