# Registry contract

`vinput-registry` owns local registry metadata parsing and planning. It stays separate from download and extraction side effects so CLI validation can run deterministically in tests and smoke checks.

## Module layout

The registry crate is split before any side-effectful installer work lands:

- `schema.rs`: registry index, model, adapter, asset, summary, validation, and URL resolution helpers;
- `plan.rs`: planned assets, dry-run install plans, checksum policy planning, and target path calculation;
- `error.rs`: `RegistryError`;
- `fetch.rs`: registry text fetch boundary, ordered mirror fallback, and the concrete `ReqwestRegistryTextSource` for HTTP index text fetching;
- `cache.rs`: text-only registry index cache read/write boundary with same-directory temporary file and rename updates;
- `tests.rs`: behavior-preserving schema, safety, planning, injected-source fetch, local HTTP fetch, and stale-cache fallback coverage.

Future checksum/archive extraction/materialization code should use separate modules and must not be hidden inside schema, dry-run planning, concrete HTTP text fetch code, or text cache code.

## Registry shape

A registry index is a JSON object with:

- `version`: registry schema version.
- `models`: ASR model entries with `id`, `label`, `provider`, and `assets`.
- `adapters`: optional text adapter entries with `id`, `label`, `kind`, and `assets`.

Each asset path must be a safe relative path. Optional `sha256` checksums must be lowercase 64-character hexadecimal strings.

## CLI diagnostics

`vinput-cli registry` prints the configured registry mirror URLs from the bundled config. File-backed diagnostics use explicit paths:

```sh
cargo run -q -p vinput-cli -- registry validate data/sample-registry-index.json
cargo run -q -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
```

These commands parse local JSON only. They do not download assets or touch install directories.

The library exposes `fetch_registry_index_from_mirrors` as the shared mirror fallback boundary. It iterates mirror URLs through a `RegistryTextSource`, falls through on transport failures, stops on the first fetched-but-invalid registry body, and performs the same `RegistryIndex` validation as file-backed CLI diagnostics. `ReqwestRegistryTextSource` is the implemented concrete HTTP registry index text source behind that boundary; it fetches JSON text from mirror URLs with sanitized transport/status errors and no auth/header/body leakage. `RegistryTextCache` and `fetch_registry_index_with_cache` are implemented as a text-only stale-cache boundary: fresh successful fetches parse before writing cache, write cache through a temporary file plus rename, and fall back to stale cache only when fresh mirror fetch fails. Checksum verification, asset download, archive extraction, install, and config materialization remain future work.

## Fixture

`data/sample-registry-index.json` is the stable smoke fixture for registry validation and planning. Integration tests also consume it directly so changes to registry parsing, planning, or fixture format fail before smoke output drifts.

The committed sample intentionally fixes these contract ids:

- model `sherpa-zh-small` with provider `sherpa-onnx` and asset `models/sherpa-zh-small.tar.zst`.
- adapter `mock-adapter` with kind `command` and no bundled assets yet.

Treat these as smoke-test fixtures rather than a real downloadable registry catalog.
