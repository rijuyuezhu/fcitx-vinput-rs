//! GUI state and task ownership for provider and adapter installation.

mod view;

use std::{
    fmt,
    sync::{Arc, Mutex},
};

use iced::{Task, widget::Id};
use vinpst_registry::{
    LiveScriptEntry, LiveScriptKind, RegistryOperationControl, RegistryOperationProgress,
};

use crate::{
    App, ConfigDocument, GuiLocale, GuiText, Message, OperationState, load_config_document,
    script_management::{install_registry_script_controlled, prepare_registry_script_controlled},
    script_recovery::recover_registry_script_config,
};

pub(crate) fn script_primary_action_id() -> Id {
    Id::new("vinpst-gui-script-primary-action")
}

/// A user-entered value whose debug representation is always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretInput(String);

impl SecretInput {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn into_inner(self) -> String {
        self.0
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// One registry-declared environment value collected before installation.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ScriptEnvironmentValue {
    pub(crate) name: String,
    pub(crate) required: bool,
    pub(crate) value: String,
}

impl fmt::Debug for ScriptEnvironmentValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptEnvironmentValue")
            .field("name", &self.name)
            .field("required", &self.required)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// A resolved provider or adapter installation request.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ScriptInstallPlan {
    pub(crate) kind: LiveScriptKind,
    pub(crate) selector: String,
    pub(crate) entry: LiveScriptEntry,
    pub(crate) script_root: std::path::PathBuf,
    pub(crate) script_path: std::path::PathBuf,
    pub(crate) environment: Vec<ScriptEnvironmentValue>,
}

impl fmt::Debug for ScriptInstallPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptInstallPlan")
            .field("kind", &self.kind)
            .field("selector", &self.selector)
            .field("entry_id", &self.entry.id)
            .field("script_root", &self.script_root)
            .field("script_path", &self.script_path)
            .field("environment", &self.environment)
            .finish()
    }
}

impl ScriptInstallPlan {
    pub(crate) fn missing_required_environment(&self) -> Option<&str> {
        self.environment
            .iter()
            .find(|value| value.required && value.value.trim().is_empty())
            .map(|value| value.name.as_str())
    }

    fn set_environment(&mut self, name: &str, value: String) {
        if let Some(environment) = self
            .environment
            .iter_mut()
            .find(|environment| environment.name == name)
        {
            environment.value = value;
        }
    }
}

/// Result of resolving a provider or adapter registry entry before installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptPrepareOutcome {
    Prepared(Box<ScriptInstallPlan>),
    Cancelled,
    Failed(String),
}

/// Opaque, debug-safe result carried by the public GUI message type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptPreparationResult(ScriptPrepareOutcome);

impl ScriptPreparationResult {
    fn new(outcome: ScriptPrepareOutcome) -> Self {
        Self(outcome)
    }

    pub(crate) fn into_inner(self) -> ScriptPrepareOutcome {
        self.0
    }
}

/// Final typed outcome of a GUI provider or adapter installation worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptInstallOutcome {
    /// The script and its validated configuration entry were installed.
    Installed(String),
    /// The user or application shutdown requested cancellation.
    Cancelled,
    /// The script was published, but its configuration entry could not be committed.
    PublishedButConfigFailed {
        /// Config mutation or persistence error without environment values.
        error: String,
    },
    /// The operation failed.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptRetryRequest {
    Prepare {
        kind: LiveScriptKind,
        selector: String,
    },
    Install(Box<ScriptInstallPlan>),
}

#[derive(Debug, Default)]
pub(crate) enum ScriptInstallState {
    #[default]
    Idle,
    Preparing(ActiveScriptPreparation),
    AwaitingEnvironment(Box<ScriptInstallPlan>),
    Active(Box<ActiveScriptInstall>),
    Recovering(Box<ActiveScriptRecovery>),
    RecoveryRequired {
        plan: Box<ScriptInstallPlan>,
        error: String,
    },
    Succeeded(String),
    Cancelled {
        retry: ScriptRetryRequest,
    },
    Failed {
        retry: ScriptRetryRequest,
        error: String,
    },
}

