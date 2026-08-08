# E2E capability matrix

Reviewed: 2026-08-07

This matrix describes user-visible parity and the evidence level for each path. Status labels are:

- **implemented**: code and focused tests exist;
- **deterministic**: integration is proven without a real desktop or microphone;
- **live-proven**: verified in a real desktop session;
- **partial**: important behavior is missing;
- **missing**: no implementation exists.

## Evidence baseline

- Rust implementation reviewed through `c507807`, including the packaged GUI resource lifecycle, the checked Arch release pipeline, Debian 12/Ubuntu 24.04 Docker package transactions, locked Nix build, RPM-family and Flatpak transaction baselines, and hardened long-lived adapter process supervision.
- Legacy reference is `/workspace/fcitx5-vinput`.
- `cargo test --workspace --all-targets`, D-Bus integration, retained-addon tests, and the complete deterministic `just ci` gate pass through `c507807` with synchronized GUI status documentation; the last pushed complete remote Rust/Nix/Debian/Ubuntu matrix is `9d31f70`.
- Native registry models are validated by model-specific local WAV smokes.
- `sherpa-native-live` installation is validated in temporary HOME environments with a copied `libsherpa-onnx` and `libonnxruntime` bundle, wrapper activation through `vinpst-daemon-with-vinpst-env.sh`, and `scripts/tests/install/run-user-ime-sherpa-native-activation-smoke.sh`. `sherpa-native-command-live` adds a checked local command adapter and has its own temporary-HOME install smoke.

## User journeys

