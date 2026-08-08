# Vinpst

Vinpst adds voice input and voice-driven text editing to Fcitx 5.

Use it to dictate into normal text fields, switch between local or remote speech-recognition backends, and optionally rewrite recognized or selected text with an LLM.

## Start here

1. [Install Vinpst](user/installation.md).
2. Follow the [Quick start](user/quick-start.md) to install a model and dictate your first sentence.
3. Read [ASR models and providers](user/asr.md) when you want another recognition backend.
4. Read [Scenes and text processing](user/scenes.md) for polishing, translation, and voice-driven editing.

If something does not work, start with [Troubleshooting](user/troubleshooting.md) or run:

```sh
vinpst doctor
```

## Main features

- Local sherpa-onnx speech recognition.
- Command and remote ASR providers.
- Fcitx preedit, candidate selection, and text commit.
- Voice commands that rewrite selected text.
- LLM scenes for polishing, translation, formatting, and other post-processing.
- Hotwords for supported ASR backends.
- A management GUI for everyday setup and a CLI for scripting and advanced configuration.

## Release status

Vinpst is preparing its first `0.1.0` release. Until public release packages are published, use the development-install instructions only if you are intentionally testing the current checkout.

Contributor, architecture, migration, and release-maintenance documents are under the **Development** section of this site; they are not required for normal use.
