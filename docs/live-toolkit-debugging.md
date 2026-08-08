# Live toolkit debugging runbook

This runbook documents the reusable debugging method established while validating the real GTK3, Qt6, and Chromium/Ozone application paths on Wayland. It is not a chat transcript or a replacement for the migration status documents. Use it when a live probe appears to fail even though dictation works in ordinary applications.

## Scope

The validated path is:

```text
real desktop F9/F10 event
  -> retained Fcitx addon
  -> org.fcitx.Vinpst
  -> native streaming ASR
  -> RecognitionPartial signals
  -> real application widget commit or command replacement
```

The retained evidence profile is `sherpa-native-command-live`. Normal mode uses F9. Command mode uses F10 and the deterministic `native-command-live-adapter` so that selected-text provenance can be checked without depending on a remote provider.

Live recipes are intentionally outside `just ci`. A compiled probe is only implemented evidence. A successful JSONL summary from a real desktop key event is live evidence.

## Evidence model

A toolkit run is accepted only when it proves both sides of the boundary:

1. the daemon emitted at least one same-run non-empty `RecognitionPartial` value;
2. the real application widget observed the final text change.

Do not require client-side toolkit preedit as the only partial signal. Fcitx may render input-panel preedit without exposing it through GTK `preedit-changed` or a non-empty Qt `QInputMethodEvent::preeditString()`.

Command runs additionally require:

- `selection-ready` after the application field has real focus;
- the selected text to equal the expected fixture (`selected text` by default);
- the final adapter result to contain the expected selected text;
- surrounding-text runs to perform replacement rather than accept an unrelated primary-selection value.

Retained summaries are under:

```text
target/tmp/ime-gtk3-native-live/{normal,command}.jsonl
target/tmp/ime-qt6-native-live/{normal,command}.jsonl
target/tmp/ime-chromium-native-live/{normal,command}.jsonl
```

## Fast triage

Before changing code, check the session and installed profile:

```sh
fcitx5-remote --check
gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.GetStatus
niri msg windows
```

The daemon should normally report `idle`. Confirm that the probe window exists and has focus before deciding that the shortcut or addon failed.

Inspect the latest evidence rather than an older background job:

```sh
tail -n 30 target/tmp/ime-gtk3-native-live/normal.jsonl
tail -n 30 target/tmp/ime-qt6-native-live/normal.jsonl
tail -n 30 target/tmp/ime-chromium-native-live/normal.jsonl
```

A useful summary must name the toolkit and mode and end with `"ok":true`.

## Failure patterns and fixes

### Speaker playback is not microphone input

**Symptom:** the probe reaches `ready`, `pw-play` runs, but no partial arrives.

**Cause:** the configured microphone does not capture the desktop output. Playing a WAV through the speakers is not a reliable audio-injection mechanism.

**Action:**

- use real speech for an interactive application probe; or
- use `scripts/live/niri/run-ime-fcitx-virtual-source-live.sh` for repeatable injected-audio Fcitx-client evidence.

Never label a speaker-to-microphone pickup attempt as retained audio proof.

### Probe window is hidden or unfocused

**Symptom:** the JSONL contains `ready`, but F9/F10 is not consumed.

**Action:** locate and focus the window explicitly:

```sh
niri msg windows
niri msg action focus-window --id <window-id>
```

A timeout with no key or partial event is usually a focus/readiness failure, not an ASR failure.

### GTK object lifetime failure after a successful commit

**Symptom:** GTK receives final text, the window closes, and the timeout callback later reads a destroyed `GtkEntry` or marks the run as failed.

**Root cause:** GLib sources outlived the widget, and the final text was read directly from the widget after destruction.

**Fix used:**

- retain `last_text` independently of the widget;
- retain and remove timeout/finish/selection source IDs;
- clear widget pointers in the destroy callback;
- call `gtk_main_quit()` only while a GTK main loop exists;
- determine completion from retained state rather than dereferencing a destroyed object.

Regression commit: `5ce94b9 fix(toolkit): stabilize GTK3 live evidence`.

### GTK/Qt client-side preedit is empty

**Symptom:** the application receives the final text, but GTK emits no useful `preedit-changed` value or Qt emits only an empty preedit.

**Cause:** the retained addon updates the Fcitx input panel; every frontend toolkit does not necessarily surface that text as client-side preedit.

**Fix used:** subscribe to daemon `RecognitionPartial` for streaming evidence and use the real widget's `changed`/`textChanged` event for final application evidence.

Regression commits:

```text
5ce94b9 fix(toolkit): stabilize GTK3 live evidence
7d975bf fix(toolkit): stabilize Qt6 live evidence
```

### Stale primary selection creates a false command pass

