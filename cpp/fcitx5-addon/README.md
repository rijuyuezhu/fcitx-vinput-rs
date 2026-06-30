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
