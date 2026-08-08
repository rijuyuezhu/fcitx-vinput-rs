# Fcitx5 frontend bridge

This directory is the retained thin C++ Fcitx5 frontend bridge for the Rust port.

The bridge owns only Fcitx API integration and user interaction:

- addon registration and metadata;
- trigger key handling: `Control_R` press/release for normal push-to-talk recording and `F10` press/release for command-mode push-to-talk recording;
- requesting `SurroundingText` capability for input contexts and capturing command-mode selections via `InputContext::surroundingText().selectedText()`;
- a small C++ request wrapper over the Rust `zbus` transport plus an Fcitx Bus signal monitor;
- minimal preedit/status/candidate presentation;
- committing final recognition text to Fcitx.

Command-mode starts require a non-empty selection. Empty selections return the local `Please select text first.` error outcome without connecting to the daemon.

Backend logic must stay in Rust crates and `vinpst-daemon`. The E2E spine calls the daemon over the legacy D-Bus contract, then commits the mock or configured recognition result returned by `StopRecording`.

## Current slice

The retained bridge intentionally avoids GUI, registry install, sherpa runtime, and full PipeWire work. The target flow is:

```text
Fcitx trigger action
  -> StartRecording or StartCommandRecording(selected_text)
  -> StopRecording(scene_id)
  -> Rust parses and normalizes the recognition payload
  -> Rust projects one final commit, clear, error, preedit, or candidate-menu presentation
  -> C++ executes the requested Fcitx operations
```

Rust configuration normalization owns the default active Scene (`__raw__`), and the Rust frontend controller preserves the Scene captured at recording start. Command-mode starts pass an empty Scene override so the daemon keeps its command-mode default; C++ does not maintain a parallel table of Scene defaults.

`include/vinpst_fcitx_bridge/dbus_contract.h` contains only the bus identity, signal names, and status literal still required by Fcitx event-loop integration. D-Bus methods, reply types, errors, and the rest of the status contract live in Rust protocol/transport crates rather than being mirrored in C++.

`include/vinpst_fcitx_bridge/frontend_presentation.h` is only a lazy C++ view over the opaque Rust presentation: candidate count, preferred cursor, and an indexed row accessor. Rust normalizes the legacy recognition payload, decides whether the frontend should commit, clear, report an error, or show a menu, resolves empty-result fallbacks, numbers LLM alternatives, marks cancel rows, selects the default cursor, and decides whether selection replacement is required. The Rust presentation remains alive through Fcitx candidate construction and only one final row is copied when C++ creates the corresponding Fcitx object; the old raw outcome/candidate-source views and eager C++ candidate vector no longer exist.

Result candidate menus are built as Fcitx `CommonCandidateList` instances only when the Rust presentation contains candidate rows. C++ does not inspect raw/LLM/ASR/cancel source kinds or recompute labels and cursor policy. The menu is labeled `Choose Result (N)`; selecting a committing row executes the Rust-projected text and replacement flag, while a non-committing row only clears the menu/preedit state.

Empty stop payloads and cancel-only payloads are treated as explicit cleanup outcomes: they clear the recording preedit and any stale result menu without committing text.

`include/vinpst_fcitx_bridge/frontend_bridge.h` and `src/frontend_bridge.cpp` retain opaque controller/presentation ownership and copy only scalar operation fields plus the single candidate row currently needed to construct an Fcitx object. Recording, command-mode, active-scene state, semantic trigger gating, D-Bus operation selection/execution, response validation, recognition normalization, candidate presentation, and cross-client adopt-and-stop all run inside Rust. Normal start, stop, and adoption borrow the current snapshot from the Rust-owned Scene menu controller, so its active id never round-trips through C++. C++ supplies the three gettext-resolved candidate annotations through one `VinpstFcitxFrontendPresentationTextView`; Rust validates the whole view atomically before executing the completed presentation as preedit, candidate-list, notification, or `commitString` operations. There is no two-phase pending-call ABI, raw candidate-source ABI, eager presentation copy, or six-argument localization tuple.

`include/vinpst_fcitx_bridge/fcitx_trigger_mode.h` and `src/fcitx_trigger_mode.cpp` retain the Fcitx-specific trigger adapter: they apply Fcitx modifier-release semantics and keep native timer scheduling plus the associated `fcitx::Key` tokens. Tap/Hold/Both debounce, pending-start, active-trigger, release, pending-stop decisions, and the final session-state gate for semantic start/stop/menu actions live in `vinpst-fcitx-core` behind opaque `vinpst-fcitx-ffi` handles.