| Journey | State | Evidence | Remaining work |
| --- | --- | --- | --- |
| First-run initialization | implemented | `vinpst init`, managed directories, config validation, dry-run/JSON tests | Install guide polish |
| Discover and install a model | implemented | live registry list/info/install, SHA-256, safe extraction, atomic materialization | Update/packaging polish |
| Discover, install, update, edit, and remove an ASR provider | implemented | current `registry/providers.json`, short ids, localized title/description, batch/streaming validation, mirror download, executable publication, update-by-reinstall with legacy timeout/env preservation, config backup, guarded managed update; local removal guard, active-clear semantics, and legacy-compatible referenced-script editor | None for current script registry |
| Discover, install, update, remove, and control an adapter | implemented | current `registry/adapters.json`, short ids, localized title/description, mirror download, executable publication, update-by-reinstall with config backup and guarded managed update; short-id removal and in-place managed-script cleanup without deleting user-defined files; installed-selector validation before start/stop/status D-Bus calls | None for current script registry |
| Select and reload a model/provider | live-proven within `sherpa-onnx`, across command/Whisper/remote boundaries, and for remote prepare failure | real F8/Enter selection switches streaming Zipformer to offline Paraformer, compatibility command and independent Whisper providers, and `remote-http/fixture-remote-asr`; the remote success gate proves multipart WAV/Bearer/model/language/prompt transport plus a final-only application commit, the invalid-scheme gate preserves Zipformer, and deterministic one-shot daemon cases prove plain-HTTP proxy routing, Basic authentication for direct HTTP over plain-HTTP proxies and CONNECT through both plain-HTTP and TLS-protected HTTPS proxy endpoints, `NO_PROXY`, additional PEM roots through `SSL_CERT_FILE`, retained built-in `WebPKI` verification, one local CA-signed TLS interception relay with no retained plaintext, 429/503, fail-closed 3xx handling with an untouched redirect target, distinct request and response-body timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection refusal, and secret redaction; a separate persistent-daemon gate proves atomic replacement of one CA-file path with mismatch rejection and idle recovery under unchanged ASR/text owner PIDs and endpoints; every live gate restores service/profile/backup/Fcitx/backend state exactly | Real hosted-provider operations, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage/rate-limit behavior, and provider credential lifecycle and production CA distribution/revocation operations |
| Normal native dictation | live-proven through isolated injection, the default physical microphone, and real applications | real Fcitx client, F9, a preflight-verified virtual PipeWire source, the default physical ALSA Digital Microphone without playback injection, streaming partials, final commits, three same-window/same-daemon GTK4 normal cycles, and real-key GTK3, GTK4, Qt6, sandbox-attested Chromium/Ozone and VS Code/Electron, GNOME Text Editor and VS Code saved-file evidence, and kitty terminal-output evidence | Additional physical-device switching breadth |
| Command native dictation | live-proven for surrounding text, an external HTTP process, primary selection, and the double-empty rejection boundary | real Fcitx client, F10, live partials, local `adapter-backed:` commits, a loopback OpenAI-compatible provider request carrying selected/ASR text, HTTP-candidate selection and deletion/replacement, zero-delete Wayland primary-selection fallback, exact `Please select text first.` rejection before recording when surrounding and primary selections are empty, plus GTK3, GTK4, Qt6, Chromium, GNOME Text Editor, kitty, and VS Code/Electron command paths and three same-window/same-daemon GTK4 command cycles; kitty, Chromium, and VS Code prove PRIMARY-selection fallback, while Chromium and VS Code use distinct application-selection/PRIMARY sentinels and restore the current-run PRIMARY bytes | Cross-application breadth and real cloud-provider behavior |
| Scene, ASR, configuration, and notification localization | legacy locale parity complete for English fallback plus zh_CN; selection/paging, cross-provider selection/failure preservation, zh_CN menu/configuration, and zh_CN scene-info/ASR-switch/error-summary notifications live-proven | real Fcitx clients prove F7/F8 display/filter/Escape, F7 Enter scene selection, F8 Enter same-provider and external-provider selection, unavailable-remote reload failure with old-backend preservation, configured-key scene paging, a 14-target ASR menu across `1/2 -> 2/2 -> 1/2`, installed-catalog `场景 /过滤` / `模型 /过滤` plus `当前：` status text, official configuration-form English/zh_CN labels and `Tap/Hold/Both` / `单击/长按/两者` choices without saving, `语音输入` summaries, `已切换场景到“Command”。`, `已请求切换语音识别到“remote-failure-fixture”。`, verbatim daemon error bodies, old-backend preservation plus nine recovered partials, and English/original-locale restoration; all gates reject unintended commits and restore profile/service/Fcitx/backend state exactly | Additional UI locales are optional expansion beyond legacy parity |
| Daemon lifecycle | implemented | direct per-user activation, systemd-backed activation, default user-config discovery with persistent D-Bus updates, status, reload, stop/restart/log plans, owner diagnostics, guarded old-systemd `daemon-reload`/restart, guarded idle same-user old-direct termination/reactivation, private-session direct replacement proof, real user-systemd replacement proof with changed `MainPID` and incremented `NRestarts`, guarded no-owner/systemd/direct removal preparation with active-session refusal, deterministic Flatpak host routing, required-permission diagnostics, atomic native/Flatpak user-service rendering/install/reload, and a checked Flatpak extension package transaction | Live Flatpak host-systemd/PipeWire lifecycle and actual package-installed multi-user proof |
| Recording control | implemented | start/stop/toggle/status D-Bus paths | Live error handling |
| Device selection | live-proven for isolated PipeWire sources | typed `GetCaptureDevice`/`SetCaptureDevice`, one daemon PID and recorder instance, two real source streams with target rebuilds, atomic persistence, and exact profile restoration | Additional physical-device switching breadth |
| Diagnose and recover | implemented | `doctor`, runtime status, owner/PID/procfs, activation and live probe, plus Flatpak `pipewire`/systemd-config/cache permission reporting | Message refinement from live failures |
| Provider-backed text processing | live-proven for both the local command adapter and an independent loopback OpenAI-compatible HTTP process, with deterministic production-client network semantics | local text helpers receive the effective scene timeout, while text and command-ASR helpers share whole-process-group termination, direct-child descendant cleanup, deadlock-free stream collection, and independent 1 MiB stdout/stderr limits; the HTTP gate proves 404 preservation, Bearer/JSON request shape, selected/raw ASR transport, exact candidate replacement, no-selection rejection and restoration; `vinpst llm test` additionally proves plain-HTTP proxy routing, Basic authentication for direct HTTP over plain-HTTP proxies and CONNECT through both plain-HTTP and TLS-protected HTTPS proxy endpoints, `NO_PROXY`, additional PEM roots through `SSL_CERT_FILE`, retained built-in `WebPKI` verification, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, 429/503 bodies, fail-closed 3xx handling with an untouched redirect target, distinct request and response-body timeout diagnostics, the legacy 4000 ms default for omitted text-scene timeouts, a 1 MiB cap for success and error response bodies, untrusted self-signed TLS rejection, DNS failure, connection refusal, and credential redaction | Real hosted-provider credentials, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, provider-specific outage/rate-limit behavior, provider credential lifecycle and production CA distribution/revocation operations, and cross-application cloud recovery |
| User installation | deterministic | temporary-HOME activation/runtime recognition, the checked external-user Arch lifecycle guide and isolated command smoke, shared checked native-runtime selection, the Arch package/repository/signature/candidate pipeline, Debian 12 and Ubuntu 24.04 Docker install/upgrade/remove transactions, the locked Nix closure build, automatic ownership-verified cross-user dispatch, guarded removal rollback, unsupported-schema preservation, and a checked x86_64 Flatpak extension build/install/update/remove/bundle-reinstall transaction whose published bundle is pinned to revision 1. The RPM-family baseline builds two releases and proves isolated install/upgrade/verify/removal | Actual host package-installed upgrade, live production multi-user and Flatpak desktop lifecycle, supported Fedora/openSUSE builds plus DNF/Zypper/signing/SELinux/live-scriptlet validation, production repository/key/cache operations, Flathub/signing policy, and unrelated-machine regression |
| Standalone GUI | packaged interactive management baseline | Rust/Iced keeps the upstream four-page task split while retaining Rust safety contracts: **Control** = PipeWire capture selection, normalization/input gain/VAD/output ducking, configured ASR providers, then daemon lifecycle; **Resources** = browsable localized model/provider/adapter catalogs and install/update only; **LLM** = LLM providers, configured adapters, and scenes; **Hotwords** = provider/path/content management. Ordinary pages no longer expose owner-monitor state, direct recording test actions, config paths, registry ids, or low-level VAD thresholds. Provider/adapter catalog rows reuse the existing secure transactional install/recovery pipeline, and capture-device selection reuses `vinpst-audio` PipeWire enumeration while preserving an already-configured missing device. Typed config persistence, D-Bus/runtime guards, secret redaction, model/script cancellation/recovery, managed removal, startup notifications, localization, and deterministic semantic tests remain as documented in [`../architecture/gui-contract.md`](../architecture/gui-contract.md). | Management-GUI interaction/visual acceptance remains manual-only; automated evidence stays below the Iced window/widget boundary. Screen-reader semantic-tree support remains post-0.1.0. |

