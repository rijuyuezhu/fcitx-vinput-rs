# CLI overview

`vinpst` is the management and diagnostics command for the daemon, configuration, and managed resources.

```sh
vinpst --help
vinpst <command> --help
```

Most commands that can emit structured data accept `--json`; `-j/--json` is also available as a global option.
The normal text output is intended for people. Use `--json` when you need transport details, exact paths, maintenance plans, or stable fields for automation.
Resource tables and mutation messages intentionally omit implementation plumbing such as config-source labels, command/environment contents, fixture roots, internal counters, and D-Bus plans. Human-readable text is not a machine contract.

## Main command groups

| Command | Purpose |
| --- | --- |
| `init` | Create the default user configuration and managed directories. |
| `config` | Validate, read, update, or safely edit configuration. |
| `daemon` | Start, inspect, reload, restart, or stop the daemon. |
| `recording` | Start, stop, toggle, or inspect recording. |
| `device` | List or select capture devices. |
| `model` | List, inspect, install, select, or remove managed models. |
| `provider` | Manage local, command, and remote ASR providers. |
| `hotword` | Inspect or edit provider hotword-file configuration. |
| `scene` | Manage post-processing scenes. |
| `llm` | Manage and test OpenAI-compatible LLM providers. |
| `adapter` | Manage command text adapters and their daemon processes. |
| `doctor` | Run combined configuration, ASR, audio, activation, and addon diagnostics; `ok` reflects active ASR readiness and `status` distinguishes `ready` from `setup-required`. |

Low-level protocol, registry-validation, package-lifecycle, fixture-path, and test helpers remain callable for project tooling but are intentionally omitted from `--help`. They are not part of the ordinary user workflow.

## Safe mutation pattern

Commands that change configuration generally support:

- `--dry-run` to preview;
- `--json` for a machine-readable plan/result;
- `--config <path>` for an explicit input;
- `--output <path>` for a separate result;
- `--in-place` for a validated replacement with an adjacent backup.

A typical workflow is:

```sh
vinpst scene use my-scene --dry-run --json
vinpst scene use my-scene --in-place
```

Use `--reload-daemon` where provided, or reload/restart the daemon after changing active ASR settings.

## Registry install versus custom entries

The CLI distinguishes managed registry resources from custom configuration:

- `model install`, `provider install`, and `adapter install` resolve registry metadata and publish managed files; the upstream-compatible `add` spelling remains accepted for all three managed installs;
- `provider create` and `adapter create` create explicit custom configuration entries, while `provider configure` edits typed provider configuration; `llm add` keeps its upstream configuration-management meaning.

Review the subcommand help before running a mutation. Pre-release Vinpst does not promise command-line compatibility with another project.

## Exit behavior

Commands return a non-zero exit status for invalid input, failed validation, unavailable services, unsafe paths, failed provider/resource operations, or incomplete mutations. Do not parse human-readable output in automation; use `--json` and still check the process exit status.
