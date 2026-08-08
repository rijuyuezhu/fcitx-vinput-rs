# niri desktop gates

These runners automate the maintainer's niri/Wayland desktop. They may use
`niri msg`, kernel uinput, Fcitx input contexts, PRIMARY selection ownership,
PipeWire virtual nodes, and application-specific window matching.

`probes/` contains the small GTK, Qt, Chromium, and Fcitx clients compiled or
executed by the runners. The probes are test instruments rather than product
code.

Common entry points include:

```sh
scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
scripts/live/niri/run-ime-gtk4-native-live.sh normal
scripts/live/niri/run-ime-chromium-virtual-live.sh command
scripts/live/niri/run-ime-kitty-live.sh command
```

Management-GUI desktop automation is intentionally not retained. Real GUI window,
widget, focus, dialog, and visual behavior is validated manually. Automated coverage
stops below the Iced window/widget boundary and uses crate-internal semantic state,
typed-message, persistence, and protocol tests.

Do not claim portability from these results. A new compositor/backend needs its
own focus, key-injection, selection, and cleanup implementation or a shared
portable abstraction with equivalent evidence.
