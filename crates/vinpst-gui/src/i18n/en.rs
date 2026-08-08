//! English GUI presentation strings.

use super::GuiText;

pub(super) const fn text(key: GuiText) -> &'static str {
    if (key as u16) <= GuiText::SourceBundledDefault as u16 {
        english_core(key)
    } else if (key as u16) <= GuiText::InvalidUtf8HotwordPath as u16 {
        english_hotwords(key)
    } else if (key as u16) <= GuiText::DetailsOpenedOnHost as u16 {
        english_desktop(key)
    } else if (key as u16) <= GuiText::AdapterLocal as u16 {
        english_resources(key)
    } else if (key as u16) <= GuiText::RemoteTitle as u16 {
        english_forms(key)
    } else if (key as u16) <= GuiText::SaveOrCancelAdapterBeforeRemoval as u16 {
        english_llm_adapter_forms(key)
    } else if (key as u16) <= GuiText::StoppingTextAdapter as u16 {
        english_install(key)
    } else {
        english_install_tail(key)
    }
}

const fn english_core(key: GuiText) -> &'static str {
    match key {
        GuiText::ApplicationTitle => "Vinpst Configuration",
        GuiText::Control => "Control",
        GuiText::Resources => "Resources",
        GuiText::Llm => "LLM",
        GuiText::Hotwords => "Hotwords",
        GuiText::OpenConfig => "Open config",
        GuiText::DaemonService => "Daemon service",
        GuiText::ReloadConfig => "Reload config",
        GuiText::SavingConfiguration => "Saving…",
        GuiText::StartingRecording => "Starting recording…",
        GuiText::StoppingRecording => "Stopping recording…",
        GuiText::RecordingStarted => "Recording started.",
        GuiText::RecordingStopped => {
            "Recording stopped; the recognition result was delivered to the frontend."
        }
        GuiText::DaemonLoading => "Daemon: loading…",
        GuiText::DaemonStatusUnavailable => "Daemon: status unavailable",
        GuiText::RefreshDaemon => "Refresh daemon",
        GuiText::StartDaemon => "Start daemon",
        GuiText::StopDaemon => "Stop daemon",
        GuiText::RestartDaemon => "Restart daemon",
        GuiText::StartingDaemon => "Starting daemon…",
        GuiText::StoppingDaemon => "Stopping daemon…",
        GuiText::RestartingDaemon => "Restarting daemon…",
        GuiText::AudioAndVad => "Audio",
        GuiText::NormalizeAudio => "Normalize audio",
        GuiText::CaptureDevice => "Capture device",
        GuiText::AudioDevicesUnavailable => "Could not refresh the microphone list.",
        GuiText::LockedWhileFinishing => "Finish the current operation first",
        GuiText::DuckOutput => "Duck output while recording",
        GuiText::EnableVad => "Enable voice activity detection",
        GuiText::SaveConfiguration => "Save",
        GuiText::ResetChanges => "Reset",
        GuiText::UnsavedChanges => "Unsaved changes",
        GuiText::ConfigurationUpToDate => "Saved",
        GuiText::ConfigDraftUnavailable => "Config draft is unavailable.",
        GuiText::SourceBundledDefault => "bundled default; Save creates the user file",
        _ => unreachable!(),
    }
}

