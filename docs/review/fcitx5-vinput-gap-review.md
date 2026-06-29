# fcitx5-vinput gap review

## Executive summary

This review compares the legacy C++ project at `/workspace/fcitx5-vinput` with the Rust rewrite at `/workspace/fcitx-vinput-rs`. It was originally written around Rust HEAD `0e0ba40 test(cli): share stdout helper`; this copy has been refreshed for the later audio diagnostics, stateful recorder seam, D-Bus signal-order coverage, and PipeWire feature scaffolding now present in the workspace. The Rust workspace has established useful crate boundaries, typed config/protocol contracts, deterministic mock audio/ASR/text runtime seams, D-Bus smoke coverage, registry fixtures, docs guards, and CI. It is not yet functionally equivalent to the legacy plugin.

The most important compatibility risk remains the daemon D-Bus ABI and frontend integration. The legacy daemon exposes `org.fcitx.Vinput` at `/org/fcitx/Vinput` with `org.fcitx.Vinput.Service`; Rust now has signature/introspection coverage for the current facade, but compatibility still depends on preserving legacy-visible method names, return shapes, signal ordering, and frontend expectations while real backends are added.

The second major gap is runtime completeness. The legacy daemon has a real PipeWire capture path, local sherpa-onnx offline and streaming ASR, Silero VAD trimming, command ASR backends, OpenAI-compatible post-processing, adapter supervision, registry download/install flows, remote text input, and packaging. The Rust project now has stronger runtime contracts, stateful recorder ownership, command-process seams, audio diagnostics/device enumeration under the PipeWire feature, and deterministic D-Bus tests. It still lacks live PipeWire recording, sherpa-onnx, full legacy ASR/text behavior, install artifacts, frontend integration, and packaging.

The rewrite should not line-port the C++ architecture.  It should preserve user-visible interfaces and behavior while improving safety: exact D-Bus contract tests, redacted secret logging, safe archive extraction tests, typed error taxonomy, backend isolation, and small crate-owned responsibilities.

## Review inputs

- Legacy source: `/workspace/fcitx5-vinput`, HEAD `aad130341bcb30f1dc15d8210246930384760950`.
- Rust source: `/workspace/fcitx-vinput-rs`; original review HEAD `0e0ba40d62eb4163b82f08e2b6e5dde788a6c74d`, refreshed through current D-Bus/reload/adapter/command ASR review work on `main`.
- Rust HEAD check-runs at original takeover: `rust` completed successfully; later CI now also covers D-Bus integration and PipeWire feature guards.
- Source review used `find`, `rg`, `sed`, Cargo manifests, tests, docs, workflows, and packaging files in both repositories.
- External dependency spot checks:
  - PipeWire Rust bindings: `pipewire` crate/docs (`https://docs.rs/pipewire/latest/pipewire/`) show current Rust bindings.
  - sherpa-onnx upstream (`https://github.com/k2-fsa/sherpa-onnx`) documents local ASR, streaming/non-streaming, VAD, and Rust/C APIs.
  - Fcitx5 addon documentation (`https://www.fcitx-im.org/wiki/Develop_an_simple_input_method`) describes shared-library addon layout and C++/CMake-oriented integration.
  - `cargo search` found `sherpa-onnx-sys`, `sherpa-onnx`, `pipewire`, and a small `fcitx5-dbus` crate.  A mature official Rust Fcitx5 addon binding was not identified in the quick check, so the frontend strategy needs validation.

## Legacy feature map

### Fcitx5 addon/front-end

- Entry point and lifecycle are in `src/addon/core`, with a shared-library addon installed under the Fcitx5 addon path.
- Input handling is in `src/addon/input/vinput_keyevent.cpp`:
  - trigger key modes for hold, tap, and both;
  - command-mode start with selected text;
  - scene-menu key handling;
  - deferred stop scheduling;
  - busy/daemon unavailable paths.