## CLI command surface comparison

The comparison is now based on the compiled upstream CLI, not only source inspection. `/workspace/fcitx5-vinput` was built at `6cdcac8b4300ff347ad3157bf61cd09a5302f7a9` (`v2.3.5-1-g6cdcac8`) with the `debug-clang-mold` preset and the `vinput` target. Its root help, every public subcommand help surface, and representative text output were inspected under an isolated temporary HOME/XDG profile.

Upstream exposes the public root groups `init`, `daemon`, `recording`, `config`, `model`, `provider`, `adapter`, `device`, `hotword`, `scene`, and `llm`. Rust exposes the same user groups plus `doctor`. Protocol dumps, registry validators, package-lifecycle helpers, fixture roots, local registry/i18n injection, transport-plan dry runs, and other test-oriented entry points remain callable where project tooling needs them but are hidden from ordinary help.

Familiar upstream spellings remain accepted without expanding the normal help surface, including `model ls/add/rm`, `provider ls/add/edit/e/rm`, `adapter ls/add`, `device ls`, `hotword e`, `scene ls/e/rm`, `config e`, and `llm ls/e/rm`. Managed-provider and managed-adapter installation therefore accept the upstream `add` spelling, and `provider edit/e` keeps the upstream meaning of editing the installed provider script. Rust-specific typed custom resources use unambiguous `provider create`, `provider configure`, and `adapter create` commands instead.

