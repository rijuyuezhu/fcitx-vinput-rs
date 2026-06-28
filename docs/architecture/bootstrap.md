# Bootstrap architecture

This repo starts with a deliberately small Rust core before porting the full C++ implementation.

## Current crates

- `vinput-protocol`: stable D-Bus names, status strings, ASR/text diagnostics, and recognition result JSON.
- `vinput-config`: typed model for the legacy `data/default-config.json`, including validation.
- `vinput-audio`: pure PCM buffers, stateful capture seams, PipeWire feature scaffolding, and deterministic transforms.
- `vinput-asr`: ASR backend/session traits, command helper contract, and mock backend.
- `vinput-text`: scene post-processing, prompt rendering, and command text adapter seams.
- `vinput-registry`: registry metadata validation and dry-run asset/install planning.
- `vinput-daemon`: mock/configured runtime, stateful audio-recorder lifecycle, diagnostics, and the legacy `zbus` service facade.
- `vinput-cli`: inspection helpers for protocol/config/registry/status/payloads and audio-device diagnostics.

## Immediate development route

1. Keep `vinput-protocol` ABI-compatible with the original C++ project.
2. Add tests before each protocol/config/runtime behavior change.
3. Keep diagnostics and smoke fixtures deterministic while replacing remaining mock edges: live PipeWire recording, sherpa-onnx backend, registry download/extraction, adapter supervision, and packaging.
4. In parallel, annotate each original `fcitx5-vinput/src` file and map it to a target crate/module before porting non-trivial behavior.
## Compatibility invariant

The C++ Fcitx5 frontend should be able to keep calling the same bus name, object path, interface, methods, signals, status strings, and recognition result JSON shape while the backend is rewritten behind that contract.