- UI behavior is split across preedit, candidate lists, menu code, and notification glue:
  - status preedit strings for starting, recording, command mode, inferring, postprocessing, daemon unavailable, and timeout/not responding;
  - candidate menus for ASR provider, scene, and result selection;
  - Fcitx notification forwarding through `/org/fcitx/Fcitx5/Vinput` and `org.fcitx.Fcitx5.Vinput1`.
- The frontend is tightly coupled to the daemon D-Bus contract and status semantics, but it does not require the daemon to preserve internal C++ class structure.

### Daemon/backend process model

- `vinput-daemon` owns D-Bus service registration, audio capture, ASR backend/session lifecycle, post-processing, remote text service, and adapter supervision.
- `DaemonRuntimeController` tracks phases `idle`, `recording`, `inferring`, `postprocessing`, and `error`.
- Start is rejected when non-idle; ASR reload is deferred while recording and applied when idle.
- Streaming sessions receive chunks while recording; buffered sessions consume all PCM after stop.
- Errors produce D-Bus notifications and phase changes.

### D-Bus ABI

Legacy constants from `src/common/dbus/dbus_interface.h`:

| Item | Value |
| --- | --- |
| Bus name | `org.fcitx.Vinput` |
| Service object path | `/org/fcitx/Vinput` |
| Service interface | `org.fcitx.Vinput.Service` |
| Fcitx bus | `org.fcitx.Fcitx5` |
| Frontend notifier object | `/org/fcitx/Fcitx5/Vinput` |
| Frontend notifier interface | `org.fcitx.Fcitx5.Vinput1` |

Legacy daemon method/signature contract from `src/daemon/runtime/dbus_service.cpp`:

| Method | Input | Output | Notes |
| --- | --- | --- | --- |
| `StartRecording` | `""` | `""` | starts normal recording |
| `StartCommandRecording` | `"s"` | `""` | selected text input |
| `StopRecording` | `"s"` | `"s"` | scene id input, recognition JSON output |
| `GetStatus` | `""` | `"s"` | status string |
| `GetAsrBackendState` | `""` | `"sssssbbas"` | target/effective ids, last error, reload flag, availability flag, remote endpoints |
| `ReloadAsrBackend` | `""` | `""` | may defer while busy |
| `StartAdapter` | `"s"` | `""` | adapter id |
| `StopAdapter` | `"s"` | `""` | adapter id |

Legacy signals:

- `RecognitionResult(s)`
- `RecognitionPartial(s)`
- `StatusChanged(s)`
- `DaemonNotification(<error-info-signature>)`

### Recognition JSON payload

Legacy recognition payload is JSON carried as a D-Bus string:

```json
{
  "commit_text": "...",
  "candidates": [
    {"text": "...", "source": "raw"}
  ]
}
```

Candidate sources include `raw`, `llm`, `asr`, and `cancel`.  Legacy parse behavior fills `commit_text` from the first candidate when missing, and creates a raw candidate from `commit_text` when candidates are absent.

### Audio capture

- Actual capture is PipeWire-only in reviewed source.
- `AudioCapture` creates a PipeWire thread loop and `vinput-capture` stream.
- Format is S16 little-endian, 16 kHz, mono.
- `global.capture_device` becomes `PW_KEY_TARGET_OBJECT` unless it is empty/default.
- `common/audio/pipewire_device.cpp` enumerates PipeWire `Audio/Source` nodes for device selection.
- Audio utilities apply input gain and peak normalization.

### ASR backend

- Runtime contract has `Buffered` and `Chunked` delivery modes, events `PartialText`, `FinalText`, `Error`, `Completed`, and backend descriptors/capabilities.
- Local provider path uses `ModelManager`, model metadata, and `backend` selection:
  - `sherpa-offline` default;
  - `sherpa-streaming` for online recognizers.
