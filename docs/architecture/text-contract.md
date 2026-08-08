# Text post-processing contract

Command-backed text adapters use stdin/stdout JSON and mirror the command ASR helper style.

## Runtime flow

StopRecording passes the final ASR payload into `TextProcessor::finish`. Raw or no-op scenes return `RecognitionPayload::raw`; scenes that need post-processing delegate to a `TextAdapter`; the daemon then emits the resulting `RecognitionPayload` through the existing D-Bus `RecognitionResult` path.

## Core types

- `TextRequest`: raw ASR text, selected scene definition, and optional selected text for command mode.
- `PromptContext` and `PromptTemplate`: deterministic placeholder rendering for scene metadata, legacy `{{ asr }}`/`{{ selected }}`/`{{ context }}` variables, and `file:///` prompt-file loading.
- `TextProcessor`: synchronous runtime seam used by the daemon.
- `TextAdapter`: post-processing seam for command, prompt, provider, timeout, context, or candidate handling.
- `CommandTextAdapter`: configured command adapter that delegates execution to a runner.
- `CommandTextProcessor`: selects configured command adapters for post-processing scenes.
- `OpenAiCompatibleTextAdapter`: builds a non-streaming chat-completions request and delegates transport to an injected seam.
- `OpenAiCompatibleTextProcessor`: selects an OpenAI-compatible provider for a scene and wires the optional recent-input context cache path.
- `ProcessCommandTextRunner`: process-backed runner using stdin/stdout JSON.

## Module layout

The text crate is split by responsibility so future HTTP transport work can land without growing a monolith:

- `error.rs`: `TextError`;
- `core.rs`: `TextRequest`, processor/adapter traits, mock and default finishers;
- `prompt.rs`: prompt context, file URI loading, interpolation, and XML helpers;
- `context_cache.rs`: recent-input JSONL path, append, truncate, and raw context prefix helpers;
- `openai.rs`: OpenAI-compatible request building, provider selection, candidate parsing, injected transport seams, and the blocking reqwest HTTP transport;
- `command.rs`: command text adapter request/response protocol and process runner;
- `adapter_runtime.rs`: supervised adapter process pid-file lifecycle;
- `payload.rs`: command-mode payload ordering;
- `tests.rs`: behavior-preserving unit coverage.

## Command-mode payload contract

Command mode preserves the legacy frontend-visible candidate order:

1. selected text as a `raw` candidate when command mode has selected text;
2. recognized command text as an `asr` candidate;
3. LLM/post-processing candidates as `llm` candidates when available.

Commit text prefers the first LLM/post-processing candidate when one exists. Without LLM candidates, command mode falls back to the selected text when present, otherwise the ASR command text. The Rust daemon only receives the selected text string over D-Bus or CLI and returns the recognition payload; the retained C++ frontend owns selected-text replacement and cleanup for command-mode commit/candidate outcomes. It first uses Fcitx surrounding text and falls back to the primary-selection clipboard path when surrounding text is unavailable; multi-application live validation remains pending.

## Command adapter process contract

A command text adapter helper is configured by `llm.adapters[]` and is executed with the configured command, args, environment, and optional working directory. The runner writes one `CommandTextRequest` JSON object to stdin, appends a newline, closes stdin, waits for the process, and decodes one `CommandTextResponse` JSON object from stdout.

Request fields include `adapter_id`, `raw_text`, optional `selected_text`, and a `scene` object with id, label, prompt, provider id, model, candidate count, timeout, and context line metadata.

Response fields are `payload` for a full `RecognitionPayload`, `text` for a simple final post-processed text, or `error` for a helper-level error. `failure` is accepted as a legacy alias for `error`. Full payload responses are normalized with the same compatibility rules as the D-Bus recognition payload. Empty or whitespace-only `text` is rejected as a missing final text response. Empty or whitespace-only `error` is ignored.

The command text adapter contract mirrors the command ASR helper style: one JSON request on stdin, one JSON response on stdout, explicit typed errors, and injected runner seams in tests.

## Long-running adapter lifecycle

Configured adapter services start in dedicated Unix process groups. The runtime directory is created before spawn so a child cannot race PID-file publication while opening readiness or state paths beneath that directory. The runtime then persists a versioned JSON record containing the leader PID and Linux `/proc/<pid>/stat` start time through a mode-0600 temporary file followed by atomic rename. A matching live fingerprint rejects duplicate start; a missing, exited, or mismatched fingerprint is stale and may be removed before a new start. Legacy integer-only PID files are fail-closed: start refuses to overwrite them, while stop removes them without sending a signal because they cannot distinguish the original adapter from PID reuse.

Tracked adapters preserve the legacy shutdown schedule: send `SIGTERM`, wait up to two seconds, then send `SIGKILL` and wait up to three more seconds. Stop, daemon drop, and exited-process refresh operate on the whole process group, so descendants are terminated even when the direct child has already exited. On Linux, tracked direct-child cleanup uses the shared `waitid(WNOWAIT)` boundary before reap. Recovery of an untracked fingerprinted record first verifies its current `/proc` start time, then waits for non-zombie process-group members rather than signal-zero existence alone: a zombie-only group counts as cleaned, while a live background descendant keeps the group active. This is a recovery path for a prior daemon owner, not a claim of pidfd-based supervision or cross-daemon start locking.

