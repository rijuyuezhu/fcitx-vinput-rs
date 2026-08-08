//! Typed messages handled by the Rust management GUI.

use std::path::PathBuf;

use crate::{
    AdapterConfigMessage, AdapterRuntimeMessage, AsrProviderMessage, ConfigSaveOutcome,
    DaemonControlMessage, DaemonOwnerEvent, DaemonSnapshot, DesktopActionMessage, HotwordMessage,
    InteractionMessage, LlmProviderMessage, ModelInstallOutcome, Page, SceneMessage,
    ScriptInstallOutcome, ScriptPreparationResult, SecretInput, StartupNotificationMessage,
};

/// Editable Control-page configuration fields.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigDraftMessage {
    /// Update the default recognition language.
    DefaultLanguage(String),
    /// Update the capture target.
    CaptureDevice(String),
    /// Toggle audio normalization before recognition.
    NormalizeAudio(bool),
    /// Update the input gain applied before recognition.
    InputGain(f32),
    /// Toggle output ducking.
    DuckOutput(bool),
    /// Update the output ducking volume.
    DuckVolume(f32),
    /// Toggle voice activity detection.
    VadEnabled(bool),
    /// Update the voice activity threshold.
    VadThreshold(f32),
    /// Select the active ASR provider.
    ActiveProvider(String),
    /// Select the active scene.
    ActiveScene(String),
}

