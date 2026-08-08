# Troubleshooting

Start with non-destructive diagnostics:

```sh
vinpst doctor
vinpst daemon status
vinpst recording status
vinpst device list
vinpst daemon log --lines 100
systemctl --user status vinpst-daemon.service
```

Use `--json` when collecting structured output. Review it before sharing; provider configuration and local paths can still be sensitive even when known credentials are redacted.

## Daemon is unavailable

```sh
systemctl --user enable --now vinpst-daemon.service
vinpst daemon status
```

If D-Bus activation metadata is installed but the service still fails, inspect:

```sh
systemctl --user status vinpst-daemon.service
journalctl --user -u vinpst-daemon.service -n 100
```

After changing package files or service metadata:

```sh
systemctl --user daemon-reload
systemctl --user restart vinpst-daemon.service
```

## Fcitx does not show Vinpst

Reload Fcitx:

```sh
fcitx5 -r
```

Then run `vinpst doctor`. It checks common addon metadata and library locations. Verify that the installed addon is named `vinpst` and the module is `fcitx5-vinpst.so`; do not rename them to another project identity.

## No usable ASR backend

```sh
vinpst doctor
vinpst model list --installed
vinpst provider list
```

The packaged daemon is allowed to stay running while the active ASR backend is unavailable. On a fresh configuration this is the expected setup state until a model is installed and selected; `vinpst doctor` reports `"status": "setup-required"` while keeping the diagnostic command itself usable. Install/select a compatible model or provider, then reload:

```sh
vinpst daemon reload-asr
```

If a new provider fails to load, the previous working provider should remain active. Capture `vinpst daemon status --json` and the relevant bounded journal section when reporting a failure.

## No audio or wrong microphone

```sh
vinpst device list
```

Select another target and restart the daemon:

```sh
vinpst device use <target> --in-place
vinpst daemon restart
```

Check that the desktop session can access PipeWire and that the selected object still exists after reconnecting audio hardware.

## Dictation starts but returns no text

Check:

- the selected provider/model supports the configured language;
- captured input is not silent or clipped;
- VAD threshold and minimum durations are reasonable;
- command providers are executable and finish before their deadline;
- remote providers are reachable and accept the configured request format.

Use ordinary dictation before debugging scenes or command mode so ASR and post-processing failures are separated.

## Command mode says to select text

The focused application did not provide selected surrounding text and no usable primary selection was available. Select text again and keep the target application focused while triggering command mode.

Some applications expose selection visually but do not provide it through the input-method surrounding-text protocol. See [Known limitations](limitations.md).

## Remote provider failures

Check endpoint, model, credentials, timeout, proxy, and CA settings. Vinpst rejects redirects, untrusted TLS, oversized bodies, invalid UTF-8, DNS failures, and connection failures rather than silently retrying another URL.

Do not disable TLS verification. Configure an additional trusted CA through `SSL_CERT_FILE` when required.

## Configuration was rejected

Validate the exact file:

```sh
vinpst config validate /path/to/config.json
```

Use `vinpst config edit` or the GUI for safe edits. Vinpst rejects configuration schema versions newer than the running binary supports.

## GUI cannot save

The GUI refuses unsafe writes when:

- the configuration changed externally after loading;
- the daemon is recording or has an active session;
- a target file is not a regular file;
- required ownership, mode, xattr, or ACL preservation cannot be guaranteed;
- a managed resource path no longer matches the validated plan.

Read the error and refresh before retrying. Do not bypass these checks by editing managed files through symlinks.
