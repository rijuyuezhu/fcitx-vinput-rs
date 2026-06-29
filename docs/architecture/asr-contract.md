# ASR contract milestone

This milestone introduces `vinput-asr`, the first backend seam after the D-Bus boundary.

## Current crate

`crates/vinput-asr` contains:

- `AudioDeliveryMode`: buffered vs chunked delivery.
- `BackendCapabilities`: partial-result support and delivery mode.
- `BackendDescriptor`: provider/model/label/capability identity.
- `RecognitionContext`: language, scene id, command-mode flag, and selected text.
- `RecognitionEvent`: partial text, final text, backend error, and completed markers.
- `RecognitionSession`: mutable session trait for `push_pcm`/raw `push_audio`/finish/cancel/poll.
- `AsrBackend`: backend factory trait.
- `MockAsrBackend`: deterministic backend used by daemon runtime and tests, including buffered, streaming, and early-final event variants.
- `CommandAsrSpec`: parsed command-provider executable metadata from config.
- `CommandAsrRequest`: buffered JSON request passed to command helpers.
- `CommandAsrResponse`: JSON response decoded from command helpers.
- `CommandAsrBackend`: buffered command backend that delegates to a runner on finish.
- `ProcessCommandAsrRunner`: process-backed runner using stdin/stdout JSON.
- `events_to_payload`: conversion from final ASR events to the legacy recognition payload JSON model.

This mirrors the original C++ recognition contract while keeping concrete backends behind Rust trait boundaries. Command providers now use legacy batch or `.streaming` runners through the factory, while the JSON helper seam remains available for explicit process-runner tests and small helper integrations. sherpa-onnx remains a later feature-gated backend.

## Daemon integration

`RuntimeState` now owns a boxed `AsrBackend` and an active `RecognitionSession` while recording. The default daemon still uses `MockAsrBackend`, but tests can inject a custom mock backend or build the active config-selected backend to prove the runtime is driven by the ASR trait boundary rather than hardcoded strings.

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

Command mode still carries selected text in `RecognitionContext`; command-scene post-processing can move to `vinput-text` later without changing the ASR trait boundary.

## Command ASR provider contracts

A command ASR provider is configured with `type = "command"`, a `command`, optional `args`, `env`, `model`, `hotwords_file`, and `timeout_ms`. The config-selected factory currently preserves the legacy command behavior:

1. provider ids that end with `.streaming` use `LegacyCommandStreamingRunner`, which writes one committed audio JSON line plus a finish line to stdin and parses JSON event lines from stdout,
2. other command providers use `LegacyCommandBatchRunner`, which writes raw signed 16-bit little-endian PCM to stdin and reads final text from stdout,
3. both runners honor configured args/env and process timeout/error handling.

`CommandAsrRequest` remains the internal buffered request type shared by these runners and by explicit test seams. It carries provider metadata, recognition context, PCM layout, and interleaved signed 16-bit samples:

```json
{
  "provider_id": "cmd",
  "model_id": "paraformer",
  "hotwords_file": "/tmp/hotwords.txt",
  "timeout_ms": 2500,
  "context": {
    "language": "zh",
    "scene_id": "__command__",
    "command_mode": true,
    "selected_text": "selected text"
  },
  "pcm": {
    "sample_rate_hz": 16000,
    "channels": 1
  },
  "samples": [10, -20, 30]
}
```

`samples` are signed 16-bit PCM values. When `channels` is greater than one they are interleaved in frame order. A single command session has one PCM spec, so the session rejects attempts to append buffers with different sample rates or channel counts.

For the explicit JSON helper seam used by `ProcessCommandAsrRunner`, a helper can return final text, and optionally a partial text:

```json
{"partial_text":"listening","text":"final text"}
```

A helper can also return an ASR-level error without a non-zero process exit:

```json
{"error":"asr failed"}
```

The deprecated `failure` response key is accepted as an alias for `error` while the JSON helper seam is still settling. Non-zero exits, invalid JSON, missing final text, and timeout paths are surfaced as backend errors.

## Diagnostics

Both `vinput-cli asr-state` and `vinput-daemon asr-state` serialize `AsrBackendState` from config only. They do not construct, reload, or probe the runtime backend. The daemon diagnostic remains usable with `--configured-backends` even when the selected runtime backend is unavailable.

Runtime ASR reload is only valid while the service is idle. `ReloadAsrBackend` and the configured-backend reload seam reject recording or inferring states with `Busy`, preserving the active ASR session and recorder so `StopRecording` can still complete normally.

## Next ASR steps

1. Move command-scene prompt and post-processing policy to `vinput-text` while preserving `RecognitionContext` as the frontend/runtime seam.
2. Add a feature-gated sherpa-onnx backend only after the command and mock contracts stay stable.