Intentional command-shape differences now add capabilities without changing the meaning of familiar upstream commands:

| Upstream C++ | Rust CLI | Reason |
| --- | --- | --- |
| `model add` | `model install` with `add` alias | Make managed installation explicit while retaining the familiar command. |
| `provider add` installs a registry provider | `provider install` with `add` alias; `provider create` creates a custom typed entry | Preserve upstream semantics and keep custom creation explicit. |
| `provider edit/e` edits the installed script | `provider edit/e` with `edit-script` compatibility alias; `provider configure` edits typed config | Preserve upstream semantics while separating executable-file editing from typed configuration. |
| `adapter add` installs a registry adapter | `adapter install` with `add` alias; `adapter create` creates a custom typed entry | Preserve upstream semantics while keeping custom adapters available. |
| no combined diagnostic command | `doctor` | Give users one setup/readiness entry point. |
| human text only | concise human text plus `--json` | Human output stays at the resource/action level; exact paths, transport plans, counters, and maintenance state remain machine-readable. |

The default Rust text surface now follows the upstream abstraction level: resource lists show identifiers, names/types, relevant model metadata, and active/installed/available state rather than config sources, command/environment contents, working directories, fixture paths, timeout internals, or struct-field names. Mutation commands report the action performed or previewed instead of serializing `dry_run`, before/after counters, write flags, or transport details. Exact internal state remains covered by JSON integration tests.

There is no remaining CLI command-group capability gap for the ordinary management workflow. Remaining work is concrete message refinement, non-systemd behavior where applicable, release/artifact-installed evidence, and further implementation extraction when a composition file becomes too large; it is not a reason to expand the public help surface.

## Shipped binary comparison

The compiled-upstream audit now covers all three shipped executable roles, not only the CLI. At upstream commit `6cdcac8b4300ff347ad3157bf61cd09a5302f7a9`, the `debug-clang-mold` preset successfully built `vinput`, `vinput-daemon`, and `vinput-gui`; the Rust workspace ships the corresponding `vinpst`, `vinpst-daemon`, and `vinpst-gui` binaries. The upstream CLI was executed directly. The upstream daemon target linked successfully but the current host cannot enter `main()` because the installed `libsherpa-onnx-c-api.so` requires an unavailable `libonnxruntime` symbol version, so daemon argv/default/lifecycle comparison combines the compiled target with its exact `main.cpp`, runtime-controller, recognition-manager, systemd, and D-Bus service sources. The upstream GUI was deliberately not launched because it has no application-specific argv parser before constructing/showing `QApplication`, and management-GUI interaction is manual-only; its compiled target, `main.cpp`, and desktop entry were inspected instead. Rust daemon behavior was additionally exercised on isolated real session buses, while Rust GUI checks stayed on the pre-window `--help`/`--check` paths.