impl ScriptInstallState {
    pub(crate) fn primary_action_focus_id(&self) -> Option<Id> {
        match self {
            Self::AwaitingEnvironment(plan) if plan.missing_required_environment().is_none() => {
                Some(script_primary_action_id())
            }
            Self::RecoveryRequired { .. } | Self::Cancelled { .. } | Self::Failed { .. } => {
                Some(script_primary_action_id())
            }
            Self::Idle
            | Self::Preparing(_)
            | Self::AwaitingEnvironment(_)
            | Self::Active(_)
            | Self::Recovering(_)
            | Self::Succeeded(_) => None,
        }
    }

    pub(crate) fn failure_message(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            Self::Idle
            | Self::Preparing(_)
            | Self::AwaitingEnvironment(_)
            | Self::Active(_)
            | Self::Recovering(_)
            | Self::RecoveryRequired { .. }
            | Self::Succeeded(_)
            | Self::Cancelled { .. } => None,
        }
    }

    pub(crate) fn dismiss_failure(&mut self) {
        if matches!(self, Self::Failed { .. }) {
            *self = Self::Idle;
        }
    }
}

#[derive(Debug)]
pub(crate) struct ActiveScriptPreparation {
    operation_id: u64,
    kind: LiveScriptKind,
    selector: String,
    control: RegistryOperationControl,
    cancelling: bool,
}

impl Drop for ActiveScriptPreparation {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

#[derive(Debug)]
pub(crate) struct ActiveScriptInstall {
    operation_id: u64,
    plan: ScriptInstallPlan,
    control: RegistryOperationControl,
    shared_progress: Arc<Mutex<RegistryOperationProgress>>,
    progress: RegistryOperationProgress,
    cancelling: bool,
}

#[derive(Debug)]
pub(crate) struct ActiveScriptRecovery {
    operation_id: u64,
    plan: ScriptInstallPlan,
}

impl Drop for ActiveScriptInstall {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

impl ScriptInstallState {
    pub(crate) fn has_worker(&self) -> bool {
        matches!(
            self,
            Self::Preparing(_) | Self::Active(_) | Self::Recovering(_)
        )
    }

    pub(crate) fn blocks_operations(&self) -> bool {
        matches!(
            self,
            Self::Preparing(_)
                | Self::AwaitingEnvironment(_)
                | Self::Active(_)
                | Self::Recovering(_)
                | Self::RecoveryRequired { .. }
        )
    }

    pub(crate) fn start_preparation(
        document: ConfigDocument,
        kind: LiveScriptKind,
        selector: String,
        operation_id: u64,
    ) -> (Self, Task<Message>) {
        let control = RegistryOperationControl::default();
        let worker_control = control.clone();
        let worker_selector = selector.clone();
        let task = crate::blocking_task::perform(
            "vinpst-gui-script-prepare",
            move || {
                prepare_registry_script_controlled(
                    &document,
                    kind,
                    &worker_selector,
                    &worker_control,
                )
            },
            move |result| Message::ScriptPrepared {
                operation_id,
                outcome: ScriptPreparationResult::new(result.unwrap_or_else(|_| {
                    ScriptPrepareOutcome::Failed(
                        "Script preparation worker stopped unexpectedly.".to_owned(),
                    )
                })),
            },
        );
        (
            Self::Preparing(ActiveScriptPreparation {
                operation_id,
                kind,
                selector,
                control,
                cancelling: false,
            }),
            task,
        )
    }

    pub(crate) fn start_install(
        document: ConfigDocument,
        plan: ScriptInstallPlan,
        operation_id: u64,
        locale: GuiLocale,
    ) -> (Self, Task<Message>) {
        let initial_progress = RegistryOperationProgress::Preparing;
        let shared_progress = Arc::new(Mutex::new(initial_progress.clone()));
        let reported = Arc::clone(&shared_progress);
        let control = RegistryOperationControl::new(move |progress| {
            *reported
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = progress;
        });
        let worker_control = control.clone();
        let worker_plan = plan.clone();
        let task = crate::blocking_task::perform(
            "vinpst-gui-script-install",
            move || {
                install_registry_script_controlled(&document, &worker_plan, &worker_control, locale)
            },
            move |result| Message::ScriptInstalled {
                operation_id,
                outcome: result.unwrap_or_else(|_| {
                    ScriptInstallOutcome::Failed(
                        "Script installation worker stopped unexpectedly.".to_owned(),
                    )
                }),
            },
        );
        (
            Self::Active(Box::new(ActiveScriptInstall {
                operation_id,
                plan,
                control,
                shared_progress,
                progress: initial_progress,
                cancelling: false,
            })),
            task,
        )
    }

