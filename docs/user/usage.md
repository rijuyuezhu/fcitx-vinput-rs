# Dictation and command mode

## Normal dictation

1. Focus a text field.
2. Press or hold the normal dictation key (Right Control by default).
3. Speak.
4. Stop recording.

Vinpst sends the final result through Fcitx. Streaming ASR backends may show partial recognition as preedit while you are speaking.

## Trigger modes

The Vinpst Fcitx addon supports:

- **Tap** — press once to start and again to stop;
- **Hold** — hold to record and release to stop;
- **Both** — short presses toggle while a sustained press behaves as push-to-talk.

Change the trigger mode and keys in the Fcitx configuration tool under the **Vinpst** addon.

## Command editing

Command mode edits text that is already selected:

1. Select text in the focused application.
2. Press or hold the command key (F10 by default).
3. Say what you want to do, for example “translate this to English” or “make this more concise.”
4. Stop recording and choose a result if multiple candidates are offered.

Command mode needs the built-in command scene plus a configured LLM provider or compatible text adapter. Vinpst replaces the selection only after processing succeeds.

See [Scenes and text processing](scenes.md) for setup.

## Switch ASR or scene while typing

- **F8** opens the ASR provider/model menu.
- **Right Shift** opens the scene menu.
- Use the configured paging keys if a menu has more than one page.
- Press **Esc** to close a menu without committing a choice.

If a requested ASR backend cannot be prepared, Vinpst keeps the previous working backend rather than switching to a broken one.

## Application compatibility

Normal dictation works anywhere ordinary Fcitx text commits work. Command editing additionally depends on the application exposing selected/surrounding text or a usable primary selection.

GTK, Qt, Chromium/Ozone, GNOME Text Editor, kitty, and VS Code/Electron paths have live coverage, but application behavior can still differ. See [Known limitations](limitations.md) when selection replacement behaves differently in one application.