| Executable boundary | Upstream C++ | Rust/Vinpst | Decision/evidence |
| --- | --- | --- | --- |
| Daemon public argv | `vinput-daemon` has one application switch, `--no-asr`; ordinary execution is the service path | normal help exposes `--no-asr`, help, and version; file/mock/configured-backend/package diagnostics remain callable but hidden | Preserve the useful Rust help/version enhancement without advertising test/package plumbing. |
| Daemon direct start | no-argument execution opens the user D-Bus service and normal configured audio/ASR runtime | no-argument execution now enters D-Bus configured-backend service mode; default CMake activation uses that direct path, while release packages explicitly pin configured backends and PipeWire | Private-bus process evidence proves direct ownership; explicit test modes retain deterministic mock defaults. |
| `--no-asr` | stored as a recognition-manager lifetime policy; later backend synchronization remains disabled | stored as a `RuntimeState` lifetime policy; reload requests keep ASR unavailable with `ASR disabled by command line.` | Unit plus real D-Bus reload evidence prove ASR cannot be re-enabled accidentally. |
| Service-name replacement | upstream requests the D-Bus name with replacement allowed | Vinpst requests `DoNotQueue` only | Intentional safety divergence: a second daemon fails immediately and cannot bypass guarded package handoff. Real session-bus integration pins `NameTaken` and unchanged ownership. |
| GUI argv | no application-specific CLI before window creation | public `--page` deep-link plus help/version; hidden `--check`, `--offline`, and `--config` package diagnostics | Extra Rust capability is retained; hidden `--offline`/`--config` require `--check`, preventing a mistaken diagnostic invocation from silently opening a window. |
| GUI input-method startup | Qt fills absent `QT_IM_MODULE=fcitx` and `XMODIFIERS=@im=fcitx` before `QApplication` | Iced/winit uses native Wayland text-input and X11 XIM discovery; no unsafe process-environment mutation is added | Toolkit-specific difference. Chinese preedit/commit on Wayland and X11/Xwayland remains an explicit human GUI-acceptance item. |
| GUI desktop entry | `Exec=vinput-gui`, matching icon, `Utility;`, English metadata | matching Vinpst executable/icon plus `Settings;Utility;`, zh_CN metadata, and `StartupNotify=true` | Intentional desktop-integration enhancement; executable launch semantics remain one-to-one. |
| Config missing | bundled default | bundled default | Same behavior. |
| Existing malformed config | compiled upstream CLI logs the parse failure but exits successfully with partially defaulted state | parse fails and the command exits non-zero | Intentional fail-closed data-safety divergence. |
| Future config schema | compiled upstream CLI accepts an unknown `version` | versions newer than the supported schema are rejected | Intentional fail-closed protection against old binaries reading/rewriting newer formats. |
| Built-in command prompt | current normalization upgrades missing/legacy/short-tag prompts to scoped `<vinput-selected>`/`<vinput-asr>` interpolation | bundled default and normalization now use the same scoped interpolation contract | Config and OpenAI-request tests prove selected/ASR inputs are each interpolated exactly once. |
| XDG/storage layout | config/data/cache under product root `vinput`; managed `models/providers/adapters`; adapter runtime under `$XDG_RUNTIME_DIR/vinput/adapters` with temp fallback | equivalent layout under independent roots `fcitx-vinpst` and runtime `vinpst` | Product-root names intentionally differ; storage/runtime layering remains equivalent. |
| Legacy D-Bus ABI | eight service methods, four signals, five lowercase status strings | same legacy method/signal signatures and status strings under the independent Vinpst bus/interface identity, plus documented Rust extensions | Live `busctl introspect` pins `GetAsrBackendState` as `sssssbbas` and the other seven legacy method/signals byte-for-byte by signature. |

The audit was intentionally iterative: public argv/help, same-name/default semantics, package/service activation, XDG/runtime paths, live D-Bus ABI, GUI startup/toolkit behavior, default-config normalization, malformed/future-config handling, and status/error transitions were checked as separate rounds. Differences are not automatically treated as gaps: independent product identity, strict config validation, guarded daemon handoff, JSON diagnostics, and Rust-only management extensions remain when they improve safety or usability without changing the meaning of a familiar upstream operation.

## Daemon capability comparison