    pub(crate) fn start_recovery(
        document: ConfigDocument,
        plan: ScriptInstallPlan,
        operation_id: u64,
        locale: GuiLocale,
    ) -> (Self, Task<Message>) {
        let worker_plan = plan.clone();
        let task = crate::blocking_task::perform(
            "vinpst-gui-script-recovery",
            move || recover_registry_script_config(&document, &worker_plan, locale),
            move |result| Message::ScriptInstalled {
                operation_id,
                outcome: result.unwrap_or_else(|_| {
                    ScriptInstallOutcome::PublishedButConfigFailed {
                        error: "Configuration recovery worker stopped unexpectedly.".to_owned(),
                    }
                }),
            },
        );
        (
            Self::Recovering(Box::new(ActiveScriptRecovery { operation_id, plan })),
            task,
        )
    }

    pub(crate) fn cancel(&mut self) {
        match self {
            Self::Preparing(active) => {
                active.control.cancel();
                active.cancelling = true;
            }
            Self::AwaitingEnvironment(plan) => {
                let retry = ScriptRetryRequest::Install(plan.clone());
                *self = Self::Cancelled { retry };
            }
            Self::Active(active) => {
                active.control.cancel();
                active.cancelling = true;
            }
            Self::Idle
            | Self::Recovering(_)
            | Self::RecoveryRequired { .. }
            | Self::Succeeded(_)
            | Self::Cancelled { .. }
            | Self::Failed { .. } => {}
        }
    }

    pub(crate) fn refresh_progress(&mut self) {
        if let Self::Active(active) = self {
            active.progress = active
                .shared_progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
        }
    }

    pub(crate) fn finish_preparation(
        &mut self,
        operation_id: u64,
        outcome: ScriptPrepareOutcome,
    ) -> bool {
        let (kind, selector) = match self {
            Self::Preparing(active) if active.operation_id == operation_id => {
                (active.kind, active.selector.clone())
            }
            _ => return false,
        };
        *self = match outcome {
            ScriptPrepareOutcome::Prepared(plan) => Self::AwaitingEnvironment(plan),
            ScriptPrepareOutcome::Cancelled => Self::Cancelled {
                retry: ScriptRetryRequest::Prepare { kind, selector },
            },
            ScriptPrepareOutcome::Failed(error) => Self::Failed {
                retry: ScriptRetryRequest::Prepare { kind, selector },
                error,
            },
        };
        true
    }

    pub(crate) fn take_plan_without_environment(&mut self) -> Option<ScriptInstallPlan> {
        let Self::AwaitingEnvironment(plan) = self else {
            return None;
        };
        if !plan.environment.is_empty() {
            return None;
        }
        let plan = (**plan).clone();
        *self = Self::Idle;
        Some(plan)
    }

    pub(crate) fn update_environment(&mut self, name: &str, value: String) {
        if let Self::AwaitingEnvironment(plan) = self {
            plan.set_environment(name, value);
        }
    }

    pub(crate) fn confirmed_plan(&self) -> Result<Option<ScriptInstallPlan>, String> {
        let Self::AwaitingEnvironment(plan) = self else {
            return Ok(None);
        };
        if let Some(name) = plan.missing_required_environment() {
            return Err(format!(
                "Enter a value for required environment variable `{name}` before installing."
            ));
        }
        Ok(Some((**plan).clone()))
    }

    pub(crate) fn finish_install(
        &mut self,
        operation_id: u64,
        outcome: ScriptInstallOutcome,
    ) -> bool {
        let (plan, recovering) = match self {
            Self::Active(active) if active.operation_id == operation_id => {
                (active.plan.clone(), false)
            }
            Self::Recovering(active) if active.operation_id == operation_id => {
                (active.plan.clone(), true)
            }
            _ => return false,
        };
        *self = match outcome {
            ScriptInstallOutcome::Installed(summary) => Self::Succeeded(summary),
            ScriptInstallOutcome::PublishedButConfigFailed { error } => Self::RecoveryRequired {
                plan: Box::new(plan),
                error,
            },
            ScriptInstallOutcome::Cancelled if recovering => Self::RecoveryRequired {
                plan: Box::new(plan),
                error: "Configuration recovery was cancelled before completion.".to_owned(),
            },
            ScriptInstallOutcome::Failed(error) if recovering => Self::RecoveryRequired {
                plan: Box::new(plan),
                error,
            },
            ScriptInstallOutcome::Cancelled => Self::Cancelled {
                retry: ScriptRetryRequest::Install(Box::new(plan)),
            },
            ScriptInstallOutcome::Failed(error) => Self::Failed {
                retry: ScriptRetryRequest::Install(Box::new(plan)),
                error,
            },
        };
        true
    }

