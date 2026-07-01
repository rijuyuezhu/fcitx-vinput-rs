# Fcitx5 frontend bridge

This directory is the retained thin C++ Fcitx5 frontend bridge for the Rust port.

The bridge owns only Fcitx API integration and user interaction:

- addon registration and metadata;
- trigger key handling: `Control_R` release for normal recording and `F10` release for command-mode recording;
- command-mode selection capture via `InputContext::surroundingText().selectedText()`;
- a small D-Bus client wrapper over the Rust daemon ABI;
- minimal preedit/status/candidate presentation;
- committing final recognition text to Fcitx.

Command-mode starts require a non-empty selection. Empty selections return the local `Please select text first.` error outcome without connecting to the daemon.

Backend logic must stay in Rust crates and `vinput-daemon`. The E2E spine calls the daemon over the legacy D-Bus contract, then commits the mock or configured recognition result returned by `StopRecording`.

## Current slice

The retained bridge intentionally avoids GUI, registry install, sherpa runtime, and full PipeWire work. The target flow is:

```text
Fcitx trigger action
  -> StartRecording or StartCommandRecording(selected_text)
  -> StopRecording(scene_id)
  -> parse recognition payload JSON
  -> commit payload.commit_text or show candidates when needed
```

Normal trigger stops use the committed raw scene id `__raw__`; command-mode trigger stops pass an empty scene id so the daemon keeps its command-mode default. `include/vinput_fcitx_bridge/scene_defaults.h` is the single C++ source of truth for these default ids and is shared by addon-facing code and D-Bus smoke tests.

`include/vinput_fcitx_bridge/dbus_contract.h` mirrors `vinput-protocol::dbus` constants used by the C++ bridge. Keep it synchronized with focused tests as the retained addon behavior evolves.

`include/vinput_fcitx_bridge/recognition_payload.h` and `src/recognition_payload.cpp` are pure C++ bridge-core code for parsing the legacy recognition payload and deciding whether the frontend should commit immediately or show a result candidate menu.

Result candidate menus are built as Fcitx `CommonCandidateList` instances only when a payload carries multiple `llm` candidates. Raw/asr candidates and single-LLM payloads commit immediately through the default candidate. The menu is labeled `Choose Result (N)`, selecting a candidate commits that candidate text, and cancel candidates only clear the menu/preedit state. Command-mode result commits and candidate selections first replace the current selected surrounding text so the command output edits the original selection instead of appending after it.

Empty stop payloads and cancel-only payloads are treated as explicit cleanup outcomes: they clear the recording preedit and any stale result menu without committing text.

`include/vinput_fcitx_bridge/frontend_bridge.h` and `src/frontend_bridge.cpp` provide the pure trigger/start/stop bridge seam. `FcitxVinputAddon` translates key events into this seam and translates `BridgeOutcome` into preedit, candidate list, notification, or `commitString` calls.

`include/vinput_fcitx_bridge/sd_bus_daemon_client.h` and `src/sd_bus_daemon_client.cpp` provide the concrete `sd-bus` implementation of the daemon client seam. It calls the Rust daemon's legacy `StartRecording`, `StartCommandRecording`, and `StopRecording` methods over the session bus; Fcitx-specific UI logic still stays outside this wrapper.

## Build

The C++ bridge has its own CMake project, following the retained legacy addon build boundary. It builds the bridge core, the concrete `sd-bus` daemon client, and CTest smoke binaries without requiring a live Fcitx desktop session. When `Fcitx5Core` development files are available, it also builds the retained `fcitx5-vinput.so` module target.

```sh
just addon-configure
just addon-build
just addon-smoke
just addon-fcitx-build
just addon-install-smoke
```

Run `just addon-dbus-smoke` to start the Rust daemon under `dbus-run-session` and exercise the C++ `SdBusDaemonClient` through the real legacy D-Bus ABI. The smoke covers both normal recording and command-mode recording with selected text, expecting the mock daemon to return `mock recognition result` and `mock command result for: selected text`.

The CMake project also configures `vinput-addon.conf.in`, configures the D-Bus activation service from `data/org.fcitx.Vinput.service.in`, and probes the legacy Fcitx addon dependencies (`Fcitx5Core`, `Fcitx5ModuleDBus`, `Fcitx5ModuleClipboard`, and `Fcitx5ModuleNotifications`) so the retained addon sources follow the original C++ project's module/install shape.

## Local daemon workflow

The current local validation path keeps the daemon and addon bridge explicit. For manual session-bus testing, run the daemon in one terminal:

```sh
cargo run -p vinput-daemon -- --dbus
```

Add `--configured-backends --config <path>` when testing configured command ASR or text adapters instead of the mock runtime.

For automated checks, prefer:

```sh
just addon-dbus-smoke
just e2e-demo
```

`just addon-dbus-smoke` wraps a private `dbus-run-session` and verifies the retained C++ `SdBusDaemonClient` against the Rust daemon without requiring a live desktop. `just e2e-demo` remains the deterministic file-input command ASR/text demo for backend-only validation.

For install-shape validation, use `just addon-install-smoke`; it stages the generated module, `vinput.conf`, and `org.fcitx.Vinput.service` under `target/tmp/fcitx-addon-install-smoke` rather than installing into the host Fcitx prefix. The activation service points at the installed Rust daemon as `vinput-daemon --dbus`, so a packaged addon trigger can activate the Rust backend without requiring the user to start it by hand first.