/// GUI messages.
#[derive(Debug, Clone)]
pub enum Message {
    /// Select a main page.
    SelectPage(Page),
    /// Update the current resource filter.
    FilterChanged(String),
    /// Update the model-catalog filter on the Resources page.
    ModelFilterChanged(String),
    /// Select and persist one configured ASR provider from the Control page.
    UseAsrProvider(String),
    /// Apply an ignored keyboard interaction owned by the application shell.
    Interaction(InteractionMessage),
    /// Apply one startup-notification interaction.
    StartupNotification(StartupNotificationMessage),
    /// Apply one global desktop integration action.
    DesktopAction(DesktopActionMessage),
    /// Refresh daemon state over D-Bus.
    RefreshDaemon,
    /// Result of an asynchronous daemon refresh.
    DaemonLoaded {
        /// Refresh generation used to reject stale owner snapshots.
        operation_id: u64,
        /// Typed daemon snapshot outcome.
        result: Result<DaemonSnapshot, String>,
    },
    /// Low-frequency non-activating poll used only while owner signals are unavailable.
    DaemonFallbackPollTick,
    /// Result of a low-frequency non-activating owner fallback poll.
    DaemonFallbackPolled {
        /// Refresh generation used to reject stale poll results.
        operation_id: u64,
        /// Optional snapshot when the well-known name has an owner.
        result: Result<Option<DaemonSnapshot>, String>,
    },
    /// Signal-monitor lifecycle or daemon owner transition.
    DaemonOwnerEvent(DaemonOwnerEvent),
    /// Start, stop, or restart the daemon service.
    DaemonControl(DaemonControlMessage),
    /// Reload config from disk.
    ReloadConfig,
    /// Update one editable Control-page config field.
    ConfigDraft(ConfigDraftMessage),
    /// Apply one scene lifecycle interaction.
    Scene(SceneMessage),
    /// Apply one ASR provider editor interaction.
    AsrProvider(AsrProviderMessage),
    /// Apply one text-adapter runtime interaction.
    AdapterRuntime(AdapterRuntimeMessage),
    /// Apply one text-adapter configuration interaction.
    AdapterConfig(AdapterConfigMessage),
    /// Apply one LLM provider lifecycle interaction.
    LlmProvider(LlmProviderMessage),
    /// Apply one hotword lifecycle interaction.
    Hotword(HotwordMessage),
    /// Restore editable fields from the loaded config.
    ResetConfigDraft,
    /// Validate, back up, and atomically save the config draft.
    SaveConfig,
    /// Result of an asynchronous config save.
    ConfigSaved(Result<ConfigSaveOutcome, String>),
    /// Start normal recording over D-Bus.
    StartRecording,
    /// Stop recording over D-Bus.
    StopRecording,
    /// Result of an asynchronous recording action.
    RecordingActionFinished {
        /// Whether the action started rather than stopped recording.
        start: bool,
        /// Secret-free D-Bus action outcome.
        result: Result<(), String>,
    },
    /// Refresh the browsable live registry model catalog.
    RefreshModelCatalog,
    /// Rescan locally installed model metadata.
    RefreshInstalledModels,
    /// Result of an asynchronous live registry model catalog refresh.
    ModelCatalogLoaded(Result<Vec<crate::RegistryModelSummary>, String>),
    /// Refresh available `PipeWire` capture devices.
    RefreshAudioDevices,
    /// Result of asynchronous capture-device discovery.
    AudioDevicesLoaded(Result<Vec<crate::CaptureDeviceChoice>, String>),
    /// Refresh the browsable ASR provider catalog.
    RefreshProviderCatalog,
    /// Result of an asynchronous ASR provider catalog refresh.
    ProviderCatalogLoaded(Result<Vec<crate::RegistryScriptSummary>, String>),
    /// Refresh the browsable LLM adapter catalog.
    RefreshAdapterCatalog,
    /// Result of an asynchronous LLM adapter catalog refresh.
    AdapterCatalogLoaded(Result<Vec<crate::RegistryScriptSummary>, String>),
    /// Install or update one model selected from the browsable registry catalog.
    InstallRegistryModel(String),
    /// Dismiss the currently presented non-recovery error dialog.
    DismissError,
    /// Request cancellation of the active model installation.
    CancelModelInstall,
    /// Retry the last failed or cancelled model installation.
    RetryModelInstall,
    /// Refresh progress from the active model installation worker.
    ModelInstallProgressTick,
    /// Result of a live registry model installation.
    ModelInstalled {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Typed worker outcome.
        outcome: ModelInstallOutcome,
    },
    /// Ask for confirmation before removing one inactive installed model.
    RequestRemoveInstalledModel(PathBuf),
    /// Remove one inactive installed model directory.
    RemoveInstalledModel(PathBuf),
    /// Result of an installed model removal.
    ModelRemoved(Result<String, String>),
    /// Select one installed model for the active local ASR provider.
    UseInstalledModel(PathBuf),
    /// Result of selecting an installed model and reloading the daemon backend.
    ModelSelected(Result<String, String>),
    /// Show typed details for one installed model.
    SelectInstalledModelDetail(PathBuf),
    /// Show typed details for one ASR provider.
    SelectAsrProviderDetail(String),
    /// Show typed details for one LLM provider.
    SelectLlmProviderDetail(String),
    /// Show typed details for one text adapter.
    SelectLlmAdapterDetail(String),
    /// Close the current resource detail panel.
    ClearResourceDetail,
    /// Install or update one ASR provider selected from the browsable catalog.
    InstallProvider(String),
    /// Install or update one LLM adapter selected from the browsable catalog.
    InstallAdapter(String),
    /// Result of resolving one provider or adapter registry entry.
    ScriptPrepared {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Typed registry-resolution outcome.
        outcome: ScriptPreparationResult,
    },
    /// Update one registry-declared environment value before installation.
    ScriptEnvironmentChanged {
        /// Stable environment variable name from the registry.
        name: String,
        /// User-entered value with redacted debug output.
        value: SecretInput,
    },
    /// Confirm the prepared provider or adapter environment and install it.
    ConfirmScriptInstall,
    /// Request cancellation of the active provider or adapter installation.
    CancelScriptInstall,
    /// Retry the last failed or cancelled provider or adapter installation.
    RetryScriptInstall,
    /// Retry only the configuration commit after a script was already published.
    RetryScriptConfigUpdate,
    /// Dismiss a published-script recovery state without deleting the script.
    DismissScriptRecovery,
    /// Refresh progress from the active provider or adapter worker.
    ScriptInstallProgressTick,
    /// Result of a live provider or adapter installation.
    ScriptInstalled {
        /// Operation generation used to reject stale completions.
        operation_id: u64,
        /// Typed worker outcome.
        outcome: ScriptInstallOutcome,
    },
    /// Open one exact managed command-provider script in the configured editor.
    EditProviderScript(String),
    /// Result of a managed provider script editor process.
    ProviderScriptEdited(Result<String, String>),
    /// Ask for confirmation before removing one ASR provider.
    RequestRemoveAsrProvider {
        /// Stable configured provider id.
        id: String,
        /// Whether removal also deletes a managed installed script.
        managed: bool,
    },
    /// Ask for confirmation before removing one text adapter.
    RequestRemoveTextAdapter {
        /// Stable configured adapter id.
        id: String,
        /// Whether removal also deletes a managed installed script.
        managed: bool,
    },
    /// Ask for confirmation before removing one LLM provider.
    RequestRemoveLlmProvider(String),
    /// Ask for confirmation before removing one inactive scene.
    RequestRemoveScene(String),
    /// Continue the pending destructive removal through its normal validated path.
    ConfirmRemoval,
    /// Dismiss the pending destructive removal without changing configuration or files.
    CancelRemoval,
    /// Remove one inactive managed command ASR provider.
    RemoveProvider(String),
    /// Remove one managed text adapter.
    RemoveAdapter(String),
    /// Result of a provider or adapter removal.
    ScriptRemoved(Result<String, String>),
}