| Capability | State | Notes |
| --- | --- | --- |
| Legacy bus/interface/path | implemented | `org.fcitx.Vinpst`, `/org/fcitx/Vinpst`, `org.fcitx.Vinpst.Service` |
| Core methods and signals | implemented | legacy methods, `RecognitionResult`, `RecognitionPartial`, `StatusChanged`, notification signal |
| Diagnostic extensions | implemented | runtime, adapter, scene, and ASR menu state; shared provider-URL diagnostics remove userinfo/fragments and redact query values, ASR/text `Debug` hides prompt/body contents, and known echoed credentials are replaced without mutating requests |
| Runtime state machine | deterministic | normal/command lifecycle, capture-before-session startup, early-chunk gating, chunk delivery, partials, explicit inferring/postprocessing phases, final result, error cleanup |
| ASR reload | deterministic | unavailable-but-running configured startup, one non-blocking prepare-before-swap worker, config reread, generation coalescing, old-backend preservation |
| Audio capture | partial | deterministic lifecycle, live typed same-daemon and same-recorder target switching across two isolated PipeWire sources, live capture from a preflight-verified virtual source, default physical ALSA Digital Microphone recognition through native ASR, and real `wpctl` duck/restore against an isolated virtual sink are proven; audible hardware-output ducking and broader physical-device combinations remain |
| File input | implemented | WAV and PCM paths are first-class deterministic seams |
| Command ASR | implemented | batch/streaming/JSON protocols, partials, configured deadlines across stdin/execution/output recovery, whole-process-group cancellation, direct-child descendant cleanup, deadlock-free pipe draining, and independent 1 MiB stdout/stderr limits; omitted `timeout_ms` remains explicitly unconfigured |
| Native offline ASR | deterministic | supported registry families pass real WAV smokes |
| Native online ASR | deterministic | online transducer and Zipformer2 CTC, 200 ms warmup, partial-before-stop |
| Offline VAD | deterministic | tracked Silero model, legacy controls, fallback and diagnostics |
| Text postprocess | live-proven for local adapter and loopback OpenAI-compatible provider | deterministic command/OpenAI paths plus real F10 HTTP request, candidate selection, deletion, commit, and restoration; third-party cloud behavior remains |
| Adapter supervision | deterministic | runtime-directory creation before spawn; fingerprinted mode-0600 PID records; fail-closed legacy PID cleanup; duplicate/stale start handling; whole-process-group TERM/KILL escalation; zombie-aware Linux cleanup that still waits for live descendants; failure-path worker cleanup; and D-Bus control |
| Notifications and recovery | live-proven for retained local cases | focus handoff keeps partials/final commit on the originating context; verified daemon loss surfaces an unavailable preedit with zero commit; information notifications are observed from the current Fcitx PID; daemon reload failure produces a matching 5-second error notification while preserving the old backend; same-provider reload and model switching are followed by successful recognition | Broader notification categories and cross-provider recovery |
| Remote text service | partial | active-provider settings, API-key/loopback policy, single input/output ownership, debounce/finalize transitions, OpenAI Realtime-compatible event shapes, Axum `/health`/browser/`/ws`/`/v1/realtime` runtime, standalone diagnostics command, normal D-Bus daemon startup/provider-selection/reload ownership, bind-failure cleanup, `SIGTERM` shutdown, redacted LAN endpoint diagnostics, local-socket tests, private-session process smoke, a real sandboxed Chromium same-host LAN page/WebSocket path, and a fail-closed external-device challenge collector requiring explicit physical-device confirmation | Successful collector proof from another physical network device |

## Registry/resource comparison

### Models

Model workflow is implemented:

- parse live registry metadata and i18n;
- resolve full ids and short ids;
- fetch with mirror fallback;
- verify declared SHA-256;
- reject unsafe archive entries;
- stage and atomically materialize across filesystems;
- persist typed `vinpst-model.json` and display metadata;
- discover flat Rust and legacy engine/model layouts;
- inspect, select, reload, and safely remove managed models.

### Providers and adapters

Current adapter script installation is implemented:

- parse the upstream `registry/adapters.json` shape and resolve full or short ids;
- derive the same managed relative paths as legacy;
- try ordered script mirrors and publish an executable file;
- add blank values for declared environment keys while preserving existing values;
- write config through output/in-place/backup policy;
- update only adapters already bound to the expected managed script and refuse user-defined replacements;
- keep dry-run free of script and config writes.

Current model, provider, and adapter lists resolve localized titles/descriptions from the shared root-level registry i18n map while retaining stable machine ids and short selectors. Localization detects and normalizes the process locale with legacy environment priority, then merges `en_US`, the requested locale, and the automatic user `vinpst/i18n.local.json` override in increasing priority; unavailable locale/local layers remain nonfatal and visible in diagnostics. Reinstalling an existing managed entry is the registry update operation: provider timeout/model/environment values and adapter environment/forward-compatible fields are preserved while the executable is replaced through the guarded publication path. Upstream-compatible `provider add` and `adapter add` route to those managed install paths, while custom typed entries use `provider create` and `adapter create`. Provider removal also matches legacy: local providers are protected, active non-local removal clears the active selection, and registry short ids can be resolved from an explicit catalog. `provider edit`/`provider e` resolves an exact installed id or explicit registry short id, rejects non-command providers, locates the first existing regular file referenced by the command or its arguments, and launches the selected editor without mutating config; `provider edit-script` remains accepted as an explicit compatibility spelling, while typed provider mutation uses `provider configure`. Adapter removal resolves explicit registry short ids, removes configuration through the normal backup policy, deletes a script only when its sole configured argument exactly matches the expected managed-root path, and preserves scripts for `--output` or user-defined adapters. Adapter start, stop, and filtered status resolve exact installed ids directly or explicit registry short ids, reject selectors that are not installed, and pass only the resolved machine id to D-Bus.

