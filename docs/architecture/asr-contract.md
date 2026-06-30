# ASR contract

`vinput-asr` owns the ASR backend/session contract. It keeps recognition behavior behind Rust trait boundaries while preserving the legacy daemon/frontend payload shape.

## Current crate responsibilities

`crates/vinput-asr` is split by responsibility while keeping the public trait boundary stable:

- `traits.rs`: `AudioDeliveryMode`, `BackendCapabilities`, `BackendDescriptor`, `RecognitionContext`, `RecognitionEvent`, `RecognitionSession`, and `AsrBackend`;
- `error.rs`: `AsrError`;
- `mock.rs`: deterministic buffered/streaming/early-final `MockAsrBackend`;
- `command.rs`: command provider specs, JSON request/response types, legacy batch and streaming runners, process runner helpers, and `CommandAsrBackend`;
- `factory.rs`: config-selected backend factory and config-derived `AsrBackendState`;
- `sherpa.rs`: local `sherpa-onnx` typed config parsing plus pre-runtime local model/hotwords path validation;
- `payload.rs`: conversion from recognition events to the legacy recognition payload JSON model;
- `tests.rs`: behavior-preserving coverage for mock, command, factory, and payload contracts.

Command providers use legacy batch or `.streaming` runners through the factory, while the JSON helper seam remains available for explicit process-runner tests and small helper integrations. Local `sherpa-onnx` has an explicit typed config seam and a pre-runtime local model/hotwords path validation seam, but the runtime remains unavailable until the concrete backend is implemented.

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

These gaps remain after the behavior-preserving ASR split:

- Local `sherpa-onnx` typed config parsing and local model/hotwords path validation exist as seams; hotwords runtime use, VAD trimming, warmup, and concrete reload state are not implemented yet.
- Runtime streaming has command-helper test seams, but live PipeWire chunk delivery to streaming ASR is not implemented.
- Command ASR is runtime-wired for configured command providers; local and remote ASR provider kinds other than command/mock remain contract-pinned but unavailable.
