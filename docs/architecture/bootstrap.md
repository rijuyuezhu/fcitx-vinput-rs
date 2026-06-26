# Bootstrap architecture

This repo starts with a deliberately small Rust core before porting the full C++ implementation.

## Current crates

- `vinput-protocol`: stable D-Bus names, status strings, ASR state, and recognition result JSON.
- `vinput-config`: typed model for the legacy `data/default-config.json`, including initial validation.
- `vinput-daemon`: mock runtime that exercises the daemon state machine without PipeWire, sherpa-onnx, or D-Bus yet.
- `vinput-cli`: inspection helpers for protocol/config/status/payloads.

## Immediate development route

1. Keep `vinput-protocol` ABI-compatible with the original C++ project.
2. Add tests before each protocol/config behavior change.
3. Replace the mock daemon edges in this order: zbus service, runtime actor, PipeWire capture, ASR session trait, sherpa-onnx backend, post-processing, adapter supervision.
4. In parallel, annotate each original `fcitx5-vinput/src` file and map it to a target crate/module before porting non-trivial behavior.

## Compatibility invariant

The C++ Fcitx5 frontend should be able to keep calling the same bus name, object path, interface, methods, signals, status strings, and recognition result JSON shape while the backend is rewritten behind that contract.