- sherpa-onnx support includes:
  - offline recognizer config construction for many model families;
  - streaming recognizer config and partial/final event generation;
  - hotwords when model metadata supports them;
  - language/model/task config from model JSON;
  - recognizer warmup and reload state tracking.
- VAD uses sherpa-onnx Silero VAD with threshold/silence/speech/window settings, 200 ms padding, and fallback to original audio when no speech is found.
- Command ASR:
  - batch mode sends raw signed 16-bit PCM bytes to stdin and trims stdout as final text;
  - streaming mode writes chunks to stdin and consumes line-delimited JSON events from stdout: `session_started`, `partial`, `final`, `final_timestamps`, `error`, and `closed`.

### Text post-processing, command mode, LLM, adapters

- Regular post-processing wraps raw ASR text with scene prompt/context and optionally calls an OpenAI-compatible chat-completions endpoint.
- Command mode applies a spoken command to selected text using the built-in `__command__` scene or a user-configured command scene.
- Prompt templates support:
  - `file:///` prompt files with a 256 KiB cap;
  - `{{asr}}`, `{{selected}}`, and `{{context}}` interpolation;
  - XML wrapper fallback when no interpolation is present.
- LLM request contract:
  - non-streaming chat completions;
  - JSON-object response format;
  - response content parsed as `{"candidates": ["..."]}`;
  - protected `extra_body` keys `messages`, `stream`, `response_format` are ignored.
- Adapter supervision uses command specs, working directory resolution, pid files, SIGTERM/SIGKILL stop flow, and D-Bus `StartAdapter`/`StopAdapter`.
- A remote text service implements a local HTTP/WebSocket text input backend for provider `provider.vinput.remote.streaming`.

### Config/default config/migration behavior

- Config version is `1`; default file is `data/default-config.json`.
- Important fields:
  - `registry.base_urls`
  - `global.default_language`, `global.capture_device`
  - `asr.active_provider`, `normalize_audio`, `input_gain`, `vad.enabled`, providers
  - `llm.providers`, `llm.adapters`
  - `scenes.active_scene`, `scenes.definitions`
- Built-in scenes are `__raw__` and `__command__`.
- Scene constraints:
  - candidate count clamped to `0..=9`;
  - timeout must be positive;
  - provider and model must be configured together;
  - prompt is required when provider/model is set.
- Normalization deduplicates registries/providers/adapters/scenes, drops invalid command providers, inserts built-in scenes, clears invalid active references, and removes empty env keys.

### Registry/model/adapter installation

- Remote model registry is fetched from `<base>/registry/models.json`; providers/adapters from `providers.json` and `adapters.json`; i18n from `i18n/<locale>.json`.
- Registry fetch uses online download with cache fallback.
- Model install downloads archive URLs with fallback, verifies SHA-256, extracts with libarchive, rejects absolute paths/path traversal/links/unsupported entries, flattens single top-level directories, and installs into the model path layout.
- Model manager validates `vinput-model.json` and required files for sherpa families.
- Script resources install ASR provider and LLM adapter scripts into managed directories and materialize config entries.

### CLI/GUI/system integration

- CLI command surface includes config get/set/edit, init, model list/add/remove/use/info, provider list/add/use/edit/remove, LLM provider/adapter commands, hotword, audio device list/use, scene management, daemon/systemd control, and recording control.
- GUI is Qt-based and manages config/resources.
- systemd user unit and D-Bus service file are installed.
- Packaging exists for Arch, Debian/Ubuntu, Fedora, openSUSE, and Flatpak.
- Release/channel workflows build and publish source archives, binary tarballs, DEB/RPM/Arch/Flatpak artifacts, and verify install channels.

## Rust project current feature map

### Workspace/crate boundaries