const fn english_hotwords(key: GuiText) -> &'static str {
    match key {
        GuiText::Details => "Details",
        GuiText::Ok => "OK",
        GuiText::ErrorDialogTitle => "Error",
        GuiText::NoValidConfig => "No valid configuration is loaded.",
        GuiText::NoHotwordProvider => "No local or command ASR provider supports hotword files.",
        GuiText::NoHotwordProviderSelected => "No hotword-capable provider is selected.",
        GuiText::SaveOrResetHotwordBeforeSelecting => {
            "Save or reset the edited hotword content before selecting another file."
        }
        GuiText::SaveOrResetHotwordBeforeProvider => {
            "Save or reset hotword changes before selecting another provider."
        }
        GuiText::SelectedHotwordProviderUnavailable => {
            "The selected hotword provider is unavailable."
        }
        GuiText::SaveHotwordBeforePathChange => {
            "Save the edited hotword content before changing its configured path."
        }
        GuiText::HotwordPathCannotBeEmpty => "Hotword file path cannot be empty.",
        GuiText::SaveHotwordBeforePathClear => {
            "Save the edited hotword content before clearing its configured path."
        }
        GuiText::SaveHotwordBeforeReload => {
            "Save the edited hotword content before loading it again."
        }
        GuiText::SetOrResetPathBeforeLoad => {
            "Set or reset the hotword path before loading content."
        }
        GuiText::SetPathBeforeLoad => "Set a hotword file path before loading content.",
        GuiText::LoadingHotwordContent => "Loading hotword content…",
        GuiText::DiscardedStaleHotwordContent => {
            "Discarded stale hotword content loaded for a previous selection."
        }
        GuiText::LoadedHotwordContent => "Loaded configured hotword content.",
        GuiText::MissingHotwordFileEmptyEditor => {
            "Configured hotword file does not exist yet; loaded an empty editor."
        }
        GuiText::SetOrResetPathBeforeSave => "Set or reset the hotword path before saving content.",
        GuiText::LoadHotwordBeforeSave => "Load the configured hotword file before saving content.",
        GuiText::SetPathBeforeSave => "Set a hotword file path before saving content.",
        GuiText::SavingHotwordContent => "Saving hotword content…",
        GuiText::NoPendingHotwordActivation => "No saved hotword activation is pending retry.",
        GuiText::SelectPendingHotwordProvider => {
            "Select the provider with the pending hotword activation before retrying."
        }
        GuiText::RetryingHotwordActivation => "Retrying hotword activation…",
        GuiText::SettingHotwordPath => "Setting hotword path…",
        GuiText::ClearingHotwordPath => "Clearing hotword path…",
        GuiText::NoProviderSelected => "No provider selected",
        GuiText::AsrProvider => "ASR provider",
        GuiText::HotwordFile => "Hotword file",
        GuiText::HotwordPathPlaceholder => "Path to a UTF-8 hotword file",
        GuiText::Browse => "Browse…",
        GuiText::SetPath => "Use file",
        GuiText::ClearPath => "Disable",
        GuiText::LoadContent => "Reload file",
        GuiText::SaveContent => "Save",
        GuiText::RetryActivation => "Apply to ASR",
        GuiText::OneHotwordPerLine => "One hotword per line; optional score: word 2.0",
        GuiText::HotwordActivationRetryable => "Saved; apply the hotwords to ASR again",
        GuiText::UnsavedHotwordContent => "Unsaved hotword content",
        GuiText::HotwordContentUnchanged => "Saved",
        GuiText::LoadConfiguredHotwordFile => "Reload the file to edit it",
        GuiText::SelectHotwordsFile => "Select Hotwords File",
        GuiText::TextFiles => "Text Files",
        GuiText::AllFiles => "All Files",
        GuiText::SelectingHotwordFile => "Selecting hotword file…",
        GuiText::SelectedHotwordFile => "File selected; choose Use file to validate and apply it.",
        GuiText::InvalidUtf8HotwordPath => "The selected hotword path is not valid UTF-8.",
        _ => unreachable!(),
    }
}

const fn english_desktop(key: GuiText) -> &'static str {
    match key {
        GuiText::OpeningConfig => "Opening config file…",
        GuiText::OpeningNotificationDetails => "Opening notification details…",
        GuiText::NoValidConfigLoaded => "No valid config is loaded.",
        GuiText::ConfigOpenLaunchFailed => {
            "Cannot open the config file: the desktop opener could not be started."
        }
        GuiText::ConfigOpenReaperFailed => {
            "Cannot open the config file: the desktop opener could not be supervised safely."
        }
        GuiText::DetailsOpenLaunchFailed => {
            "Cannot open notification details: the desktop opener could not be started."
        }
        GuiText::DetailsOpenReaperFailed => {
            "Cannot open notification details: the desktop opener could not be supervised safely."
        }
        GuiText::ConfigOpened => "Passed the config file to the desktop opener.",
        GuiText::ConfigOpenedOnHost => "Passed the config file to the host desktop opener.",
        GuiText::DetailsOpened => "Passed notification details to the desktop opener.",
        GuiText::DetailsOpenedOnHost => "Passed notification details to the host desktop opener.",
        _ => unreachable!(),
    }
}

