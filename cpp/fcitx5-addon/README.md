# Fcitx5 frontend bridge

This directory is the retained thin C++ Fcitx5 frontend bridge for the Rust port.

The bridge owns only Fcitx API integration and user interaction:

- addon registration and metadata;
- trigger key/menu handling;
- a small D-Bus client wrapper over the Rust daemon ABI;
- minimal preedit/status/candidate presentation;
- committing final recognition text to Fcitx.

Backend logic must stay in Rust crates and `vinput-daemon`. The first E2E slice should call the daemon over the legacy D-Bus contract, then commit the mock or configured recognition result returned by `StopRecording`.

## First slice

The initial bridge should intentionally avoid GUI, registry install, sherpa runtime, and full PipeWire work. The target flow is:

```text
Fcitx trigger action
  -> StartRecording or StartCommandRecording(selected_text)
  -> StopRecording(scene_id)
  -> parse recognition payload JSON
  -> commit payload.commit_text or show candidates when needed
```

`include/vinput_fcitx_bridge/dbus_contract.h` mirrors `vinput-protocol::dbus` constants used by the C++ bridge. Keep it synchronized with focused tests before adding the actual addon implementation.

`include/vinput_fcitx_bridge/recognition_payload.h` and `src/recognition_payload.cpp` are pure C++ bridge-core code for parsing the legacy recognition payload and deciding whether the frontend should commit immediately or show a result candidate menu.

`include/vinput_fcitx_bridge/frontend_bridge.h` and `src/frontend_bridge.cpp` provide the pure trigger/start/stop bridge seam. The future Fcitx `AddonInstance` should translate key events into this seam and translate `BridgeOutcome` into preedit, candidate list, notification, or `commitString` calls.

## Build

The C++ bridge has its own CMake project, following the retained legacy addon build boundary. It currently builds the pure bridge core and CTest smoke binaries without requiring a live Fcitx desktop session:

```sh
just addon-configure
just addon-build
just addon-smoke
```

The CMake project also configures `vinput-addon.conf.in` and probes the legacy Fcitx addon dependencies (`Fcitx5Core`, `Fcitx5ModuleDBus`, `Fcitx5ModuleClipboard`, and `Fcitx5ModuleNotifications`) so the future shared-library addon target can follow the original C++ project's module/install shape when adapter sources land.