| Crate | Current role |
| --- | --- |
| `vinput-protocol` | D-Bus names, status strings, ASR/text diagnostic structs, recognition payload JSON. |
| `vinput-config` | Typed default-config schema, validation, normalization-like constraints, built-in scenes. |
| `vinput-audio` | PCM buffer/spec types, pure audio processing, stateful recorder/source seams, mock audio, and PipeWire diagnostics scaffolding. |
| `vinput-asr` | ASR traits, mock backend including early-final event cases, command ASR JSON/process seam, factory diagnostics. |
| `vinput-text` | prompt/template helpers, command text adapter request/response, mock and command text processors. |
| `vinput-registry` | registry index data contract, validation, safe path/asset planning, dry-run install plans. |
| `vinput-daemon` | in-memory configured runtime, stateful audio recorder ownership, D-Bus facade/integration, and daemon diagnostics. |
| `vinput-cli` | diagnostics and smoke commands for protocol/config/registry/asr/text/audio/daemon. |

### Protocol/config/text/registry coverage

- D-Bus constants match legacy names and paths.
- Status strings match legacy strings.
- Recognition JSON payload matches the legacy `commit_text`/`candidates` shape and fallback behavior.
- Config can parse the current legacy default config, pins committed-file vs bundled-default parity, covers legacy version promotion, missing-version rejection, minimal/preserved/partial built-in scene normalization, omitted/blank active-scene defaulting, and has extensive validation tests.
- Text crate has prompt fixtures, legacy `file:///` prompt-file loading with the 256 KiB cap, whitespace-tolerant `{{ asr }}`/`{{ selected }}`/`{{ context }}` rendering, OpenAI-compatible endpoint URL/header/request-body assembly with XML fallback/constraints, candidate parsing/payload mapping, protected `extra_body` merge rules, recent-input context-cache prefix helpers, command-mode payload ordering, command adapter seams, and deterministic command processor tests. It still lacks an OpenAI-compatible HTTP LLM client and runtime context-cache wiring.
- Registry crate deliberately lacks network download and archive handling; it currently owns pure manifest contracts, zero/missing version checks, entry field checks, deterministic planning/order and mirror/install fixtures, checksum policy, and expanded path safety fixtures.

### Runtime/daemon/CLI/CI coverage

- `RuntimeState` owns a stateful `AudioRecorder`, uses mock audio by default, and can build configured ASR/text seams.
- D-Bus integration tests exist behind `dbus-integration`, including stop-time partial/result signal ordering.
- CLI and daemon smoke paths cover protocol/config/registry/asr/text/audio diagnostics.
- CI runs fmt, workspace tests, D-Bus integration tests, clippy, D-Bus-feature clippy, and PipeWire feature compile/test guards.
- No Fcitx addon, live PipeWire recording stream, real sherpa backend, real registry install, systemd/dbus install artifacts, or distro packaging exist in Rust yet.

## Compatibility constraints

### D-Bus ABI

The Rust daemon must preserve legacy bus/object/interface names, method names, method signatures, signal signatures, and error behavior.  Current Rust D-Bus integration tests pin the legacy method and signal signatures, including the `GetAsrBackendState` `sssssbbas` tuple.  `GetTextAdapterState` is an explicit Rust diagnostic extension rather than a legacy C++ vtable method.

The next contract step is behavior parity: caller-level fixtures for frontend-visible edge cases, legacy error mapping/notifications, status ordering, and real-backend lifecycle transitions as PipeWire, sherpa, and text post-processing land.

### JSON payload

`StopRecording` and recognition signals must keep the legacy recognition payload JSON exactly enough for older frontends and CLI parsers:

- key names: `commit_text`, `candidates`, `text`, `source`;
- candidate sources: `raw`, `llm`, `asr`, `cancel`;
- parse fallback semantics for missing `commit_text` or candidates;
- no accidental schema rename from Rust serde defaults.

### Config/default config

Rust treats `data/default-config.json` as a compatibility fixture and pins committed-file vs bundled-default parity. Existing user config remains a compatibility target: the rewrite may add new optional fields, but must preserve existing fields, defaults, built-in scene IDs/labels, provider IDs, registry base URL semantics, and validation/normalization behavior.

