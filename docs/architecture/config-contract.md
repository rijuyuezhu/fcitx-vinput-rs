# Config contract

`vinput-config` owns config parsing, normalization, defaults, and validation. CLI and daemon diagnostics consume the same typed config so file-backed checks stay deterministic.

## Baseline fixture

`data/default-config.json` is the committed compatibility baseline copied from the original project. It is also the stable smoke fixture for explicit config CLI paths:

```sh
cargo run -q -p vinput-cli -- config validate data/default-config.json --summary-only
cargo run -q -p vinput-cli -- asr-state --config data/default-config.json
```

Integration tests consume the same committed fixture directly, so changes to config parsing or defaults must keep the CLI summary and ASR diagnostics contracts stable.

The committed baseline intentionally fixes these compatibility fields:

- ASR provider `sherpa-onnx` as the active local provider placeholder.
- active scene `__raw__`, with `__command__` kept as the command-mode prompt fixture.
- empty `llm.providers` and `llm.adapters`, so text-adapter diagnostics report no configured adapters.

Runtime availability is not implied by the fixture; local `sherpa-onnx` remains a placeholder until the concrete backend is implemented.

## Diagnostics behavior

Config diagnostics parse local JSON only. They do not construct runtime ASR backends, launch helpers, download registry assets, or require the daemon to be running.

`vinput-daemon --config data/default-config.json print-config`, `asr-state`, `text-adapters`, and `audio-devices` are covered by integration tests to keep daemon diagnostics aligned with the same committed fixture. `audio-devices` reports the parsed capture target without constructing the runtime. In default builds it reports `backend: "unavailable"`; with `pipewire-backend` it may enumerate live PipeWire sources, but still succeeds with `live: false` and an `enumeration_error` when PipeWire client configuration or a server is unavailable.
