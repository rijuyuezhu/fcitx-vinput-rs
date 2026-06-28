# Text post-processing contract

Command-backed text adapters use stdin/stdout JSON and mirror the command ASR helper style.

## Runtime flow

StopRecording passes the final ASR payload into `TextProcessor::finish`. Raw or no-op scenes return `RecognitionPayload::raw`; scenes that need post-processing delegate to a `TextAdapter`; the daemon then emits the resulting `RecognitionPayload` through the existing D-Bus `RecognitionResult` path.

## Core types

- `TextRequest`: raw ASR text, selected scene definition, and optional selected text for command mode.
- `PromptContext` and `PromptTemplate`: deterministic placeholder rendering for scene metadata.
- `TextProcessor`: synchronous runtime seam used by the daemon.
- `TextAdapter`: post-processing seam for command, prompt, provider, timeout, context, or candidate handling.
- `CommandTextAdapter`: configured command adapter that delegates execution to a runner.
- `ProcessCommandTextRunner`: process-backed runner using stdin/stdout JSON.

## Command adapter process contract

A command text adapter helper is configured by `llm.adapters[]` and is executed with the configured command, args, environment, and optional working directory. The runner writes one `CommandTextRequest` JSON object to stdin, appends a newline, closes stdin, waits for the process, and decodes one `CommandTextResponse` JSON object from stdout.

Request fields include `adapter_id`, `raw_text`, optional `selected_text`, and a `scene` object with id, label, prompt, provider id, model, candidate count, timeout, and context line metadata.

Response fields are `payload` for a full `RecognitionPayload`, `text` for a simple final post-processed text, or `error` for a helper-level error. `failure` is accepted as a legacy alias for `error`. Full payload responses are normalized with the same compatibility rules as the D-Bus recognition payload. Empty or whitespace-only `text` is rejected as a missing final text response. Empty or whitespace-only `error` is ignored.

The command text adapter contract mirrors the command ASR helper style: one JSON request on stdin, one JSON response on stdout, explicit typed errors, and injected runner seams in tests.

## Daemon runtime wiring

The default daemon constructor still uses mock text processing for prototype compatibility. To exercise configured backends explicitly, run the daemon with `--configured-backends`. That path builds ASR from `asr.active_provider` and text post-processing from `llm.adapters[]`.

`CommandTextProcessor` only dispatches a post-processing scene when exactly one command adapter is configured. With no adapters it returns `AdapterRequired`; with multiple adapters it returns `AmbiguousAdapter` instead of guessing. Runtime code can use `RuntimeState::with_configured_backends` for configured ASR plus configured command text adapters, or `RuntimeState::with_configured_text` when ASR/audio seams are injected in tests.