### Runtime status semantics

The externally visible status strings and transitions must stay compatible:

- `idle` before/after successful flow;
- `recording` after start;
- `inferring` while ASR is finishing;
- `postprocessing` when LLM/text processing is active;
- `error` when fatal start/streaming/runtime errors occur;
- busy start rejection while non-idle;
- legacy ASR reload deferral while busy, or a deliberately documented compatibility decision if Rust keeps the current idle-only guard.

### CLI/smoke behavior

Existing Rust smoke commands should stay stable as early CI guards.  Legacy CLI parity should be added as small compatible slices instead of replacing current diagnostic commands wholesale.

## Difference matrix

| Feature domain | Legacy capability | Rust current capability | Rust gap | Must compatible? | Suggested implementation | Risks/dependencies | Priority |
| --- | --- | --- | --- | --- | --- | --- | --- |
| D-Bus service ABI | Exact legacy `sd-bus` vtable; void and tuple method signatures; legacy signals. | Same names/paths, zbus facade, and D-Bus integration introspection tests for legacy method/signal signatures; `GetTextAdapterState` is an explicit diagnostic extension. | Remaining behavior compatibility gaps: frontend integration is absent, and some legacy-visible method semantics still need parity as real backends land. | Yes | Keep introspection/signature tests pinned; add behavior-level ABI fixtures and keep diagnostic JSON out of legacy method shapes. | zbus multiple-return mapping, signal ordering, and frontend expectations. | P0 |
| Recognition payload | Stable JSON payload and candidate source semantics. | Matching Rust model, fallback tests, shared raw/menu/sentinel fixture files consumed by protocol/daemon/CLI tests, cancel sentinel parser roundtrip, blank-cancel parsing, invalid-candidate fallback, and D-Bus raw-result fixture coverage. | Need behavior-level D-Bus coverage for menu/sentinel payloads and any newly discovered legacy parser edge cases. | Yes | Keep fixture files shared across protocol, daemon, and CLI; add behavior-level menu/sentinel D-Bus fixtures when those frontend-visible flows exist; keep serde field names pinned. | Low. | P0 |
| Status/runtime state | Busy rejection, recording/inferring/postprocessing/error transitions, deferred reload, notification errors. | Runtime owns stateful recorder/session seams, rejects start/reload while recording, emits stop-time partial/result ordering, and preserves active sessions across busy reload attempts. | Remaining gaps: legacy error notifications, explicit postprocessing phase, deferred reload semantics if kept, real worker/audio streaming lifecycle. | Yes | Keep current start/stop/busy/reload guards pinned; add transition fixtures for error notifications, postprocessing, and deferred reload compatibility decisions. | Async locking and D-Bus signal ordering. | P0 |
| Fcitx frontend addon | Shared-library addon, key state machine, preedit/candidate/menu/notification behavior. | Not implemented. | No input method integration. | Yes for usability | Keep a thin legacy-compatible frontend bridge; decide whether C++ shim or Rust FFI only after daemon ABI stabilizes. | Fcitx5 addon API is C++-oriented; Rust binding maturity unclear. | P1/P2 |
| Audio capture | PipeWire S16LE/16k/mono capture, target object selection, source enumeration, chunk callback. | Pure PCM processing, `AudioRecorder` lifecycle seam, mock source/recorder, capture-target config parsing, and feature-gated PipeWire source enumeration used by CLI/daemon diagnostics. | No live PipeWire recording stream; enumeration diagnostics exist but capture still returns explicit unavailable errors. | Behavior yes, implementation no | Complete `PipeWireAudioRecorder` live stream behind `AudioRecorder`; keep `AudioDeviceEnumerator` diagnostics and mock tests; mirror target-object/default semantics. | PipeWire runtime and distro deps; Rust crate docs.rs latest build issue should be validated locally. | P1 |
| Audio processing | Gain, peak normalization, VAD optional trimming. | Gain/normalization-like mock processing and silence threshold. | Exact normalization/VAD path incomplete. | Mostly yes | Add pure audio golden tests before real capture; port VAD behind ASR/audio boundary. | Sample-rate assumptions and clipping behavior. | P1 |
| Local ASR | sherpa-onnx offline/streaming, many model families, hotwords, model metadata, warmup. | Traits/mock; command seam; local providers mostly mock/unsupported. | No real sherpa backend or model manager. | Yes | Add model metadata parser/validator, then sherpa offline, then streaming. | FFI safety, bundled runtime libraries, model family coverage. | P1/P2 |
| Command ASR | Batch raw PCM stdin; streaming line JSON stdout; timeouts/errors. | Legacy batch raw-PCM runner, `.streaming` JSON-line runner with streaming capabilities, duplicate-partial suppression, JSON helper seam, process timeout/error tests, runtime PCM metadata forwarding, and D-Bus stop-time partial coverage. | Remaining parity gaps are incremental long-lived streaming sessions, richer cancellation/process-shutdown fixtures, and broader third-party helper coverage. | Yes for existing adapters | Keep legacy batch/streaming fixtures pinned; extend streaming session lifecycle and cancellation/process-shutdown cases before treating third-party helpers as drop-in compatible. | Process cancellation/shutdown, stderr handling, and helper protocol drift. | P1 |
| ASR reload state | target/effective backend, previous backend retained on failure, reload progress, remote endpoints. | Diagnostic `AsrBackendState` derived from config/backend; reload is rejected while recording so active sessions are preserved. | No real async reload manager, warmup, reload progress, or previous-effective fallback beyond current failed-build no-swap behavior. | Yes | Keep idle-only/busy guard tests pinned; add state classification tests and manager abstraction before real backend. | Concurrent reload while recording, backend warmup latency, and preserving prior effective sessions. | P1 |
| Text/LLM post-processing | OpenAI-compatible HTTP, prompt files, interpolation, context cache, candidate JSON parsing, command mode. | Prompt/request/command-adapter seams; mock/command processors; `file:///` prompt-file loader with 256 KiB cap; whitespace-tolerant legacy double-brace interpolation; OpenAI-compatible endpoint URL/header helpers, candidate response parser/payload mapper, protected `extra_body` merge helper, pure request-body builder with XML fallback/constraints, and recent-input context-cache prefix helpers, and command-mode payload ordering. | No HTTP LLM provider/client or complete provider call/runtime context-cache wiring. | Yes for user config | Wire the tested URL/header/request builder/context helper to an HTTP client and runtime cache path with redaction/cancellation. | Secret handling; provider-specific `extra_body`; cancellation; cache I/O diagnostics. | P1/P2 |
| Adapter supervision | Start/stop scripts, pid files, working dir, running status. | Configured command adapters can be started/stopped through runtime and D-Bus; pid files, working directories, reaping, and diagnostics are covered. | Legacy parity still needs richer adapter lifecycle behavior, packaging/install integration, and frontend UX around adapter state. | Yes | Keep current supervisor tests pinned; extend lifecycle/error fixtures before adding install-time adapter materialization and frontend controls. | Process leaks, pid races, and cross-platform semantics. | P2 |
| Remote text service | Local HTTP/WebSocket remote input provider with auth/debounce/endpoints. | Not implemented. | Entire remote provider missing. | Yes if provider kept | Rebuild using safe HTTP/WebSocket crates after provider contract tests. | Auth, LAN exposure, WebSocket parser safety. | P2 |
| Config normalization | Legacy drops/dedupes invalid entries and inserts built-ins. | Typed validation and many guards; exact normalization parity uncertain. | Need golden tests against legacy normalization edge cases. | Yes | Add fixture-based parser/normalizer tests; separate strict validate from repair/normalize. | Backward compatibility with user configs. | P0/P1 |
| Registry/model install | Online fetch/cache fallback, i18n, SHA-256, safe archive extraction, model path layout, script materialization. | Pure index validation, path safety, dry-run install plans. | No network/cache/download/extract/install/i18n/providers registry parity. | Yes for CLI/resource flows | Add contract fixtures; implement fetch/cache; then checksum and archive extraction; then install/materialize. | Supply-chain safety and archive traversal. | P1/P2 |
| CLI management | Rich model/provider/adapter/scene/device/config/daemon commands. | Diagnostics and smoke commands. | Most legacy user commands missing. | Mostly yes | Add CLI subcommands around tested library APIs; preserve current diagnostics. | UX compatibility and translations. | P2 |
| GUI | Qt resource/config GUI. | Not implemented. | Entire GUI missing. | No for daemon MVP; yes for full replacement | Defer until CLI/backends stable. | Qt/Rust GUI decision. | P3 |
| Packaging/install | CMake install, systemd user unit, D-Bus service, Fcitx addon conf, distro packages, Flatpak, release workflows. | Cargo workspace and CI only. | No install integration or packages. | Yes for release | Add generated install artifacts after daemon/frontend are real; then package. | Distro policy and bundled sherpa libraries. | P2/P3 |
| Tests/CI | Legacy has build/release/channel workflows; runtime coverage is mixed. | Strong Rust unit/smoke/CI guards for current scope. | Missing compatibility and real-backend integration tests. | Yes | Keep expanding contract tests before features; add optional integration jobs for PipeWire/sherpa when feasible. | CI environment limitations. | P0+ |

