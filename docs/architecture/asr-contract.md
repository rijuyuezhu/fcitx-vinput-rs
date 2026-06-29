# ASR contract

`vinput-asr` owns the ASR backend/session contract. It keeps recognition behavior behind Rust trait boundaries while preserving the legacy daemon/frontend payload shape.

## Current crate responsibilities

`crates/vinput-asr` currently contains these responsibilities, though the implementation still needs to be split into focused modules during the refactor phase:

- `AudioDeliveryMode`: buffered vs chunked delivery.
- `BackendCapabilities`: partial-result support and delivery mode.
- `BackendDescriptor`: provider/model/label/capability identity.
- `RecognitionContext`: language, scene id, command-mode flag, and selected text.
- `RecognitionEvent`: partial text, final text, backend error, and completed markers.
- `RecognitionSession`: mutable session trait for `push_pcm`/raw `push_audio`/finish/cancel/poll.
- `AsrBackend`: backend factory trait.
- `MockAsrBackend`: deterministic backend used by default runtime and tests, including buffered, streaming, and early-final event variants.
- `CommandAsrSpec`: parsed command-provider executable metadata from config.
- `CommandAsrRequest`: buffered JSON request passed to command helpers.
- `CommandAsrResponse`: JSON response decoded from command helpers.
- `CommandAsrBackend`: command backend that delegates to a runner on finish and exposes buffered or streaming capabilities from the factory.
- `ProcessCommandAsrRunner`: process-backed runner using stdin/stdout JSON.
- `events_to_payload`: conversion from final ASR events to the legacy recognition payload JSON model.

Command providers now use legacy batch or `.streaming` runners through the factory, while the JSON helper seam remains available for explicit process-runner tests and small helper integrations. Local sherpa-onnx remains future work.

## Daemon integration

`RuntimeState` owns a boxed `AsrBackend` and an active `RecognitionSession` while recording. The default daemon uses `MockAsrBackend`; explicit configured paths can build the active config-selected backend to exercise command ASR seams.

The current runtime flow is:

```text
StartRecording
  -> create_session
  -> begin audio recorder
StopRecording
  -> stop recorder and collect PCM
  -> apply deterministic audio processing
  -> push PCM to the active ASR session
  -> drain already-pending ASR events
  -> finish session
  -> poll and merge final/stop-time events
  -> emit stop-time partial through D-Bus when present
  -> events_to_payload
  -> text finishing
  -> reset Idle
```

This is a contract seam, not full legacy runtime parity. Live PipeWire streaming, local sherpa-onnx, VAD trimming, warmup/reload state, and real worker orchestration still belong to future feature phases after the refactor plan permits them.

## Command ASR provider contracts

A command ASR provider is configured with `type = "command"`, a `command`, optional `args`, `env`, `model`, `hotwords_file`, and `timeout_ms`. The config-selected factory preserves the legacy command behavior currently covered by tests:

1. provider ids that end with `.streaming` use `LegacyCommandStreamingRunner`, expose streaming/chunked capabilities, write one committed audio JSON line plus a finish line to stdin, parse JSON event lines from stdout, and suppress repeated partial text like the legacy C++ session;
2. other command providers use `LegacyCommandBatchRunner`, which writes raw signed 16-bit little-endian PCM to stdin and reads final text from stdout;
3. both runners honor configured args/env and process timeout/error handling.

`CommandAsrRequest` remains the internal buffered request type shared by these runners and explicit test seams. It carries provider metadata, recognition context, PCM layout, and interleaved signed 16-bit samples.

A JSON helper can return final text and optionally partial text:

```json
{"partial_text":"listening","text":"final text"}
```

A helper can also return an ASR-level error without a non-zero process exit:

```json
{"error":"asr failed"}
```

The deprecated `failure` response key is accepted as an alias for `error`. Non-zero exits, invalid JSON, missing final text, and timeout paths are surfaced as backend errors.

## Diagnostics

Both `vinput-cli asr-state` and `vinput-daemon asr-state` serialize `AsrBackendState` from config only. They do not construct, reload, or probe the runtime backend. The daemon diagnostic remains usable with `--configured-backends` even when the selected runtime backend is unavailable.

## Known compatibility gaps

These gaps are tracked by the ignored refactor plan in `docs/plan/review-driven-refactor-plan.md`:

- `ReloadAsrBackend` should match the legacy daemon by deferring reload requests while busy instead of rejecting them outright.
- Local sherpa-onnx backend, model path management, hotwords, VAD trimming, warmup, and reload state are not implemented yet.
- Runtime streaming currently has test seams, but live PipeWire chunk delivery to streaming ASR is not implemented.

## Next ASR refactor steps

Before adding sherpa-onnx, split the monolithic ASR crate into focused modules:

- `error.rs`
- `traits.rs`
- `mock.rs`
- `factory.rs`
- `payload.rs`
- `command/mod.rs`
- `command/batch.rs`
- `command/streaming.rs`
- `command/process.rs`

The split should be behavior-preserving and keep existing command ASR tests intact.