const fn english_resources(key: GuiText) -> &'static str {
    match key {
        GuiText::FilterProvidersAndScenes => "Filter scenes",
        GuiText::FilterModels => "Filter models",
        GuiText::ManagedAsrModels => "Models",
        GuiText::InstalledModels => "Installed models",
        GuiText::AvailableModels => "Available models",
        GuiText::RefreshCatalog => "Refresh catalog",
        GuiText::LoadingModelCatalog => "Loading model catalog…",
        GuiText::LoadingCatalog => "Loading…",
        GuiText::CatalogUnavailable => "Could not load this list.",
        GuiText::NoCatalogItems => "Nothing available.",
        GuiText::NoRegistryModelsAvailable => "No models match the current filter.",
        GuiText::Install => "Install",
        GuiText::Update => "Update",
        GuiText::Continue => "Continue",
        GuiText::ManagedCommandAsrProviders | GuiText::AsrProviders => "ASR providers",
        GuiText::NoManagedModelsInstalled => "No models installed.",
        GuiText::SelectLocalProviderForManagedModel => {
            "Select a local ASR provider before choosing an installed model."
        }
        GuiText::AddCustomProvider => "Add custom provider",
        GuiText::ManagedTextAdapters | GuiText::Adapters => "LLM adapters",
        GuiText::AddCustomAdapter => "Add custom adapter",
        GuiText::RefreshRuntime => "Refresh status",
        GuiText::NoTextAdaptersConfigured => "No LLM adapters configured.",
        GuiText::Remove => "Remove",
        GuiText::Edit => "Edit",
        GuiText::EditScript => "Edit script",
        GuiText::Start => "Start",
        GuiText::Stop => "Stop",
        GuiText::Local => "local",
        GuiText::Remote => "remote",
        GuiText::Command => "command",
        GuiText::UnselectedModel => "unselected model",
        GuiText::Active => "active",
        GuiText::Inactive => "inactive",
        GuiText::CommandAdapter => "command adapter",
        GuiText::RuntimeUnavailable => "status unavailable",
        GuiText::NotReportedByDaemon => "not reported by daemon",
        GuiText::Running => "running",
        GuiText::Stopped => "stopped",
        GuiText::CloseDetails => "Close details",
        GuiText::ResourceDetailsUnavailable => "Resource details unavailable",
        GuiText::Model => "Model",
        GuiText::StableId => "Stable id",
        GuiText::Status => "Status",
        GuiText::Backend => "Backend",
        GuiText::Runtime => "Runtime",
        GuiText::Family => "Family",
        GuiText::Language => "Language",
        GuiText::DeclaredSize => "Declared size",
        GuiText::RegularFiles => "Regular files",
        GuiText::Supported => "supported",
        GuiText::Installed => "installed",
        GuiText::Unsupported => "unsupported",
        GuiText::NotDeclared => "not declared",
        GuiText::InstallDirectory => "Install directory",
        GuiText::MetadataFile => "Metadata file",
        GuiText::Kind => "Kind",
        GuiText::Timeout => "Timeout",
        GuiText::Endpoint => "Endpoint",
        GuiText::ManagedScript => "Managed script",
        GuiText::Arguments => "Arguments",
        GuiText::Environment => "Environment",
        GuiText::LlmProvider => "LLM provider",
        GuiText::TextAdapter => "Text adapter",
        GuiText::Credential => "Credential",
        GuiText::ExtraBodyFields => "Extra body fields",
        GuiText::ExtensionFields => "Extension fields",
        GuiText::WorkingDirectory => "Working directory",
        GuiText::NotConfigured => "not configured",
        GuiText::Configured => "configured",
        GuiText::Yes => "yes",
        GuiText::No => "no",
        GuiText::AdapterLocal => "adapter/local",
        _ => unreachable!(),
    }
}

