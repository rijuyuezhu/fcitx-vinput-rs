# Quick start

This guide assumes Vinpst is installed and `vinpst` is available in your shell.

## 1. Initialize and start Vinpst

```sh
vinpst init
systemctl --user enable --now vinpst-daemon.service
fcitx5 -r
```

Open the Fcitx configuration tool and make sure the **Vinpst** addon is enabled.

## 2. Install a speech-recognition model

Open **Vinpst Configuration** from your application menu, or run:

```sh
vinpst-gui
```

On **Resources**:

1. Find a model under **Models**.
2. Install it.
3. Click **Use** on the installed model.

That is enough for local dictation. The default configuration already contains the local `sherpa-onnx` provider and the raw scene.

CLI alternative:

```sh
vinpst model list -a
vinpst model install <model-id>
vinpst model use <model-id> --in-place --reload-daemon
```

## 3. Dictate

Focus a text field and use the normal dictation key.

| Action | Default key |
| --- | --- |
| Normal dictation | Right Control |
| Command editing | F10 |
| ASR provider/model menu | F8 |
| Scene menu | Right Shift |

The default trigger mode supports both tap-to-toggle and hold-to-talk behavior.

For ordinary dictation, press or hold **Right Control**, speak, then stop recording. Streaming backends may show partial text before the final result is committed through Fcitx.

## 4. Optional: voice-edit selected text

Command mode needs an LLM provider or compatible text adapter.

1. Select text in an application.
2. Press or hold **F10**.
3. Say an instruction such as “translate this to English” or “make this shorter.”
4. Stop recording and choose a candidate if more than one is returned.

See [Scenes and text processing](scenes.md) for setup.

## If setup does not work

Run:

```sh
vinpst doctor
vinpst daemon status
```

A fresh installation without a selected model is reported as **setup required** rather than a daemon crash. For other problems, see [Troubleshooting](troubleshooting.md).

To change trigger keys or tap/hold behavior, open the **Vinpst** addon in the Fcitx configuration tool. See [Settings](settings.md) for the rest of the everyday options.