## Daemon runtime wiring

The default daemon constructor still uses mock text processing for prototype compatibility. To exercise configured backends explicitly, run the daemon with `--configured-backends`. That path builds ASR from `asr.active_provider` and text post-processing from `llm.providers[]` through `ReqwestOpenAiCompatibleChatTransport` when providers are configured; when no providers are configured it falls back to command adapters from `llm.adapters[]`.

Prompt-file compatibility mirrors the legacy daemon: only literal `file:///absolute/path` URIs are accepted, the path is loaded only when it points to a regular file, and reads are capped at 256 KiB. Legacy double-brace interpolation accepts optional whitespace around variable names; unsupported variables are preserved verbatim. Plain `PromptTemplate` rendering keeps `{{context}}` empty for deterministic non-runtime tests, while OpenAI-compatible request builders can load the recent-input cache and inject the rendered context prefix. OpenAI-compatible request helpers preserve the legacy `extra_body` merge rule: provider-specific fields pass through, while `messages`, `stream`, and `response_format` are protected because they are required for the JSON candidates contract. Request diagnostics redact the HTTP auth header, remove URL userinfo/fragments, replace query values with `REDACTED`, and show only top-level body keys in `Debug`; the actual transport request retains its original URL, query values, headers, and JSON body.

The recent-input cache helpers mirror the legacy split: frontend-facing code can buffer committed fragments with legacy CJK/space/flush rules, append JSONL entries, and truncate the cache to the last non-empty lines, while daemon-facing request builders read raw non-empty lines and send the last `scene.context_lines` lines as context. The default cache path follows legacy XDG order: `XDG_CACHE_HOME/vinpst/context.jsonl`, then `$HOME/.cache/vinpst/context.jsonl`, then relative `vinpst/context.jsonl` when no base exists.

`CommandTextProcessor` dispatches only when exactly one command adapter is configured, and `CommandTextRequest` carries the effective scene timeout. `ProcessCommandTextRunner` uses the shared `vinpst-process` supervisor for process groups, whole-operation deadlines, descendant cleanup, concurrent output draining, and independent 1 MiB stdout/stderr limits. Rust tests cover ignored stdin, background descendants, large stderr, overflow rejection, missing/ambiguous adapters, prompt construction, the legacy 4000 ms default, request/response-body timeouts, response bounds, redirect refusal, and secret-safe diagnostics.

`OpenAiCompatibleTextProcessor` uses the scene provider when set and otherwise requires exactly one configured provider. `ReqwestOpenAiCompatibleChatTransport` is the concrete blocking transport. `scripts/tests/asr/run-openai-compatible-text-provider-fixture-smoke.sh` keeps only a short production `vinpst llm test` loopback boundary for authenticated request shape and result parsing; the larger proxy/TLS matrix is exercised once at the shared HTTP/remote-ASR boundary instead of duplicated for text. `scripts/tests/asr/run-provider-ca-rotation-smoke.sh` retains the cross-provider same-daemon CA replacement proof.

`scripts/live/niri/run-ime-fcitx-external-text-provider-live.sh` remains the real configured-daemon/F10 evidence: HTTP failure commits/deletes nothing, recovery sends selected and raw ASR text and commits the chosen candidate, and empty selection is rejected before provider access. This is local deterministic evidence, not hosted-provider, PAC, NTLM/Kerberos, enterprise certificate-deployment, or production credential-lifecycle proof.

## Diagnostics

The daemon exposes `text-adapters` as a CLI diagnostic subcommand and `GetTextAdapterState` as a D-Bus diagnostic method. `GetRuntimeStatus` includes the same text-adapter state inside a broader runtime JSON snapshot. Both read the same runtime config and serialize the shared `TextAdapterState` JSON shape:

- `adapter_count`: number of configured command text adapters.
- `adapter_ids`: configured adapter ids in config order.
- `single_adapter_id`: the only configured adapter id, or `null` when no unique adapter exists.
- `adapters`: sanitized per-adapter summaries with `id`, `kind`, `args_count`, `env_count`, `has_working_dir`, `is_running`, and `pid`.

`is_running` and `pid` are runtime observations. Static diagnostics such as `vinpst-daemon text-adapters` report `is_running: false` and `pid: null`. A live daemon updates those fields from its supervised adapter process table, so `GetTextAdapterState` can show a started adapter as running without exposing environment keys, environment values, or working directory paths. `GetTextAdapterState` also reaps supervised adapter processes that have already exited; exited adapters are reported as `is_running: false` with `pid: null`, and their pid files are removed.

Diagnostics intentionally do not execute helpers or construct runtime backends. They report argument and environment counts for routing visibility, but never include the configured command path, command arguments, environment keys, environment values, configured working directory path, or forward-compatible adapter fields. Passing `--configured-backends` does not change `print-config`, `asr-state`, or `text-adapters`; those commands are safe to run even when configured runtime backends are unavailable.