impl Message {
    pub(crate) fn blocked_while_busy(&self) -> bool {
        matches!(
            self,
            Self::SelectPage(_)
                | Self::Interaction(InteractionMessage::SelectPage(_))
                | Self::RequestRemoveInstalledModel(_)
                | Self::RequestRemoveAsrProvider { .. }
                | Self::RequestRemoveTextAdapter { .. }
                | Self::RequestRemoveLlmProvider(_)
                | Self::RequestRemoveScene(_)
                | Self::UseAsrProvider(_)
                | Self::DaemonControl(
                    DaemonControlMessage::Start
                        | DaemonControlMessage::Stop
                        | DaemonControlMessage::Restart
                )
                | Self::ReloadConfig
                | Self::ConfigDraft(_)
                | Self::ResetConfigDraft
                | Self::SaveConfig
                | Self::Scene(
                    SceneMessage::BeginAdd
                        | SceneMessage::BeginEdit(_)
                        | SceneMessage::EditorChanged { .. }
                        | SceneMessage::ProviderSelected(_)
                        | SceneMessage::CancelEdit
                        | SceneMessage::Save
                        | SceneMessage::Use(_)
                        | SceneMessage::Remove(_)
                )
                | Self::AsrProvider(
                    AsrProviderMessage::BeginAdd
                        | AsrProviderMessage::BeginEdit(_)
                        | AsrProviderMessage::KindChanged(_)
                        | AsrProviderMessage::EditorChanged { .. }
                        | AsrProviderMessage::EnvironmentKeyChanged { .. }
                        | AsrProviderMessage::EnvironmentValueChanged { .. }
                        | AsrProviderMessage::AddEnvironment
                        | AsrProviderMessage::RemoveEnvironment(_)
                        | AsrProviderMessage::ResetEdit
                        | AsrProviderMessage::CancelEdit
                        | AsrProviderMessage::Save
                )
                | Self::LlmProvider(
                    LlmProviderMessage::BeginAdd
                        | LlmProviderMessage::BeginEdit(_)
                        | LlmProviderMessage::Remove(_)
                        | LlmProviderMessage::TestInputChanged(_)
                        | LlmProviderMessage::Test(_)
                        | LlmProviderMessage::EditorChanged { .. }
                        | LlmProviderMessage::ResetEdit
                        | LlmProviderMessage::CancelEdit
                        | LlmProviderMessage::Save
                )
                | Self::Hotword(
                    HotwordMessage::ProviderSelected(_)
                        | HotwordMessage::PathChanged(_)
                        | HotwordMessage::BrowsePath
                        | HotwordMessage::SetPath
                        | HotwordMessage::ClearPath
                        | HotwordMessage::LoadContent
                        | HotwordMessage::ContentAction(_)
                        | HotwordMessage::SaveContent
                        | HotwordMessage::ResetChanges
                        | HotwordMessage::RetryActivation
                )
        )
    }
}