## Native runtime coverage

| Family/path | Evidence |
| --- | --- |
| SenseVoice | real registry-model WAV smoke |
| Qwen3 ASR | real registry-model WAV smoke |
| Paraformer | real registry-model WAV smoke |
| Dolphin | real registry-model WAV smoke |
| Moonshine v1 | local WAV and D-Bus reload smoke |
| Offline transducer | real registry-model WAV smoke |
| Online transducer | real registry-model WAV smoke and activation/addon path |
| Zipformer2 CTC | real registry-model WAV smoke |
| Command batch/streaming | deterministic process protocol tests and user profile smokes |

### P1.2 sherpa streaming backend

Implemented through D-Bus and the retained frontend:

- recorder callbacks use legacy-compatible 800-frame batches;
- online hypotheses produce deduplicated `RecognitionPartial` signals;
- stop cancels the generation poller and preserves final/completed events;
- activation-safe owner tracking accepts signals only from the current daemon owner;
- partial text reaches concrete Fcitx preedit before stop;
- final commit remains the synchronous stop outcome.

The remaining streaming gap is live PipeWire behavior in a real application, not the deterministic backend path.

## Frontend capability

Implemented and deterministically tested, with normal/command outcome application additionally live-proven in a real Fcitx client input context:

- normal, command, scene-menu, ASR-menu, previous-page, and next-page persistent KeyLists;
- Tap/Hold/Both trigger mode with legacy timing;
- scene and installed-model-aware ASR menus;
- keyboard, paging, digit, enter, escape, mouse, and slash-filter behavior;
- UTF-8 editing and multi-term search;
- zh_CN gettext catalog, localized scene-info/error summaries, and English fallback;
- localized installed-model titles with stable-id fallback;
- Fcitx notifications and stderr fallback;
- daemon signal monitoring, owner-loss recovery, and external-session reconciliation;
- selected-text replacement plus primary-selection clipboard fallback.