`include/vinpst_fcitx_bridge/fcitx_menu_filter.h` and `fcitx_menu_projection.h` retain only Fcitx key-to-semantic classification, gettext lookup of stable fragments, candidate-object creation, cursor calls, callbacks, input-panel publication, grouped platform menu state, and RAII ownership of opaque Rust handles. The daemon decodes Scene and ASR replies directly into Rust-owned menu controllers; no snapshot handle crosses the FFI. Rust owns each controller's latest snapshot, each menu session's open/closed state, current page, filter lifecycle, release handling, two-stage Escape behavior, paging targets, digit/Enter selection, terminal close/select transitions, row ordering, stable label fallback, provider-kind/loading/current-backend label composition, active-row exclusion, effective-ASR fallback decisions, filtering, and visible-row control-command projection. Scene and ASR use one `VinpstFcitxMenuProjection` ABI and one C++ `MenuProjection` wrapper; ASR localization crosses as one `VinpstFcitxAsrMenuTextView` and is validated atomically. The shared C++ projected-menu shell only opens, rebuilds, creates and publishes `CommonCandidateList` objects, invokes indexed callbacks, and executes the final Rust control. Query text, page mirrors, visibility mirrors, source snapshot indexes, raw snapshot objects, specialized projection types, and duplicate control vectors never cross into C++.

`rust_handle.h` provides the single move-only ownership wrapper for every Rust opaque handle used by the addon, while `rust_string.h` is the only C++ byte/string-view conversion boundary. On the Rust side, `ffi_string.rs` is the only production module that constructs borrowed string views or validates raw UTF-8 pointers. Individual adapters do not hand-roll deleters, reinterpret UTF-8 pointers, or call `from_raw_parts`.

`include/vinpst_fcitx_bridge/sd_bus_daemon_client.h` and `src/sd_bus_daemon_client.cpp` are a compatibility-named C++ RAII wrapper over the safe `vinpst-fcitx-dbus` `zbus::blocking` transport and typed `vinpst-fcitx-ffi` calls. Rust performs session-bus connection, method selection, typed calls, tuple decoding, response-type validation, error capture, and direct Scene/ASR controller refresh. Successful calls return their typed value directly; failures return only an opaque Rust-owned UTF-8 error string, so the old success/error response union and `is_error` decoding no longer cross into C++. ASR provider/model selection crosses as one `VinpstFcitxAsrTargetView`; the wrapper otherwise only copies status/error text, receives persisted booleans, and forwards opaque controller handles. It never owns a Scene or ASR snapshot. Diagnostic and adapter lifecycle methods remain available through Rust protocol/CLI surfaces instead of expanding this C++ seam. The separate signal monitor retains Fcitx Bus owner matching and mechanical signal tuple extraction because those callbacks must run inside the Fcitx event-loop boundary. Control events, status preedit context, and notification fields cross in `VinpstFcitxDaemonControlView`, `VinpstFcitxDaemonStatusView`, and `VinpstFcitxDaemonNotificationView`; Rust atomically validates these views and owns live status, partial text, partial deduplication, command-mode association, epoch reset, preedit priority, and notification planning. C++ retains only the watched Fcitx `InputContext` and applies the final rendered preedit or notification.

The Rust blocking transport retains the legacy 60-second D-Bus method deadline so an unresponsive daemon cannot block the Fcitx frontend indefinitely. `scripts/tests/check_fcitx_ffi_abi.py` builds the static library and requires its complete `vinpst_fcitx_*` symbol set to match the public C header exactly; the normal C++ smoke suite then exercises the opaque ownership and view contracts through that header.

## Build

The C++ bridge keeps its CMake addon/install boundary, while CMake builds and statically links `vinpst-fcitx-ffi` and its safe `vinpst-fcitx-dbus` transport dependency. The project builds the retained platform adapters and CTest smoke binaries without requiring a live Fcitx desktop session. When `Fcitx5Core` development files are available, it also builds the retained `fcitx5-vinpst.so` module target.

```sh
just addon-configure
just addon-build
just addon-smoke
just addon-fcitx-build
just addon-install-smoke
just ime-install-smoke
just ime-configured-install-smoke
just ime-pipewire-live
just ime-configured-pipewire-live
```

