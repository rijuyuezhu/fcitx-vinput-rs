# Scenes and text processing

ASR produces raw text. A **scene** decides whether that text should be used as-is or sent through an LLM for rewriting.

```text
ASR raw text → scene → optional LLM/adapter → final text
```

The three pieces have different jobs:

- a **scene** says what to do with the text;
- an **LLM provider** says which OpenAI-compatible model/API should do it;
- an **LLM adapter** is an optional local bridge for software that does not already expose a compatible API.

## Built-in scenes

- **Raw (`__raw__`)** — commits recognized text without LLM rewriting.
- **Command (`__command__`)** — combines selected text with a spoken instruction for command mode.

Use **Right Shift** to open the Fcitx scene menu and switch the active ordinary scene.

## Set up an LLM scene

The normal GUI flow is all on the **LLM** page:

1. Add an LLM provider and test its connection.
2. Add a scene.
3. Choose the provider and model for that scene.
4. Write the prompt that describes the transformation.
5. Click **Use** on the scene when you want it active.

Common scene ideas include polishing dictated text, translating it, fixing formatting, or matching a preferred writing style.

`context_lines` controls how much recently committed text is supplied as extra context. Keep it at `0` unless the task benefits from nearby text.

## LLM providers

An LLM provider is an OpenAI-compatible chat endpoint. Different scenes may use different providers or models.

Example CLI setup:

```sh
vinpst llm add local \
  --base-url http://127.0.0.1:11434/v1 \
  --model qwen2.5:7b \
  --in-place
vinpst llm test local
```

Avoid storing or sharing literal API keys when the provider can read them from a deployment-specific environment source.

Removing a provider leaves its scenes in place, but clears their provider and model choices so they can be configured again.

## LLM adapters

If a local model or service needs a bridge process:

1. install the adapter under **Resources → LLM adapters**;
2. configure, start, stop, and inspect it from the **LLM** page;
3. point an LLM provider at the adapter's compatible endpoint when required by that adapter.

CLI equivalents:

```sh
vinpst adapter list --available
vinpst adapter install <adapter-id> --in-place
vinpst adapter start <adapter-id>
vinpst adapter status <adapter-id>
vinpst adapter stop <adapter-id>
```

## Manage scenes from the CLI

```sh
vinpst scene list
vinpst scene add --help
vinpst scene edit <scene-id> --help
vinpst scene use <scene-id> --in-place
vinpst scene remove <scene-id> --in-place
```

Built-in scene identities are retained by configuration normalization and are not removed like normal custom scenes.

## Command mode

Command mode is for editing existing text with your voice:

1. select text in an application;
2. press or hold **F10**;
3. say an instruction such as “translate to English” or “add comments”;
4. stop recording and accept the result.

The built-in command scene keeps the selected text and spoken instruction in separate scoped inputs before they reach the text processor. If processing fails, Vinpst keeps the original selection rather than replacing it with an error or empty result.

See [Dictation and command mode](usage.md) for the desktop interaction details.