const fn english_forms(key: GuiText) -> &'static str {
    match key {
        GuiText::Scenes => "Scenes",
        GuiText::AddScene => "Add scene",
        GuiText::Available => "available",
        GuiText::NoScenesMatch => "No scenes match the current filter.",
        GuiText::Use => "Use",
        GuiText::SceneId => "Scene id",
        GuiText::StableUniqueId => "stable unique id",
        GuiText::LabelField => "Label",
        GuiText::DisplayLabelPlaceholder => "display label",
        GuiText::PromptField => "Prompt",
        GuiText::OptionalPromptTemplate => "optional prompt template",
        GuiText::NoProviderClearBinding => "No provider (clear binding)",
        GuiText::ModelOverride => "Model override",
        GuiText::OptionalModelId => "optional model id",
        GuiText::CandidateCount => "Candidate count",
        GuiText::ZeroTo32 => "0 to 32",
        GuiText::TimeoutMsLabel => "Timeout (ms)",
        GuiText::BlankLegacyDefault => "blank uses the legacy default",
        GuiText::ContextLines => "Context lines",
        GuiText::UpdateScene => "Update scene",
        GuiText::Cancel => "Cancel",
        GuiText::SavingSceneConfiguration => "Saving scene configuration…",
        GuiText::SelectingScene => "Selecting scene…",
        GuiText::RemovingScene => "Removing scene…",
        GuiText::AddCustomAsrProvider => "Add custom ASR provider",
        GuiText::EditAsrProvider => "Edit ASR provider",
        GuiText::AddProvider => "Add provider",
        GuiText::UpdateProvider => "Update provider",
        GuiText::ResetForm => "Reset form",
        GuiText::UnsavedProviderChanges => "Unsaved provider changes",
        GuiText::ProviderFormUnchanged => "Provider form is unchanged",
        GuiText::ProviderId => "Provider id",
        GuiText::CustomProviderPlaceholder => "custom-provider",
        GuiText::ProviderType => "Provider type",
        GuiText::BlankBackendDefault => "blank uses backend default",
        GuiText::HotwordsManagedOnPage => {
            "Hotword path and content remain managed on the Hotwords page."
        }
        GuiText::CommandField | GuiText::CommandTitle => "Command",
        GuiText::ProviderCommandPlaceholder => "/path/to/provider",
        GuiText::JsonStringArray => "JSON string array",
        GuiText::AddVariable => "Add variable",
        GuiText::NoEnvironmentVariables => "No environment variables.",
        GuiText::VariableName => "Variable name",
        GuiText::Value => "Value",
        GuiText::SavingAsrProvider => "Saving ASR provider…",
        GuiText::RemovingAsrProvider => "Removing ASR provider…",
        GuiText::SaveOrCancelProviderBeforeRemoval => {
            "Save or cancel the open ASR provider form before removing a provider."
        }
        GuiText::LocalTitle => "Local",
        GuiText::RemoteTitle => "Remote",
        _ => unreachable!(),
    }
}

