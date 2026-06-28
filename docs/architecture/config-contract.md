# Config contract

`vinput-config` owns config parsing, normalization, defaults, and validation. CLI and daemon diagnostics consume the same typed config so file-backed checks stay deterministic.

## Baseline fixture

`data/default-config.json` is the committed compatibility baseline copied from the original project. It is also the stable smoke fixture for explicit config CLI paths:

```sh
cargo run -p vinput-cli -- config validate data/default-config.json --summary-only
cargo run -p vinput-cli -- asr-state --config data/default-config.json
```

Integration tests consume the same committed fixture directly, so changes to config parsing or defaults must keep the CLI summary and ASR diagnostics contracts stable.

## Diagnostics behavior

Config diagnostics parse local JSON only. They do not construct runtime ASR backends, launch helpers, download registry assets, or require the daemon to be running.