    pub(crate) fn recovery_plan(&self) -> Option<ScriptInstallPlan> {
        match self {
            Self::RecoveryRequired { plan, .. } => Some((**plan).clone()),
            _ => None,
        }
    }

    pub(crate) fn dismiss_recovery(&mut self) {
        if matches!(self, Self::RecoveryRequired { .. }) {
            *self = Self::Idle;
        }
    }

    pub(crate) fn set_recovery_error(&mut self, message: String) {
        if let Self::RecoveryRequired { error, .. } = self {
            *error = message;
        }
    }

    fn retry_request(&self) -> Option<ScriptRetryRequest> {
        match self {
            Self::Cancelled { retry } | Self::Failed { retry, .. } => Some(retry.clone()),
            Self::Idle
            | Self::Preparing(_)
            | Self::AwaitingEnvironment(_)
            | Self::Active(_)
            | Self::Recovering(_)
            | Self::RecoveryRequired { .. }
            | Self::Succeeded(_) => None,
        }
    }
}

impl App {
    pub(crate) fn begin_provider_install(&mut self, selector: String) -> Task<Message> {
        self.begin_script_install(LiveScriptKind::AsrProvider, selector)
    }

    pub(crate) fn begin_adapter_install(&mut self, selector: String) -> Task<Message> {
        self.begin_script_install(LiveScriptKind::LlmAdapter, selector)
    }

    pub(crate) fn begin_script_install(
        &mut self,
        kind: LiveScriptKind,
        selector: String,
    ) -> Task<Message> {
        self.begin_script_preparation_for(kind, selector)
    }

    pub(crate) fn retry_script_install(&mut self) -> Task<Message> {
        let Some(retry) = self.script_install.retry_request() else {
            return Task::none();
        };
        match retry {
            ScriptRetryRequest::Prepare { kind, selector } => {
                self.begin_script_preparation_for(kind, selector)
            }
            ScriptRetryRequest::Install(plan) => self.begin_resolved_script_install(*plan),
        }
    }

    pub(crate) fn retry_script_config_update(&mut self) -> Task<Message> {
        let Some(plan) = self.script_install.recovery_plan() else {
            return Task::none();
        };
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_llm_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_asr_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            let error = self
                .config
                .as_ref()
                .err()
                .cloned()
                .unwrap_or_else(|| "No valid config is loaded.".to_owned());
            self.script_install
                .set_recovery_error(format!("Reloaded config is invalid: {error}"));
            return Task::none();
        };
        let document = document.clone();
        let operation_id = self.next_script_operation_id();
        let (state, task) =
            ScriptInstallState::start_recovery(document, plan, operation_id, self.locale);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    pub(crate) fn dismiss_script_recovery(&mut self) {
        self.script_install.dismiss_recovery();
        self.operation = OperationState::Idle;
    }

