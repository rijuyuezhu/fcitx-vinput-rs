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
    vinput-postprocess  # scene prompts, LLM adapters, command-mode text transforms
    vinput-registry     # registry metadata, download, safe extraction, materialization
    vinput-daemon       # async runtime, D-Bus service, orchestration actors
    vinput-cli          # clap CLI over protocol/config/daemon APIs
  cpp/
    fcitx5-addon        # retained thin AddonInstance frontend bridge
    gui-qt              # optional retained GUI until a Rust UI decision is made
  data/
  docs/
```

The current demo has `vinput-protocol`, `vinput-config`, `vinput-daemon`, and `vinput-cli`. The remaining crates should be added when their first TDD slice is ready.

## Runtime actors

```text
Fcitx5 addon (C++)
  └─ D-Bus methods/signals using vinput-protocol ABI
      └─ vinput-daemon::dbus
          └─ Runtime actor
              ├─ Audio capture task          -> vinput-audio
              ├─ ASR session task            -> vinput-asr
              ├─ Postprocess task            -> vinput-postprocess
              ├─ Adapter supervisor task     -> vinput-postprocess / vinput-daemon
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
- Registry metadata and planning behavior: see `docs/architecture/registry-contract.md`

Any change to this crate must include compatibility tests.

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
