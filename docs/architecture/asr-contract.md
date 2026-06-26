# ASR contract milestone

This milestone introduces `vinput-asr`, the first backend seam after the D-Bus boundary.

## Current crate

`crates/vinput-asr` contains:

- `AudioDeliveryMode`: buffered vs chunked delivery.
- `BackendCapabilities`: partial-result support and delivery mode.
- `BackendDescriptor`: provider/model/label/capability identity.
- `RecognitionEvent`: partial text, final text, backend error, and completed markers.
- `RecognitionSession`: mutable session trait for push/finish/cancel/poll.
- `AsrBackend`: backend factory trait.
- `MockAsrBackend`: deterministic backend used by daemon runtime and tests.
- `events_to_payload`: conversion from final ASR events to the legacy recognition payload JSON model.

This mirrors the original C++ recognition contract without bringing over concrete sherpa-onnx or command-streaming implementation details yet.

## Daemon integration

`RuntimeState` now owns a boxed `AsrBackend` and an active `RecognitionSession` while recording. The default daemon still uses `MockAsrBackend`, but tests can inject a custom mock backend to prove the runtime is driven by the ASR trait boundary rather than hardcoded strings.

The current mock flow is:

```text
StartRecording
  -> create_session
  -> push mock PCM
  -> poll partial events
StopRecording
  -> push mock PCM
  -> finish session
  -> poll final events
  -> events_to_payload
  -> reset Idle
```

Command mode still performs a placeholder transform after ASR. That is intentional; command-scene prompt and post-processing should move to `vinput-postprocess` later.

## Next ASR steps

1. Add a richer `RecognitionRequest`/`RecognitionContext` type for language, scene, command mode, selected text, and model options.
2. Add `vinput-audio` for real PCM buffers, sample rate metadata, gain, silence detection, and normalization.
3. Add a command backend behind `AsrBackend` with fixture tests.
4. Add a feature-gated sherpa-onnx backend only after the trait and mock tests are stable.
5. Wire streaming partial events to `RecognitionPartial` D-Bus signals instead of only storing the latest partial text.
