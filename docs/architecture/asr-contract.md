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
- `MockAsrBackend`: deterministic backend used by daemon runtime and tests.
- `CommandAsrSpec`: parsed command-provider executable metadata from config.
- `CommandAsrRequest`: buffered JSON request passed to command helpers.
- `CommandAsrResponse`: JSON response decoded from command helpers.
- `CommandAsrBackend`: buffered command backend that delegates to a runner on finish.
- `ProcessCommandAsrRunner`: process-backed runner using stdin/stdout JSON.
- `events_to_payload`: conversion from final ASR events to the legacy recognition payload JSON model.

This mirrors the original C++ recognition contract while keeping concrete backends behind Rust trait boundaries. The command backend is now executable through a small JSON helper contract; sherpa-onnx remains a later feature-gated backend.

## Daemon integration

`RuntimeState` now owns a boxed `AsrBackend` and an active `RecognitionSession` while recording. The default daemon still uses `MockAsrBackend`, but tests can inject a custom mock backend or build the active config-selected backend to prove the runtime is driven by the ASR trait boundary rather than hardcoded strings.

The current runtime flow is:

```text
StartRecording
  -> create_session
  -> push mock/processed PCM
  -> poll partial events
StopRecording
  -> push mock/processed PCM
  -> finish session
  -> poll final/stop-time partial events
  -> emit stop-time partial through D-Bus when present
  -> events_to_payload
  -> text finishing
  -> reset Idle
```

Command mode still carries selected text in `RecognitionContext`; command-scene post-processing can move to `vinput-postprocess` later without changing the ASR trait boundary.

## Command helper JSON contract

A command ASR provider is configured with `type = "command"`, a `command`, optional `args`, `env`, `model`, `hotwords_file`, and `timeout_ms`. At runtime the process runner:

1. spawns `command` with `args` and `env`,
2. writes one `CommandAsrRequest` JSON object to stdin,
3. closes stdin,
4. waits for stdout within `timeout_ms` when configured,
5. decodes one `CommandAsrResponse` JSON object from stdout.

The request shape is intentionally plain JSON so shell/Python/Rust helpers can implement it without a binary protocol:

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

`samples` are signed 16-bit PCM values. When `channels` is greater than one they are interleaved in frame order; the current runtime still produces mono mock audio by default. A single command request has one PCM spec, so the command session rejects attempts to append buffers with different sample rates or channel counts.

A successful helper can return final text, and optionally a partial text:

```json
{"partial_text":"listening","text":"final text"}
```

A helper can also return an ASR-level error without a non-zero process exit:

```json
{"error":"asr failed"}
```

The deprecated `failure` response key is accepted as an alias for `error` while the contract is still settling. Non-zero exits, invalid JSON, missing final text, and timeout paths are surfaced as backend errors.

## Diagnostics

Both `vinput-cli asr-state` and `vinput-daemon asr-state` serialize `AsrBackendState` from config only. They do not construct, reload, or probe the runtime backend. The daemon diagnostic remains usable with `--configured-backends` even when the selected runtime backend is unavailable.

## Next ASR steps

1. Move command-scene prompt and post-processing policy to `vinput-postprocess` while preserving `RecognitionContext` as the frontend/runtime seam.
2. Add a feature-gated sherpa-onnx backend only after the command and mock contracts stay stable.