Run `scripts/tests/cpp/run-cpp-dbus-smoke.sh` to start the Rust daemon under `dbus-run-session` and exercise the frontend-used C++ request surface through the real D-Bus ABI. The smoke covers daemon status, Scene and ASR display snapshots, target selection, normal recording, and command-mode recording with selected text; the mock daemon must return `mock recognition result` and `mock command result for: selected text`. Diagnostic and adapter lifecycle coverage lives in Rust daemon/CLI gates, including `scripts/tests/daemon/run-dbus-adapter-lifecycle-smoke.sh`. Run `just addon-dbus-pipewire-live` on a desktop PipeWire session to repeat the same frontend path with the live PipeWire recorder worker selected by `--audio-backend pipewire`. Run `just ime-pipewire-live` for the staged install variant that activates the PipeWire-enabled daemon through the generated D-Bus service. Run `just ime-configured-pipewire-live` when the same staged activation path should also exercise configured command ASR/text adapters.

The CMake project also configures `vinpst-addon.conf.in`, the D-Bus activation service from `data/org.fcitx.Vinpst.service.in`, and the systemd user unit from `data/vinpst-daemon.service.in`. System installs route D-Bus activation through `vinpst-daemon.service`, while `VINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF` removes both the unit and the `SystemdService=` hint for environments that require direct `Exec=` activation. The project probes the legacy Fcitx addon dependencies (`Fcitx5Core`, `Fcitx5ModuleDBus`, `Fcitx5ModuleClipboard`, and `Fcitx5ModuleNotifications`) so the retained addon sources follow the original C++ project's module/install shape.

## Local daemon workflow

The current local validation path keeps the daemon and addon bridge explicit. For manual session-bus testing, run the daemon in one terminal:

```sh
cargo run -p vinpst-daemon -- --dbus
```

Add `--configured-backends --config <path>` when testing configured command ASR or text adapters instead of the mock runtime.

For automated checks, prefer:

```sh
scripts/tests/cpp/run-cpp-dbus-smoke.sh
just demo
```

`scripts/tests/cpp/run-cpp-dbus-smoke.sh` wraps a private `dbus-run-session` and verifies the retained `SdBusDaemonClient` against the Rust daemon without requiring a live desktop. `just addon-dbus-pipewire-live` is the explicit desktop-only variant for the same bridge path with live PipeWire capture; it sets `VINPST_DBUS_SMOKE_RECORD_MS=100` so the bridge waits briefly between `StartRecording` and `StopRecording`. `just addon-dbus-activation-smoke` uses a staged D-Bus service file instead of manually launching the daemon, so it validates the activation path that a packaged Fcitx addon relies on. `just addon-dbus-configured-activation-smoke` repeats that activation path with `--configured-backends` and deterministic demo WAV input. `just ime-configured-activation-smoke` runs the same configured activation path from a staged install tree containing the daemon, addon, config, and demo WAV. `just ime-pipewire-live` repeats the staged activation shape with a PipeWire-enabled daemon and `--dbus --audio-backend pipewire`, but remains explicit desktop-only. `just demo` remains the deterministic file-input command ASR/text demo for backend-only validation.

For install-shape validation, use `just addon-install-smoke`; it stages the generated module, `vinpst.conf`, `org.fcitx.Vinpst.service`, and `vinpst-daemon.service` under `target/tmp/fcitx-addon-install-smoke` rather than installing into the host Fcitx prefix. The D-Bus service retains an `Exec=` fallback and names `vinpst-daemon.service` through `SystemdService=`, while the unit owns the matching `Type=dbus`, bus name, and daemon command. The smoke also configures a no-systemd build and proves that both the unit and hint disappear together. The default generated service relies on `vinpst-daemon`'s normal direct-start service semantics instead of forcing the hidden deterministic `--dbus` test mode. Packagers or local E2E installs can still override `VINPST_DAEMON_ARGS`, for example `-DVINPST_DAEMON_ARGS="--dbus --configured-backends --config /path/to/config.json"`, when they need an explicit config or test seam. PipeWire-enabled release packages currently pass `--dbus --configured-backends --audio-backend pipewire` explicitly so the installed unit records the production runtime contract rather than relying on build-feature inference.

For manual local installs, `vinpst activation-service --daemon /path/to/vinpst-daemon --configured-backends --config /path/to/config.json --audio-backend pipewire` prints direct-`Exec=` `org.fcitx.Vinpst.service` content. Add `--output ~/.local/share/dbus-1/services/org.fcitx.Vinpst.service` to write it after choosing daemon/config paths for the current machine. This per-user helper intentionally does not add `SystemdService=` because it does not install a matching user unit.
