# Registry contract

`vinput-registry` owns local registry metadata parsing and planning. It stays separate from download and extraction side effects so CLI validation can run deterministically in tests and smoke checks.

## Registry shape

A registry index is a JSON object with:

- `version`: registry schema version.
- `models`: ASR model entries with `id`, `label`, `provider`, and `assets`.
- `adapters`: optional text adapter entries with `id`, `label`, `kind`, and `assets`.

Each asset path must be a safe relative path. Optional `sha256` checksums must be lowercase 64-character hexadecimal strings.

## CLI diagnostics

`vinput-cli registry` prints the configured registry mirror URLs from the bundled config. File-backed diagnostics use explicit paths:

```sh
cargo run -p vinput-cli -- registry validate data/sample-registry-index.json
cargo run -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
```

These commands parse local JSON only. They do not download assets or touch install directories.

## Fixture

`data/sample-registry-index.json` is the stable smoke fixture for registry validation and planning. Integration tests also consume it directly so changes to registry parsing, planning, or fixture format fail before smoke output drifts.