## Must-fill functionality before Rust can replace legacy

1. D-Bus behavior parity fixtures beyond current signature/introspection and integration coverage, especially frontend-visible edge cases.
2. Recognition JSON golden fixtures shared by CLI/daemon/protocol tests; raw/menu/sentinel files are shared across all three crates and cancel sentinel, blank-cancel parsing, and invalid-candidate fallback now roundtrip, while behavior-level D-Bus coverage still only pins raw output.
3. Runtime state-machine parity beyond current start/stop/busy/reload guards: legacy error notifications, explicit postprocessing phase, and deferred reload semantics.
4. Config/default-config compatibility fixtures beyond committed-vs-bundled parity, version promotion, missing-version rejection, built-in scene normalization, and omitted/blank active-scene defaulting, plus normalize-vs-validate policy.
5. Command ASR long-lived streaming lifecycle plus broader cancellation/process-shutdown parity; batch raw PCM, one-shot `.streaming` JSON-line runners, stderr/timeout fixtures, duplicate-partial suppression, streaming capabilities, and D-Bus stop-time partial fixtures already exist.
6. Live PipeWire recording behind `AudioRecorder`; source enumeration diagnostics already exist behind `pipewire-backend` and should stay covered.
7. Model metadata/path manager and local model validation.
8. sherpa-onnx offline backend, then streaming backend, then VAD/hotwords.
9. OpenAI-compatible post-processing and command mode parity; endpoint URL/header building, prompt-file loading, whitespace-tolerant legacy interpolation, candidate parsing/payload mapping, command-mode payload ordering, protected `extra_body` merging, pure request-body assembly with XML fallback/constraints, and recent-input context-prefix helpers are now pinned, while HTTP transport, runtime cache wiring, and provider call behavior remain.
10. Adapter lifecycle parity beyond the current runtime/D-Bus start-stop path: install materialization, richer edge fixtures, and frontend UX.
11. Registry fetch/cache/archive/install/materialization; pure manifest zero/missing version checks, entry field checks, planning/order and mirror/install fixtures, checksum policy, and expanded path safety fixtures already exist.
12. Minimal Fcitx frontend integration.
13. systemd user unit, D-Bus service file, Fcitx addon installation, and packaging.