const fn english_llm_adapter_forms(key: GuiText) -> &'static str {
    match key {
        GuiText::ProvidersTitle => "Providers",
        GuiText::TestInput => "Test input",
        GuiText::TestInputPlaceholder => "short connectivity-test text",
        GuiText::NoLlmProviders => "No LLM providers.",
        GuiText::Test => "Test",
        GuiText::BaseUrl => "Base URL",
        GuiText::BaseUrlPlaceholder => "https://provider.example/v1",
        GuiText::ApiKey => "API key",
        GuiText::OptionalKeyExpression => "optional key or environment expression",
        GuiText::DefaultModel => "Default model",
        GuiText::ExtraBody => "Extra body",
        GuiText::MaskedJsonObjectBlank => "masked JSON object; blank means {}",
        GuiText::TestingLlmProvider => "Testing LLM provider…",
        GuiText::ConnectivityInputRequired => {
            "LLM provider connectivity-test input cannot be empty."
        }
        GuiText::SavingLlmProvider => "Saving LLM provider…",
        GuiText::AddCustomTextAdapter => "Add custom text adapter",
        GuiText::EditTextAdapter => "Edit text adapter",
        GuiText::AdapterId => "Adapter id",
        GuiText::CustomAdapterPlaceholder => "custom-adapter",
        GuiText::AdapterCommandPlaceholder => "/path/to/adapter",
        GuiText::JsonStringObject => "JSON string object",
        GuiText::OptionalWorkingDirectory => "optional absolute or configured path",
        GuiText::AddAdapter => "Add adapter",
        GuiText::UpdateAdapter => "Update adapter",
        GuiText::UnsavedAdapterChanges => "Unsaved adapter changes",
        GuiText::AdapterFormUnchanged => "Adapter form is unchanged",
        GuiText::SavingTextAdapter => "Saving text adapter…",
        GuiText::RemovingTextAdapter => "Removing text adapter…",
        GuiText::SaveOrCancelAdapterBeforeRemoval => {
            "Save or cancel the open text-adapter form before removing an adapter."
        }
        _ => unreachable!(),
    }
}

const fn english_install(key: GuiText) -> &'static str {
    match key {
        GuiText::Retry => "Retry",
        GuiText::Cancelling => "Cancelling…",
        GuiText::Finishing => "Finishing…",
        GuiText::ModelInstallationCancelled => "Model installation cancelled.",
        GuiText::PreparingModelInstallation => "Preparing model installation…",
        GuiText::ResolvingModelCatalog => "Resolving model catalog…",
        GuiText::VerifyingModelChecksum => "Verifying model checksum…",
        GuiText::WritingModelMetadata => "Writing model metadata…",
        GuiText::PublishingModelAtomically => "Publishing model atomically…",
        GuiText::UpdatingConfigurationProgress => "Saving settings…",
        GuiText::ModelInstallationCompleted => "Model installation completed.",
        GuiText::ValuesStoredHidden => {
            "Values are stored in the user configuration and hidden in diagnostics."
        }
        GuiText::Required => "required",
        GuiText::Optional => "optional",
        GuiText::EnterEnvironmentValue => "Enter environment value",
        GuiText::ReusingPublishedScript => {
            "The published script is being reused; no download is running."
        }
        GuiText::ScriptPublishedConfigurationIncomplete => {
            "Script installed, but settings were not saved"
        }
        GuiText::RecoveryInstructions => {
            "Reload after resolving external changes or permissions, then retry. The script will not be downloaded again; dismissing keeps the published file."
        }
        GuiText::RetryConfigurationUpdate => "Retry saving settings",
        GuiText::DismissKeepScript => "Dismiss (keep script)",
        GuiText::ScriptInstallationCancelled => "Script installation cancelled.",
        GuiText::AsrProviderResource => "ASR provider",
        GuiText::TextAdapterResource => "text adapter",
        GuiText::EditingManagedProviderScript => "Opening provider script…",
        GuiText::StartingTextAdapter => "Starting text adapter…",
        GuiText::StoppingTextAdapter => "Stopping text adapter…",
        _ => unreachable!(),
    }
}

const fn english_install_tail(key: GuiText) -> &'static str {
    match key {
        GuiText::ConfigurationSaved => "Configuration saved.",
        GuiText::RemovingModel => "Removing model…",
        GuiText::RemovingProvider => "Removing provider…",
        GuiText::RemovingAdapter => "Removing adapter…",
        GuiText::HotwordChangesBlocked => "Save or reset hotword changes before continuing.",
        GuiText::HotwordActivationNotApplied => {
            "The saved hotword path configuration was not applied to the active daemon; activation can be retried."
        }
        GuiText::ChecksumVerified => "checksum verified",
        GuiText::RegistryNoChecksum => "checksum unavailable",
        GuiText::SelectingModel => "Selecting model…",
        _ => unreachable!(),
    }
}