    fn begin_script_preparation_for(
        &mut self,
        kind: LiveScriptKind,
        selector: String,
    ) -> Task<Message> {
        if selector.is_empty() {
            let resource = self.locale.text(match kind {
                LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
                LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
            });
            self.operation =
                OperationState::Failed(self.locale.registry_selector_required(resource));
            return Task::none();
        }
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_llm_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_asr_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let document = document.clone();
        if self.is_busy() {
            return Task::none();
        }
        let operation_id = self.next_script_operation_id();
        let (state, task) =
            ScriptInstallState::start_preparation(document, kind, selector, operation_id);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    pub(crate) fn finish_script_preparation(
        &mut self,
        operation_id: u64,
        outcome: ScriptPrepareOutcome,
    ) -> Task<Message> {
        if !self
            .script_install
            .finish_preparation(operation_id, outcome)
        {
            return Task::none();
        }
        let Some(plan) = self.script_install.take_plan_without_environment() else {
            return Task::none();
        };
        self.begin_resolved_script_install(plan)
    }

    pub(crate) fn update_script_environment(&mut self, name: &str, value: SecretInput) {
        self.script_install
            .update_environment(name, value.into_inner());
    }

    pub(crate) fn confirm_script_install(&mut self) -> Task<Message> {
        match self.script_install.confirmed_plan() {
            Ok(Some(plan)) => self.begin_resolved_script_install(plan),
            Ok(None) => Task::none(),
            Err(error) => {
                self.operation = OperationState::Failed(error);
                Task::none()
            }
        }
    }

    fn begin_resolved_script_install(&mut self, plan: ScriptInstallPlan) -> Task<Message> {
        if let Err(error) = self.ensure_no_unsaved_config_draft() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_scene_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_llm_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        if let Err(error) = self.ensure_no_open_asr_provider_editor() {
            self.operation = OperationState::Failed(error);
            return Task::none();
        }
        let Ok(document) = &self.config else {
            self.operation =
                OperationState::Failed(self.locale.text(GuiText::NoValidConfigLoaded).to_owned());
            return Task::none();
        };
        let document = document.clone();
        if matches!(
            self.script_install,
            ScriptInstallState::Preparing(_)
                | ScriptInstallState::Active(_)
                | ScriptInstallState::Recovering(_)
        ) {
            return Task::none();
        }
        if let Some(name) = plan.missing_required_environment() {
            self.operation = OperationState::Failed(self.locale.required_environment_value(name));
            self.script_install = ScriptInstallState::AwaitingEnvironment(Box::new(plan));
            return Task::none();
        }
        let operation_id = self.next_script_operation_id();
        let (state, task) =
            ScriptInstallState::start_install(document, plan, operation_id, self.locale);
        self.operation = OperationState::Idle;
        self.script_install = state;
        task
    }

    fn next_script_operation_id(&mut self) -> u64 {
        let operation_id = self.next_script_install_id;
        self.next_script_install_id = self.next_script_install_id.wrapping_add(1).max(1);
        operation_id
    }

    pub(crate) fn finish_script_install(
        &mut self,
        operation_id: u64,
        outcome: ScriptInstallOutcome,
    ) -> Task<Message> {
        if !self.script_install.finish_install(operation_id, outcome) {
            return Task::none();
        }
        if matches!(self.script_install, ScriptInstallState::Succeeded(_)) {
            let path = self
                .config
                .as_ref()
                .ok()
                .map(|document| document.path.clone());
            self.replace_config(load_config_document(path.as_deref()));
            return self.begin_daemon_refresh(false);
        }
        Task::none()
    }
}

#[cfg(test)]
#[path = "script_install_guard_tests.rs"]
mod guard_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(environment: Vec<ScriptEnvironmentValue>) -> ScriptInstallPlan {
        ScriptInstallPlan {
            kind: LiveScriptKind::AsrProvider,
            selector: "fixture".to_owned(),
            entry: LiveScriptEntry {
                id: "provider.fixture.batch".to_owned(),
                short_id: Some("fixture".to_owned()),
                stream: false,
                command: "python3".to_owned(),
                script_urls: vec!["https://example.invalid/provider.py".to_owned()],
                readme_url: None,
                envs: Vec::new(),
            },
            script_root: "/tmp/providers".into(),
            script_path: "/tmp/providers/fixture/batch".into(),
            environment,
        }
    }

    #[test]
    fn stale_preparation_does_not_replace_active_operation() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinpst_config::VinpstConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) = ScriptInstallState::start_preparation(
            document,
            LiveScriptKind::AsrProvider,
            "fixture".to_owned(),
            12,
        );

        assert!(!state.finish_preparation(11, ScriptPrepareOutcome::Cancelled));
        assert!(state.has_worker());
        assert!(state.finish_preparation(12, ScriptPrepareOutcome::Cancelled));
        assert!(matches!(
            state.retry_request(),
            Some(ScriptRetryRequest::Prepare { .. })
        ));
    }

    #[test]
    fn required_environment_blocks_confirmation_until_entered() {
        let mut state =
            ScriptInstallState::AwaitingEnvironment(Box::new(plan(vec![ScriptEnvironmentValue {
                name: "TOKEN".to_owned(),
                required: true,
                value: String::new(),
            }])));

        assert!(state.primary_action_focus_id().is_none());
        assert!(state.confirmed_plan().is_err());
        state.update_environment("TOKEN", "super-secret".to_owned());
        assert!(state.primary_action_focus_id().is_some());
        let confirmed = state
            .confirmed_plan()
            .expect("valid environment")
            .expect("pending plan");
        assert_eq!(confirmed.environment[0].value, "super-secret");
    }

