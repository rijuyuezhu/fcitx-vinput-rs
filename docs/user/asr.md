# ASR models and providers

ASR (automatic speech recognition) turns microphone audio into text.

```text
Microphone → ASR → raw text → optional scene/LLM processing → final text
```

Vinpst supports three ASR provider types:

- **Local** — runs a sherpa-onnx model on your machine.
- **Command** — runs an installed helper program that returns recognized text.
- **Remote** — calls an OpenAI-compatible transcription endpoint.

Only one ASR provider is active at a time. Use the **F8** menu to switch quickly while typing.

## Local models

For the simplest offline setup, use the built-in local provider with a managed model.

### GUI

Open **Resources → Models**:

- install a model from the available list;
- click **Use** on an installed model to activate it;
- remove models you no longer need.

The model list shows the language and backend information needed to choose between available models.

### CLI

```sh
vinpst model list -a
vinpst model install <model-id>
vinpst model use <model-id> --in-place --reload-daemon
vinpst model remove <model-id>
```

## ASR providers

Use another provider when you want a cloud service, a streaming service, or a custom recognizer.

### GUI

1. Open **Resources → ASR providers** and install a provider from the registry.
2. Open **Control → ASR providers** to configure it and choose the active provider.
3. Reload or restart the daemon only when the GUI asks you to do so.

Local and remote providers can also be created manually from the Control page when registry installation is not appropriate.

### CLI

```sh
vinpst provider list --available
vinpst provider install <provider-id> --in-place
vinpst provider configure <provider-id> --help
vinpst provider use <provider-id> --in-place
vinpst daemon reload-asr
```

`provider edit` is reserved for editing the script of an installed command provider. Use `provider configure` for typed provider settings.

## Remote providers

Remote ASR sends recorded audio to the configured service, so it requires network access and whatever credentials that service needs. Keep API keys out of bug reports and shared shell history where possible.

The HTTP client honors the usual `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` variables. If a private service uses an additional CA, configure `SSL_CERT_FILE` for the daemon environment.

## Hotwords

Hotwords help supported recognition backends with names, product terms, technical vocabulary, and other words that are otherwise easy to misrecognize.

Use the **Hotwords** page in Vinpst Configuration, or:

```sh
vinpst hotword get
vinpst hotword set /absolute/path/to/hotwords.txt --in-place
vinpst hotword edit
vinpst hotword clear --in-place
```

Not every model or provider supports hotwords. If the selected backend does not support them, Vinpst keeps the configuration but does not pretend that they are active.

## When recognition is not working

Start with:

```sh
vinpst doctor
vinpst daemon status
vinpst daemon log --lines 100
```

See [Troubleshooting](troubleshooting.md) for audio, activation, provider, and model-specific checks.