**Symptom:** command replacement succeeds, but the adapter input contains text selected earlier in another application instead of the probe field's `selected text`.

**Cause:** the probe selected its text before the field had real desktop focus. Fcitx therefore had no valid surrounding selection and fell back to the existing primary selection.

**Fix used:**

- wait until the field reports real focus;
- select the fixture after focus;
- emit `selection-ready` only after reading the exact selected range back;
- optionally set `VINPST_TOOLKIT_EXPECTED_COMMIT_SUBSTRING`;
- require the final adapter result to contain the selected fixture.

Regression commits:

```text
34a0ef1 fix(toolkit): prove GTK surrounding selection
5f06766 fix(toolkit): prove Qt surrounding selection
```

This distinction is important: surrounding-text replacement and primary-selection fallback are separate live cases and must not satisfy each other's gate.

### First run after capture-target switching returns empty ASR text

**Symptom:** the virtual-source preflight contains valid non-zero audio, but the first command probe reports `sherpa-onnx online recognizer returned empty text`; an immediate rerun receives normal partials and commits.

**Interpretation:** distinguish the audio route from recognizer readiness. A successful preflight proves the virtual source itself, while an empty first recognition after daemon restart or capture-target switching can be a cold-start timing race. It is not evidence that primary-selection fallback failed when the client already reports `selection_source=primary`, `surrounding_text_provided=false`, and `delete_count=0`.

**Action:**

- confirm the daemon has returned to `idle` with the expected provider/model and capture target;
- confirm the preflight WAV has non-zero samples;
- rerun the same retained gate once;
- treat repeated empty recognitions as an audio/backend readiness bug and preserve both JSONL attempts.

The first 2026-07-30 primary-fallback attempt hit this symptom; the second attempt produced seven partials and a valid adapter commit. Do not hide the failed attempt by weakening the fallback assertions.

### English abbreviations become `<unk>`

**Symptom:** Chinese text is committed correctly, while terms such as `GTK` are emitted as `<unk>`.

**Cause:** the current Chinese Zipformer model's language/token coverage.

**Action:** treat this as a model-quality issue unless partials or final commits are missing. Use pure Chinese utterances for toolkit transport validation, and use a bilingual model for a separate multilingual-quality test.

### Persisted menu keys differ from source defaults

**Symptom:** a paging probe sends `Page_Down`, but the addon reports the key press as unconsumed even though the menu visibly has more than one page.

**Cause:** source defaults are not the live contract after Fcitx has persisted user configuration. The validated profile used:

```ini
[PagePrevKeys]
0=minus
[PageNextKeys]
0=equal
```

**Action:** read the active addon configuration under `~/.config/fcitx5/conf/vinpst.conf` (or `VINPST_LIVE_FCITX_ADDON_CONFIG`) and use the configured `PagePrevKeys`/`PageNextKeys`. Do not hard-code `PageUp`/`PageDown` in a retained gate.

Failure signatures are useful:

- wrong key: the page-key press is not consumed;
- correct configured key with the old addon: the press is consumed, but the client receives an empty input panel instead of page 2.

### Client-side input-panel candidate state is transient

**Symptom:** the first page renders as `Scenes /filter (1/2)`, the configured next-page key is consumed, and the following client-side UI update contains an empty title and no candidates.

**Root cause:** the addon stored no page number of its own and attempted to recover the current page from `InputPanel::candidateList()` during the next key event. With client-side input-panel delivery, that pointer is not a durable state store and may already be absent.

**Fix used:**

- store `scene_menu_page_` and `asr_menu_page_` in the addon;
- rebuild and republish a fresh candidate list for the requested page;
- clamp the page through `CommonCandidateList::setPage`;
- use the retained page for digit selection and Enter fallback;
- reset the retained page when the menu closes.

The real scene gate temporarily added 12 inert scenes, used the configured `equal`/`minus` keys, and proved `1/2 -> 2/2 -> 1/2`, four candidates on page 2, zero commits, Escape close, unchanged active scene, and byte-for-byte profile/backup restoration. Evidence is under `target/tmp/ime-fcitx-menu-paging-live`. The same page-state implementation is shared by the ASR menu, while the retained live paging evidence currently covers the Scene menu.

Regression commit: `80d2dc5 fix(fcitx): preserve menu page state`.

### Installed daemon fails only when run directly

**Symptom:** invoking `~/.local/bin/vinpst-daemon` reports an ONNX Runtime symbol or version error, while D-Bus activation works.

**Cause:** direct execution bypasses the generated runtime-library environment. The installed daemon is intended to start through `~/.local/share/fcitx-vinpst/vinpst-daemon-with-vinpst-env.sh`, which sources `fcitx-vinpst.env` before loading `libsherpa-onnx` and `libonnxruntime`.

