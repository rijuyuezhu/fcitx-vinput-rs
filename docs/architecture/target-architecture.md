# Target architecture

This architecture keeps the risky C++ Fcitx5 addon boundary thin while moving daemon/backend logic to Rust crates with explicit test seams.

## Top-level shape

```text
fcitx-vinput-rs/
  crates/
    vinput-protocol     # stable D-Bus/JSON ABI shared by all components
    vinput-config       # config schema, migration, normalization, validation
    vinput-audio        # PipeWire capture and pure PCM transforms
    vinput-asr          # ASR traits, mock backend, command backend, sherpa-onnx backend
    vinput-text         # scene prompts, text adapters, command-mode text transforms
    vinput-registry     # registry metadata, download, safe extraction, materialization
    vinput-daemon       # async runtime, D-Bus service, orchestration actors
    vinput-cli          # clap CLI over protocol/config/daemon APIs
  cpp/
    fcitx5-addon        # retained thin AddonInstance frontend bridge
    gui-qt              # optional retained GUI until a Rust UI decision is made
  data/
  docs/
```

The current workspace already has the pure protocol/config/audio/ASR/text/registry crates plus `vinput-daemon` and `vinput-cli`. Backend implementations can keep landing behind those seams without changing the top-level crate boundaries.

## Runtime actors

```text
Fcitx5 addon (C++)
  └─ D-Bus methods/signals using vinput-protocol ABI
      └─ vinput-daemon::dbus
          └─ Runtime actor
              ├─ Audio capture task          -> vinput-audio
              ├─ ASR session task            -> vinput-asr
              ├─ Postprocess task            -> vinput-text
              ├─ Adapter supervisor task     -> vinput-text / vinput-daemon
              ├─ Remote text service task    -> vinput-daemon::remote
              └─ Registry/install helpers    -> vinput-registry
```

## State machine

The daemon should make state transitions explicit and testable:

```text
Idle
  ├─ StartRecording / StartCommandRecording
  ▼
Recording
  ├─ RecognitionPartial*      # streaming backends only
  ├─ StopRecording
  ▼
Inferring
  ├─ ASR final/error
  ▼
Postprocessing?               # scene/LLM/command mode
  ├─ RecognitionResult / Notification
  ▼
Idle
```

Every transition should have a unit test before it is wired to D-Bus or PipeWire.

## Compatibility contracts

`vinput-protocol` owns the stable contract:

- bus name: `org.fcitx.Vinput`
- object path: `/org/fcitx/Vinput`
- interface: `org.fcitx.Vinput.Service`
- status strings: `idle`, `recording`, `inferring`, `postprocessing`, `error`
- recognition result JSON: `{ "commit_text": string, "candidates": [{ "text": string, "source": string }] }`
- ASR backend state JSON fields matching the original frontend expectations
- Config file baseline and diagnostics behavior: see `docs/architecture/config-contract.md`
- Registry metadata and planning behavior: see `docs/architecture/registry-contract.md`

Any change to this crate must include compatibility tests.


## Active E2E acceleration target

The current implementation phase prioritizes a usable product spine over further broad refactor work. The next frontend step is a retained thin C++ Fcitx5 bridge that talks to the Rust daemon over the existing protocol boundary and commits a mock or configured recognition result. Backend logic should stay in Rust crates; the frontend should own only Fcitx API integration, menu/status/preedit/candidate UI, text context, and the bus bridge.

See `docs/migration/e2e-port-plan.md` for the active Rust-vs-legacy gap list and execution phases.

## Frontend and packaging boundary

T6 should start with a retained C++ Fcitx5 skeleton frontend that talks to the Rust daemon over the existing `vinput-protocol` D-Bus ABI. The skeleton should own only Fcitx API integration, menus, preedit/status presentation, selected-text collection, and frontend-side clipboard/deletion fallbacks. Backend logic, ASR/text processing, registry operations, and runtime state must stay in Rust crates and the daemon.

Do not replace the Fcitx5 addon with a Rust addon until mature Rust bindings and deployment integration have been verified separately. Packaging/service install artifacts remain future work and must not be hidden inside daemon, registry, or frontend logic.

## TDD migration order

1. **Protocol/config locked baseline**
   - Add golden JSON tests for existing daemon/frontend payloads.
   - Add config migration tests before editing defaults.

2. **D-Bus daemon shell**
   - Add a `zbus` service that exposes legacy methods/signals.
   - Test under `dbus-run-session` using a Rust proxy.
   - Keep mock runtime behind the service first.

3. **Runtime state machine**
   - Replace the demo runtime with an actor that accepts typed commands.
   - Test busy/error/cancel/reload races without audio or ASR.

4. **Audio and ASR seams**
   - Port pure PCM transforms first.
   - Add `AsrBackend`/`RecognitionSession` trait with mock implementation.
   - Add PipeWire and sherpa-onnx behind feature/integration tests.

5. **Postprocess and command mode**
   - Port prompt rendering and command-mode behavior with fixture tests.
   - Mock LLM adapter HTTP/process edges before real adapter supervision.

6. **Registry/CLI/GUI/addon tightening**
   - Port registry parsing/download with safe extraction tests.
   - Rebuild CLI commands against typed crates.
   - Reduce C++ addon to Fcitx API, menus, preedit, and D-Bus bridge.
   - Defer GUI rewrite unless Qt maintenance becomes worse than a Rust UI.

## What not to port mechanically

- Raw HTTP/WebSocket code in `daemon/remote`: replace with a Rust HTTP/WebSocket stack.
- Generic path/file/process/string utilities: prefer well-maintained Rust crates.
- C++ daemon poll loop: replace with structured async tasks and explicit shutdown.
- Ad-hoc JSON parsing: use typed serde models and golden fixtures.