## Parts that can be redesigned instead of copied

- Internal daemon threading: use Rust async/task boundaries or explicit worker abstractions; do not copy eventfd/sd-bus loops line-for-line.
- D-Bus internals: keep zbus if it can express the legacy ABI cleanly.
- Audio backend internals: keep `AudioSource` trait; add PipeWire implementation without exposing PipeWire types across crates.
- ASR internals: keep trait-based backend/session contracts; isolate unsafe sherpa FFI in a narrow module/crate.
- HTTP/remote service: use maintained Rust HTTP/WebSocket libraries instead of porting raw socket parsing.
- Registry install: preserve path/checksum/archive behavior but implement with typed errors, temporary dirs, and path traversal tests.
- Frontend strategy: a thin C++ Fcitx addon can remain acceptable while Rust owns daemon/core logic; a pure Rust addon should wait until bindings are proven.
- CLI organization: keep diagnostic commands and add legacy-compatible management commands as small tested modules.

## Recommended implementation order

1. **Lock contracts first**
   - D-Bus introspection/signature tests.
   - Add behavior-level D-Bus menu/sentinel recognition payload fixtures once those frontend-visible flows exist.
   - Config default/normalization fixtures.
   - Command ASR and text post-processing fixtures.
2. **Fill pure logic**
   - Config normalization parity.
   - Model metadata manager.
   - Registry checksum/path/archive planning tests.
   - Prompt/context/candidate parsing logic.