**Action:** use the generated wrapper for standalone diagnostics and compare its environment with the activation service before treating the model as broken.

### A copied local provider id is not another backend instance

**Symptom:** a temporary provider with `type: local` and an id such as `sherpa-onnx-live-alt` appears in the F8 menu but reload fails with “provider ... is not implemented yet”.

**Cause:** the local provider id is also the runtime implementation selector; `type: local` alone does not route an arbitrary id to sherpa-onnx.

**Action:** use `scripts/live/niri/run-ime-fcitx-model-switch-live.sh` for another model under the existing `sherpa-onnx` provider, `scripts/live/niri/run-ime-fcitx-cross-provider-live.sh` for the separate internal-to-command-provider contract that intentionally reuses the original sherpa model, and `scripts/live/niri/run-ime-fcitx-whisper-provider-live.sh` when independent recognizer/model proof is required. The Whisper gate pins source/model hashes, traces the external process, checks temporary WAV cleanup, and restores Zipformer streaming recognition. Use `scripts/live/niri/run-ime-fcitx-cross-provider-failure-live.sh` for the complementary prepare-failure contract: one unique remote candidate with an unsupported endpoint scheme is selected directly, the prior effective backend must remain intact, daemon/Fcitx error notifications must match, and the restored backend must produce partials and a final commit. Use `scripts/live/niri/run-ime-fcitx-remote-asr-live.sh` for successful remote-protocol proof: it validates multipart WAV/Bearer/model/language/prompt transport, redacted trace evidence, a final-only application commit, and Zipformer restoration against an independent loopback endpoint.

### D-Bus activation ignores a relative model root

**Symptom:** the daemon command line contains `--model-root target/tmp/...`, but `GetAsrDisplayMenuState` exposes no installed target.

**Cause:** a D-Bus activated process does not inherit the repository working directory. Relative model roots resolve from an unrelated directory.

**Action:** write an absolute model root into the temporary activation service and verify the exact path in `vinpst daemon status` before opening F8.

### Offline ASR commits without streaming partials

**Symptom:** Paraformer commits valid final text, but a generic streaming gate fails with “no non-placeholder partial preedit”.

**Cause:** offline recognizers do not promise `RecognitionPartial` output. Requiring partials for every model conflates runtime class with frontend correctness.

**Action:** retain `require_partial=true` by default, opt out only for a known offline model, and still require one non-empty final commit. Owner-loss and streaming-model cases must continue to require partial evidence. The reusable live gate records this distinction in its summary.

Regression commit: `876ca2e test(e2e): support offline ASR live probes`.

### Menu filters collide with persisted paging keys or survive cancellation

**Symptom:** hyphens disappear from an F8 filter query, or a new probe starts with an old query already present.

**Cause:** persisted `minus`/`equal` paging keys are handled before printable filter input, and canceling a probe can leave addon menu state alive in the current Fcitx process.

**Action:** avoid configured paging keys in a retained filter fixture, prefer a model root that exposes exactly one target, and restart/verify Fcitx before an isolated selection gate.

### Switching back through the menu API is rejected after a temporary model root

**Symptom:** after F8 persisted Paraformer, `SetActiveAsrTarget` rejects the original Zipformer as not configured or installed.

**Cause:** the temporary model root intentionally exposes only Paraformer, and the persisted profile no longer names Zipformer. The API correctly refuses a target outside both sources.

**Action:** restore the exact original profile bytes and backup state, call the normal reload path, and wait until target and effective provider/model both match the original. Then prove another recognition. Do not weaken target validation for test cleanup.

The retained roundtrip is `scripts/live/niri/run-ime-fcitx-model-switch-live.sh`; evidence is under `target/tmp/ime-fcitx-model-switch-live`. It proves F8/Enter selection, an offline Paraformer commit, restoration to Zipformer with streaming partials, and exact service/profile/Fcitx/backend recovery. The non-selecting paging companion is `scripts/live/niri/run-ime-fcitx-asr-menu-paging-live.sh`; it exposes 14 uniquely titled metadata copies while hard-linking immutable model assets, then verifies configured paging keys and exact restoration without reloading a model. Persisted trigger timing is isolated by `scripts/live/niri/run-ime-fcitx-trigger-modes-live.sh`, which swaps only the addon mode and activation audio backend, uses real Fcitx events, and restores both files exactly. Installed-catalog localization is isolated by `scripts/live/niri/run-ime-fcitx-localization-live.sh`: the user addon embeds `XDG_DATA_HOME/locale`, disables its build-tree fallback, verifies zh_CN Scene/ASR panels through real Fcitx events, exits Fcitx before exact config comparison to avoid delayed-save races, and restores an English session. `scripts/live/niri/run-ime-fcitx-notification-localization-live.sh` separately proves zh_CN scene-information text and information/error summaries, keeps the daemon's technical error body verbatim, waits for each restarted Fcitx process to settle, and restores the original locale plus all mutable files. `scripts/live/niri/run-ime-fcitx-asr-notification-localization-live.sh` proves the remaining `已请求切换语音识别到“...”` template through real F8/Enter events; both the shared child and wrapper stop Fcitx before restoring byte-exact addon configuration so localized comment writeback cannot escape the gate.