    #[test]
    fn failed_install_retry_preserves_environment_without_debug_exposure() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinpst_config::VinpstConfig::bundled_default().expect("bundled config"),
        };
        let install_plan = plan(vec![ScriptEnvironmentValue {
            name: "TOKEN".to_owned(),
            required: true,
            value: "super-secret".to_owned(),
        }]);
        let (mut state, _) =
            ScriptInstallState::start_install(document, install_plan, 13, GuiLocale::EnUs);

        assert!(state.finish_install(
            13,
            ScriptInstallOutcome::Failed("fixture failure".to_owned())
        ));
        assert!(state.primary_action_focus_id().is_some());
        let debug = format!("{state:?}");
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("TOKEN"));
        assert!(matches!(
            state.retry_request(),
            Some(ScriptRetryRequest::Install(_))
        ));
    }

    #[test]
    fn stale_install_completion_does_not_replace_active_operation() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinpst_config::VinpstConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) =
            ScriptInstallState::start_install(document, plan(Vec::new()), 15, GuiLocale::EnUs);

        assert!(!state.finish_install(14, ScriptInstallOutcome::Cancelled));
        assert!(state.has_worker());
        assert!(state.finish_install(15, ScriptInstallOutcome::Cancelled));
        assert!(matches!(
            state.retry_request(),
            Some(ScriptRetryRequest::Install(_))
        ));
    }

    #[test]
    fn published_script_failure_enters_redacted_recovery_state() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinpst_config::VinpstConfig::bundled_default().expect("bundled config"),
        };
        let install_plan = plan(vec![ScriptEnvironmentValue {
            name: "TOKEN".to_owned(),
            required: true,
            value: "super-secret".to_owned(),
        }]);
        let (mut state, _) =
            ScriptInstallState::start_install(document, install_plan, 16, GuiLocale::EnUs);

        assert!(state.finish_install(
            16,
            ScriptInstallOutcome::PublishedButConfigFailed {
                error: "permission denied".to_owned(),
            }
        ));

        assert!(matches!(state, ScriptInstallState::RecoveryRequired { .. }));
        assert!(state.blocks_operations());
        assert!(state.recovery_plan().is_some());
        assert!(!format!("{state:?}").contains("super-secret"));
    }

    #[test]
    fn stale_recovery_completion_is_rejected_and_dismiss_clears_state() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinpst_config::VinpstConfig::bundled_default().expect("bundled config"),
        };
        let (mut state, _) =
            ScriptInstallState::start_recovery(document, plan(Vec::new()), 17, GuiLocale::EnUs);

        assert!(!state.finish_install(
            16,
            ScriptInstallOutcome::PublishedButConfigFailed {
                error: "stale".to_owned(),
            }
        ));
        assert!(state.has_worker());
        assert!(state.finish_install(
            17,
            ScriptInstallOutcome::PublishedButConfigFailed {
                error: "still blocked".to_owned(),
            }
        ));
        assert!(matches!(state, ScriptInstallState::RecoveryRequired { .. }));

        state.dismiss_recovery();

        assert!(matches!(state, ScriptInstallState::Idle));
    }

    #[test]
    fn environment_message_debug_never_exposes_entered_value() {
        let message = Message::ScriptEnvironmentChanged {
            name: "TOKEN".to_owned(),
            value: SecretInput::new("super-secret".to_owned()),
        };

        let debug = format!("{message:?}");
        assert!(debug.contains("TOKEN"));
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn active_script_state_cancels_when_dropped() {
        let document = ConfigDocument {
            path: "/tmp/config.json".into(),
            from_disk: false,
            config: vinpst_config::VinpstConfig::bundled_default().expect("bundled config"),
        };
        let (state, _) =
            ScriptInstallState::start_install(document, plan(Vec::new()), 14, GuiLocale::EnUs);
        let control = match &state {
            ScriptInstallState::Active(active) => active.control.clone(),
            _ => panic!("active script install state"),
        };

        drop(state);

        assert!(control.is_cancelled());
    }

    #[test]
    fn secret_input_debug_is_redacted() {
        let input = SecretInput::new("super-secret".to_owned());
        assert_eq!(format!("{input:?}"), "<redacted>");
    }
}
