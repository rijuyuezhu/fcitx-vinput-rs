# Audio contract

`vinput-audio` owns pure PCM data structures and deterministic byte-level audio helpers. Desktop capture backends such as PipeWire should feed this crate instead of duplicating PCM parsing or format policy.

## PCM layout

The canonical in-memory representation is signed 16-bit interleaved PCM carried by `PcmBuffer` with explicit `PcmSpec` metadata:

- `sample_rate_hz`: non-zero sample rate.
- `channels`: non-zero interleaved channel count, defaulting to mono when omitted from JSON.
- `samples`: raw `i16` samples whose length must align to the channel count.

Frame-oriented calculations, duration, and silence trimming use frames rather than raw sample count. Multi-channel buffers are preserved as complete interleaved frames.

## Byte formats

Raw PCM bytes are signed 16-bit little-endian. Use `PcmBuffer::from_pcm16le_bytes` to decode raw bytes with explicit `PcmSpec`, and `i16_samples_to_le_bytes` when command ASR helpers need raw PCM bytes. Odd byte counts are rejected before sample conversion.

WAV decoding supports uncompressed RIFF/WAVE PCM format tag 1 with 16-bit samples. The parser preserves sample rate and channel metadata, skips unknown chunks using RIFF padding rules, rejects odd data chunk byte counts, and validates `block_align` plus `byte_rate` against the parsed sample format.

## Capture device discovery

Desktop capture backends should expose `AudioDeviceEnumerator` for UI/CLI device lists. `AudioDeviceInfo` mirrors the legacy PipeWire discovery shape: backend-local `id`, backend object `name`, and human-readable `description`. Enumerators should return only capture sources, preserving backend discovery order. `AudioDeviceInfo::capture_target` maps a discovered source name to the concrete `CaptureTarget::Object` used by recording.

The optional `pipewire-backend` feature verifies that the Rust PipeWire bindings and system headers compile, link, and initialize, maps `PipeWire:Interface:Node` globals with `media.class=Audio/Source` into `AudioDeviceInfo`, and provides a `PipeWireDeviceEnumerator` implementation. `vinput-cli audio-devices` and `vinput-daemon audio-devices` use this enumerator when they are built with the feature; enumeration failures are reported in JSON as `enumeration_error` with `live: false` instead of making diagnostics fail. Live context and registry probes require a usable PipeWire client configuration, so they are guarded by `VINPUT_TEST_PIPEWIRE_CONTEXT` or `VINPUT_TEST_PIPEWIRE_ENUMERATE` instead of running in default CI. Default CI must compile and test the feature without requiring a live PipeWire daemon; live probes must only run when those environment variables are set explicitly.

## Capture lifecycle

Desktop recorders should implement the stateful `AudioRecorder` contract instead of overloading `AudioSource`. The contract mirrors the legacy daemon lifecycle:

1. Parse `global.capture_device` with `CaptureTarget::from_config_value`; `default` maps to the backend default, any other non-empty value is passed as a concrete backend target object.
2. `begin_recording` starts a fresh capture session and rejects duplicate starts.
3. Optional chunk callbacks may receive interleaved `PcmBuffer` chunks for streaming ASR sessions.
4. `stop_and_get_buffer` stops capture and returns the accumulated PCM buffer.
5. `cancel_recording` stops capture and discards pending audio.

`RuntimeState` consumes `AudioRecorder` directly: `StartRecording` begins capture and enters `recording`; `StopRecording` collects the final buffer, applies processing, pushes PCM to the active ASR session, drains already pending ASR events, finishes the session, then merges any final events before payload conversion. This preserves early partial/final events emitted before `finish`. The existing `AudioSource` trait remains a one-shot source for deterministic tests and file-input demos. `SourceAudioRecorder` adapts those one-shot sources into the stateful runtime path, while `RecorderAudioSource` adapts stateful recorders back into legacy one-shot call sites.

ASR session ownership is explicit across the stop path. If recorder stop, PCM delivery, ASR polling, payload conversion, or text finishing fails, `RuntimeState` calls `RecognitionSession::cancel` before returning the error and resetting to idle. Dropping a runtime with an active recording also cancels the active ASR session before cancelling the recorder.

`PipeWireStreamConfig` records the selected capture target plus the pinned `S16LE` 16 kHz mono PCM policy that future live streams must request. `PipeWireAudioRecorder` currently exists behind `pipewire-backend` as an explicit skeleton: it stores the selected `CaptureTarget`, links and initializes the PipeWire client library, and returns `RecordingBackendUnavailable` instead of silently falling back to mock capture. The future live implementation should negotiate signed 16-bit 16 kHz mono PCM first, stream chunks through the callback, and materialize `CapturedAudio` with source metadata on stop.

## Processing order

`AudioProcessingOptions::process` applies deterministic transforms in this order:

1. Trim leading and trailing silent frames using the absolute silence threshold.
2. Optionally normalize to a target peak.
3. Apply input gain with saturating `i16` conversion.

This order is part of the backend contract because command ASR helpers, file-input E2E demos, and future PipeWire capture should observe the same PCM delivered to ASR sessions.
