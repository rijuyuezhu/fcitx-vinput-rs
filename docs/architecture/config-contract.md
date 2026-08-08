# Config contract

`vinpst-config` owns config parsing, normalization, defaults, and validation. CLI and daemon diagnostics consume the same typed config so file-backed checks stay deterministic.

## Baseline fixture

`data/default-config.json` is the committed compatibility baseline aligned with the current upstream defaults. It is also the stable smoke fixture for explicit config CLI paths:

```sh
cargo run -q -p vinpst-cli -- config validate data/default-config.json --summary-only
cargo run -q -p vinpst-cli -- asr-state --config data/default-config.json
```

Daemon config resolution uses the canonical Vinpst XDG path. An explicit `--config` path has highest priority. Without it, the daemon reads `$XDG_CONFIG_HOME/fcitx-vinpst/config.json`, falling back to `$HOME/.config/fcitx-vinpst/config.json`; only a missing user file falls back to the bundled default. CLI `doctor`, `asr-state`, and `audio-devices` use the same explicit/discovered/bundled priority so diagnostics describe the configuration the normal daemon will consume. A discovered user file is retained as the runtime persistence path, so D-Bus scene/provider selection and config reload update the same file. `scripts/tests/daemon/run-daemon-default-config-smoke.sh` starts the daemon on a private session bus without `--config`, switches the active scene, and verifies the discovered file is atomically updated.

Integration tests consume the same committed fixture directly, so changes to config parsing or defaults must keep the CLI summary and ASR diagnostics contracts stable.

The committed baseline intentionally fixes these compatibility fields:

- output ducking disabled by default, with a `duck_output_volume` multiplier of `0.25` when enabled;
- ASR provider `sherpa-onnx` as the active local provider placeholder.
- active scene `__raw__`, with `__command__` using the current upstream scoped interpolation prompt. The prompt places selected text and ASR text in `<vinput-selected>` and `<vinput-asr>` blocks through `{{selected}}` and `{{asr}}`, so request assembly does not append a second copy of either input.
- empty `llm.providers` and `llm.adapters`, so text-adapter diagnostics report no configured adapters.

Runtime availability is not implied by the fixture; local `sherpa-onnx` requires the feature-gated native backend and a compatible installed model.

## Legacy compatibility policy

The legacy C++ project accepted or repaired some malformed user config shapes more loosely. The Rust contract is intentionally explicit: parsing may normalize missing builtin scenes and blank/missing `active_scene` to `__raw__`. Missing `__command__`, an empty command prompt, the former free-form command prompt, and the former short-tag `<selected>/<asr>` prompt are upgraded to the current upstream scoped interpolation prompt. The one numeric compatibility repair retained here is `global.duck_output_volume`, which clamps finite parsed values to `0.0..=1.0` like legacy. Validation still rejects programmatically constructed non-finite values and does not silently deduplicate or drop invalid entries.

Config-file failure behavior intentionally differs from upstream where fail-closed handling protects user data. Both projects fall back to the bundled default only when the normal user config file is absent. With an existing malformed JSON file, the compiled upstream CLI reports the parse failure but exits successfully with a partially defaulted config; Vinpst returns an error instead. The compiled upstream CLI also accepts an unknown future `version`, while Vinpst rejects schema versions newer than `CURRENT_CONFIG_VERSION`. This prevents an older binary from silently reading or later rewriting a config format it does not understand.

Pinned decisions, covered by `crates/vinpst-config/tests/legacy_compat.rs`:

- duplicate or blank registry mirrors are rejected, not deduplicated or dropped.
- duplicate or blank LLM provider, LLM adapter, and ASR provider ids are rejected.
- command ASR providers must configure a non-empty `command`.
- `global.duck_output_while_recording` defaults to `false`; `global.duck_output_volume` defaults to `0.25`, finite parsed values are clamped to `0.0..=1.0`, and non-finite runtime values are rejected.
- VAD threshold and duration values are strictly range-checked instead of silently clamped: threshold `0.05..=0.95`, minimum speech `0.05..=2.0` seconds, minimum silence `0.05..=5.0` seconds, and speech padding at most `2000` ms.
- omitted scene `timeout_ms` uses the legacy 4000 ms request deadline; an explicitly provided zero remains invalid rather than being repaired. Scene `candidate_count` and `context_lines` limits are strict and are not clamped after invalid values are provided.
- missing active scene references are rejected. Unknown non-empty active ASR provider references are rejected when an explicit provider list is present; minimal diagnostics configs that omit providers retain the historical placeholder behavior. The exact empty string is a valid legacy-compatible "no provider selected" state, while whitespace-only ids remain invalid.
- unknown scene `provider_id`, blank scene `model`, and blank scene `prompt` are rejected. Non-empty `model` or `prompt` without a provider remains accepted by the current validation contract.

These tests document compatibility policy rather than feature parity: future migration work may choose to implement more legacy-style repair, but it must update the tests and this document deliberately.

Provider removal follows the legacy config lifecycle: local providers are retained, removing the active non-local provider clears `asr.active_provider`, and the resulting config remains valid but has no runtime backend selected. Configured runtime construction reports that state as unavailable instead of choosing another provider implicitly.

## Offline VAD fields

`asr.vad` preserves the legacy offline Silero controls: `enabled`, `threshold`, `min_speech_duration`, `min_silence_duration`, and `speech_pad_ms`. Defaults are `true`, `0.45`, `0.15`, `0.5`, and `300` respectively. The native runtime applies them only to buffered offline sherpa recognition; online/streaming recognition does not use this trimmer.

## Diagnostics behavior

Config diagnostics parse local JSON only. They do not construct runtime ASR backends, launch helpers, download registry assets, or require the daemon to be running.

`VinpstConfig::summary()` is the compact config diagnostic surface. It reports validation status, schema version, active scene/provider ids, and counts only. It must not serialize secret-bearing config fields such as LLM API keys, provider or adapter environment values, command arguments, working directories, provider base URLs, or forward-compatible extra bodies. `redact_url_for_diagnostics` is the shared URL diagnostic boundary for ASR and text providers: it removes userinfo and fragments, preserves scheme/host/port/path plus query-key order and duplicates, and replaces every query value with `REDACTED`. Invalid URLs become the fixed marker `<invalid-url>`. This helper never mutates the configured URL used for an HTTP request.

`vinpst-daemon --config data/default-config.json print-config`, `asr-state`, `text-adapters`, and `audio-devices` are covered by integration tests to keep daemon diagnostics aligned with the same committed fixture. `audio-devices` reports the parsed capture target without constructing the runtime. In default builds it reports `backend: "unavailable"`; with `pipewire-backend` it may enumerate live PipeWire sources, but still succeeds with `live: false` and an `enumeration_error` when PipeWire client configuration or a server is unavailable.
