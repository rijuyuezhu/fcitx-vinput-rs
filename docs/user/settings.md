# Settings

Most everyday Vinpst settings are on the **Control** page of Vinpst Configuration.

```sh
vinpst-gui
```

Vinpst's speech/LLM settings and Fcitx trigger-key settings are separate. Use Vinpst Configuration for the former and the Fcitx configuration tool for the latter.

## Audio

### Capture device

Choose the microphone/capture target used by PipeWire. Select **Default** to follow the current system default.

To list devices from the CLI:

```sh
vinpst device list
```

### Normalize audio

**Normalize audio** evens out completed recordings before recognition. It is useful when microphone levels vary between recordings.

### Input gain

**Input gain** multiplies the captured signal before recognition. Start near `1.0×` and raise it gradually if the microphone is too quiet. Excessive gain can clip audio and reduce recognition quality.

### Voice activity detection

**Enable voice activity detection** trims silence for supported local recognition paths. The normal GUI exposes the on/off choice; advanced threshold/timing values remain available in the JSON configuration.

### Reduce output volume while recording

Enable **Reduce output volume while recording** when speaker audio is leaking back into the microphone. The volume slider controls how much of the current output level remains during recording; Vinpst restores the previous volume afterward.

## ASR provider

The Control page shows the configured ASR providers and the current provider. Use **Resources → ASR providers** when you want to install another registry provider; return to Control to configure or select it.

See [ASR models and providers](asr.md) for the full workflow.

## Daemon

The daemon controls are at the bottom of the Control page. In normal use you should only need **Start**, **Stop**, or **Restart** when changing setup or recovering from a problem. If the daemon behaves unexpectedly, run `vinpst doctor`.

## Fcitx keys and trigger mode

Open the Fcitx configuration tool and select the **Vinpst** addon. You can change:

- normal dictation keys;
- command-mode keys;
- scene-menu and ASR-menu keys;
- candidate paging keys;
- Tap/Hold/Both trigger behavior.

Current defaults are Right Control for normal dictation, F10 for command mode, Right Shift for the scene menu, and F8 for the ASR menu.

## Advanced configuration

The main configuration file is:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinpst/config.json
```

For values not exposed by the normal GUI, use the CLI or edit the JSON deliberately:

```sh
vinpst config get /global/default_language
vinpst config set /asr/vad/threshold 0.45 --in-place
vinpst config validate \
  "${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinpst/config.json"
```

In-place CLI mutations create a nearby backup when replacing an existing file. Vinpst Configuration also refuses to overwrite a file that changed externally after it was loaded.

Fcitx stores addon key settings separately under its own `conf/vinpst.conf`; those settings are not part of the daemon JSON file.

For command-by-command syntax, see [CLI overview](cli.md). For service, audio, or configuration failures, see [Troubleshooting](troubleshooting.md).