The installed `sherpa-native-command-live` profile now has retained normal/command evidence for GTK3, GTK4, Qt6, Chromium/Ozone, GNOME Text Editor, kitty, and VS Code/Electron, including ten consecutive GTK4 normal cycles and ten consecutive GTK4 command cycles in one window and one daemon owner; Chromium and VS Code additionally have explicit renderer-sandbox evidence (`NoNewPrivs=1`, seccomp filter mode, zero effective capabilities, and a nested PID namespace). Chromium also proves no browser sandbox-disable flag, while the VS Code isolated process set rejects all supported sandbox-disable flags. The profile also retains real Fcitx-client evidence for default physical-microphone dictation, local-adapter and loopback OpenAI-compatible HTTP-provider surrounding-text replacement, Wayland primary-selection fallback, scene selection, configured-key scene and ASR paging, installed-catalog zh_CN Scene/ASR titles/status and scene-info/ASR-switch/error-summary notifications with English/original-locale restoration, F8 same-provider model and external command-provider selection/reload, persisted Tap/Hold/Both timing, and information/error notifications. The HTTP provider gate proves an independent failing server process that returns 404 after real F10/ASR and preserves the selected buffer with no delete/commit, followed by an independent successful server process proving Bearer/JSON transport, selected/raw ASR request content, exact provider-candidate commit, and local-adapter restoration; it explicitly does not claim third-party cloud-service proof. The compatibility ASR cross-provider gate proves an external child-process boundary and exact restoration while reusing the original sherpa/Zipformer recognizer; the companion Whisper gate proves an independent whisper.cpp v1.9.1 process and multilingual `ggml-base.bin` model with pinned hashes, a distinct final commit, and restoration to Zipformer partials. The invalid-scheme remote gate proves old-backend preservation, exact daemon/Fcitx error notification senders and payloads, profile/backup restoration, and subsequent streaming recognition. The successful remote gate proves the implemented OpenAI-compatible HTTP runtime against an independent loopback process: multipart WAV/Bearer/model/language/prompt, final-only commit, redacted evidence, and Zipformer restoration. It is not proof of a real hosted service. The physical gate uses no playback injection; fallback/localization/trigger/menu/model/provider gates restore the original addon config, scene, profile, activation service, Fcitx process, and effective backend. The bounded GTK4 soak retained 20 F9/F10 events per mode, ten completion events, nine ready transitions, at least seven partials per cycle, and exact restoration in 91-92 seconds per mode. The production remote-ASR and text-provider clients additionally have deterministic process coverage for plain-HTTP proxy routing, proxy-URL Basic authentication including CONNECT through TLS-protected HTTPS proxy endpoints, one local CA-signed TLS interception relay with no retained plaintext, same-daemon atomic replacement of one CA-file path with mismatch rejection and idle recovery, `NO_PROXY` bypass, 429/503 bodies, request and response-body timeouts, a 1 MiB cap for success and error response bodies, self-signed TLS rejection, DNS failure, connection refusal, and credential redaction. The text-provider path separately proves the legacy 4000 ms effective timeout when a scene omits `timeout_ms`. Shared URL diagnostics remove userinfo/fragments and preserve only query keys with redacted values while request URLs keep their real query values; ASR `Debug` hides prompts, text `Debug` hides body contents, and failure bodies replace exact known credentials rather than attempting arbitrary secret discovery. Remaining behavior includes additional terminal and sandbox-packaged/application selected-text behavior and hour-scale or longer soak coverage, real hosted-ASR/provider credential lifecycle and production CA distribution/revocation operations, PAC, NTLM/Kerberos, enterprise TLS-interception policy and certificate deployment, real hosted text-provider operations and cross-application recovery, and additional physical-device switching breadth. UI locales beyond the legacy English/zh_CN set are optional expansion.

## Release and platform gaps

- externally hosted repository publication, production signing-key custody/rotation/revocation and independent public-key/fingerprint distribution, Flatpak repository/Flathub policy and live desktop integration, and supported-distro RPM repository/signing/SELinux validation;
- actual host package-installed upgrade and live production multi-user upgrade/removal proof; deterministic ownership-verified upgrade/removal dispatch is complete, unsupported future-schema refusal/preservation is complete, and migration rollback waits for a second production schema;
- successful remote-text challenge collection from another physical network device; the real Chromium same-host LAN path and fail-closed collector are complete;
- further Rust `vinpst-gui` error-message refinement and semantic-tree support are post-`0.1.0`; ordinary management paths have deterministic coverage, positional focus-order desktop collectors are intentionally retired, and the keyboard-supported/screen-reader-unsupported policy plus CLI/Fcitx fallbacks are complete; see [`../architecture/gui-contract.md`](../architecture/gui-contract.md).

## Immediate next work

1. Run the final non-publishing release workflow on the exact protected-`main` candidate.
2. Verify the downloaded manifest, checksums, and release-workflow provenance attestations.
3. Install the native candidate in an unrelated clean user environment and repeat initialization, diagnostics, dictation, command replacement, switching, restart/reload, and removal.
4. Review release notes and limitations, publish `v0.1.0` through the draft-first workflow, and repeat the smoke from public downloads.
5. Treat hosted-provider operations, additional devices/applications, Flatpak desktop integration, RPM/Nix publication, long-duration soak, and semantic-identity GUI automation as post-0.1.0 breadth unless a concrete release defect is found.

## Stop conditions

Do not claim full parity until all of these pass in a documented installation:

```sh
vinpst init
vinpst model list
vinpst model install <id-or-short-id>
vinpst model use <id-or-short-id>
vinpst doctor
vinpst daemon status
vinpst recording start
vinpst recording stop
```

The same profile must also prove real normal dictation, live partial/preedit, command replacement, restart/reload, and clean removal without manual JSON edits.