3. **Harden process/local backends**
   - Broader Command ASR batch fixtures.
   - Long-lived Command ASR streaming lifecycle fixtures.
   - Richer text adapter process supervision edge cases.
4. **Add real system backends**
   - PipeWire capture.
   - sherpa offline.
   - sherpa streaming.
   - VAD/hotwords/model reload manager.
5. **Integrate frontend and install**
   - Fcitx addon bridge.
   - systemd/dbus service files.
   - packaging smoke.
6. **Broaden release/GUI**
   - distro packages, Flatpak, release workflows.
   - GUI or replacement management UI.

## Uncertainties and external dependencies to validate

- Whether `zbus` can expose every legacy method exactly, especially `sssssbbas`, without awkward wrappers.
- Whether the current `pipewire` Rust crate version works reliably in the project CI/distro matrix; docs.rs currently lists the latest crate but indicates its latest docs build failed.
- Whether to use upstream `sherpa-onnx`/`sherpa-onnx-sys` crates, bindgen local headers, or a custom FFI crate for the exact bundled runtime version.
- Whether `fcitx5-dbus` is useful for any frontend-facing D-Bus calls; it is not a full addon binding.
- Whether a pure Rust Fcitx5 addon is practical; official Fcitx5 addon docs are C++/CMake-centric.
- How much of the Qt GUI should be retained, replaced, or deferred.
- Packaging policy around bundled onnxruntime/sherpa libraries for each distro in the Rust rewrite.

## Legacy problems Rust should improve, not replicate

- **Secret logging:** legacy debug logging can include Authorization headers; Rust should redact API keys by default even under debug.
- **Fixed debug output paths:** legacy streaming can dump empty results to a fixed `/tmp` WAV path; Rust should make debug artifacts opt-in and collision-safe.
- **Raw socket/WebSocket parsing:** rebuild remote text service with a maintained parser and explicit auth/frame-size tests.
- **Ad-hoc ABI drift:** D-Bus ABI should be guarded by introspection and caller-level tests.
- **Archive safety as an afterthought:** keep and expand path traversal/link/absolute-path tests before enabling extraction.
- **Global lifecycle side effects:** isolate global init/shutdown and FFI lifetimes behind narrow RAII wrappers.
- **Unstructured errors:** use typed errors internally and map them deliberately to legacy D-Bus failures/notifications.
- **Large coupled runtime class:** keep Rust runtime split across audio/asr/text/registry/daemon crates rather than accumulating all logic in one crate.