### Concurrent work contaminates a small commit

**Symptom:** `git status` contains unrelated live-test or documentation changes while a focused fix is being prepared.

**Action:**

- stage only explicit paths;
- use an index-only patch generated from `git show HEAD:<path>` when one file contains unrelated hunks;
- inspect `git diff --cached --stat` and `git diff --cached` before committing;
- never reset or discard another worker's uncommitted changes.

## Real application recipes

Run with a real desktop key event:

```sh
scripts/live/niri/run-ime-gtk3-native-live.sh normal
scripts/live/niri/run-ime-gtk3-native-live.sh command
scripts/live/niri/run-ime-qt6-native-live.sh normal
scripts/live/niri/run-ime-qt6-native-live.sh command
scripts/live/niri/run-ime-chromium-native-live.sh normal
scripts/live/niri/run-ime-chromium-native-live.sh command
```

For a strict command provenance check:

```sh
VINPST_TOOLKIT_EXPECTED_COMMIT_SUBSTRING='selected text' \
  scripts/live/niri/run-ime-gtk3-native-live.sh command
VINPST_TOOLKIT_EXPECTED_COMMIT_SUBSTRING='selected text' \
  scripts/live/niri/run-ime-qt6-native-live.sh command
```

Chromium command mode applies the same selected-text assertion automatically.

## Expected JSONL sequence

Normal mode should resemble:

```json
{"event":"ready","toolkit":"gtk3","mode":"normal","manual_trigger":true}
{"event":"daemon-partial","text":"你好"}
{"event":"changed","text":"你好"}
{"event":"summary","toolkit":"gtk3","mode":"normal","partial":true,"commit":true,"ok":true}
```

Command mode should additionally include:

```json
{"event":"selection-ready","text":"selected text"}
{"event":"daemon-partial","text":"改得更简洁"}
{"event":"changed","text":"adapter-backed: selected text | command: 改得更简洁"}
{"event":"summary","selection_ready":true,"expected_commit":true,"replacement":true,"ok":true}
```

Exact recognized words are not the toolkit contract. The required contract is same-run partial evidence, one final application change, and correct selected-text provenance.

## Validated result set

The 2026-07-30 real-key validation produced `ok: true` for:

- GTK3 normal and command;
- Qt6 normal and command;
- Chromium/Ozone normal and command.

Chromium support was added by `7f270fc test(toolkit): add Chromium live probe`. The retained evidence was recorded by `6d24f39 docs(migration): record toolkit live evidence`.

## Extending the matrix

When adding another application or failure mode:

1. emit `ready` only after the target input is focused and prepared;
2. never synthesize the shortcut inside the toolkit;
3. isolate same-run daemon signals from unrelated sessions;
4. assert the final text in the real application surface;
5. keep surrounding-text and primary-selection fallback as distinct cases;
6. write JSONL to a stable `target/tmp` directory;
7. add a deterministic architecture/contract test for the probe;
8. run the narrow validation tier, then the full relevant gate before handoff.
### External text provider boundary

Use `scripts/live/niri/run-ime-fcitx-external-text-provider-live.sh` to prove the configured F10 path against a loopback OpenAI-compatible endpoint, including real surrounding-text replacement and restoration of the normal command adapter afterwards. `scripts/tests/asr/run-openai-compatible-text-provider-fixture-smoke.sh` is the small deterministic CLI boundary: it starts one fixture, runs production `vinpst llm test`, checks the authenticated `/v1/chat/completions` request and result, and verifies the API key is absent from retained evidence. Shared transport behavior such as proxy routing/authentication, custom CA trust, TLS proxying/interception, redirect refusal, response bounds, and network failures is tested once through `vinpst-http`/text unit tests plus the production remote-ASR network smoke instead of duplicating a second large text-provider shell harness. `scripts/tests/asr/run-provider-ca-rotation-smoke.sh` remains the same-daemon CA replacement proof for both remote ASR and provider-backed text processing. Real hosted credentials, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage policy, production CA distribution/revocation, provider credential custody/rotation, and cross-application disconnect recovery remain separate work.
