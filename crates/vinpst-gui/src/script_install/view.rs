//! Localized provider and adapter installation rendering.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, progress_bar, row, text, text_input},
};
use vinpst_config::VinpstConfig;
use vinpst_registry::{LiveScriptKind, RegistryOperationProgress};

use super::{
    ActiveScriptInstall, ActiveScriptPreparation, ScriptInstallPlan, ScriptInstallState,
    script_primary_action_id,
};
use crate::{
    App, GuiLocale, GuiText, Message, SecretInput,
    script_catalog::ScriptCatalogState,
    script_management::{managed_adapter_script_path, managed_provider_script_path},
};

impl ScriptInstallPlan {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let resource = localized_resource_label(locale, self.kind);
        let mut body = column![
            text(locale.configure_script_before_install(resource, &self.entry.id)).size(18),
            text(locale.text(GuiText::ValuesStoredHidden)),
        ]
        .spacing(8);

        for environment in &self.environment {
            let name = environment.name.clone();
            body = body.push(
                column![
                    text(locale.environment_requirement(&environment.name, environment.required)),
                    text_input(
                        locale.text(GuiText::EnterEnvironmentValue),
                        &environment.value
                    )
                    .secure(true)
                    .on_input(move |value| Message::ScriptEnvironmentChanged {
                        name: name.clone(),
                        value: SecretInput::new(value),
                    })
                    .width(Length::Fill),
                ]
                .spacing(4),
            );
        }

        let can_install = self.missing_required_environment().is_none();
        body.push(
            row![
                keyboard_button(locale.text(GuiText::Continue))
                    .id(script_primary_action_id())
                    .on_press_maybe(can_install.then_some(Message::ConfirmScriptInstall)),
                keyboard_button(locale.text(GuiText::Cancel))
                    .on_press(Message::CancelScriptInstall),
            ]
            .spacing(10),
        )
        .into()
    }
}

impl ScriptInstallState {
    pub(crate) fn view(&self, locale: GuiLocale) -> Option<Element<'_, Message>> {
        match self {
            Self::Idle | Self::Failed { .. } => None,
            Self::Preparing(active) => Some(active.view(locale)),
            Self::AwaitingEnvironment(plan) => Some(plan.view(locale)),
            Self::Active(active) => Some(active.view(locale)),
            Self::Recovering(active) => {
                let resource = localized_resource_label(locale, active.plan.kind);
                Some(
                    column![
                        text(
                            locale.retrying_script_configuration(resource, &active.plan.entry.id,)
                        ),
                        text(locale.text(GuiText::ReusingPublishedScript)),
                    ]
                    .spacing(6)
                    .into(),
                )
            }
            Self::RecoveryRequired { plan, error } => {
                let resource = localized_resource_label(locale, plan.kind);
                let path = plan.script_path.display().to_string();
                Some(
                    column![
                        text(locale.text(GuiText::ScriptPublishedConfigurationIncomplete)).size(18),
                        text(locale.script_published_at(resource, &plan.entry.id, &path)),
                        text(locale.configuration_error(error)),
                        text(locale.text(GuiText::RecoveryInstructions)),
                        row![
                            keyboard_button(locale.text(GuiText::ReloadConfig))
                                .on_press(Message::ReloadConfig),
                            keyboard_button(locale.text(GuiText::RetryConfigurationUpdate))
                                .id(script_primary_action_id())
                                .on_press(Message::RetryScriptConfigUpdate),
                            keyboard_button(locale.text(GuiText::DismissKeepScript))
                                .on_press(Message::DismissScriptRecovery),
                        ]
                        .spacing(10),
                    ]
                    .spacing(8)
                    .into(),
                )
            }
            Self::Succeeded(summary) => Some(text(locale.operation_success(summary)).into()),
            Self::Cancelled { .. } => Some(
                row![
                    text(locale.text(GuiText::ScriptInstallationCancelled)),
                    keyboard_button(locale.text(GuiText::Retry))
                        .id(script_primary_action_id())
                        .on_press(Message::RetryScriptInstall),
                ]
                .spacing(10)
                .into(),
            ),
        }
    }
}

impl App {
    pub(crate) fn provider_install_controls(&self, busy: bool) -> Element<'_, Message> {
        script_catalog_view(
            &self.provider_catalog,
            self.config.as_ref().ok().map(|document| &document.config),
            LiveScriptKind::AsrProvider,
            self.locale,
            busy,
            Message::RefreshProviderCatalog,
            Message::InstallProvider,
        )
    }

    pub(crate) fn adapter_install_controls(&self, busy: bool) -> Element<'_, Message> {
        script_catalog_view(
            &self.adapter_catalog,
            self.config.as_ref().ok().map(|document| &document.config),
            LiveScriptKind::LlmAdapter,
            self.locale,
            busy,
            Message::RefreshAdapterCatalog,
            Message::InstallAdapter,
        )
    }
}

fn script_catalog_view<'a>(
    state: &'a ScriptCatalogState,
    config: Option<&'a VinpstConfig>,
    kind: LiveScriptKind,
    locale: GuiLocale,
    busy: bool,
    refresh: Message,
    install: fn(String) -> Message,
) -> Element<'a, Message> {
    match state {
        ScriptCatalogState::Loading => text(locale.text(GuiText::LoadingCatalog)).into(),
        ScriptCatalogState::Failed(_) => column![
            text(locale.text(GuiText::CatalogUnavailable)),
            keyboard_button(locale.text(GuiText::RefreshCatalog))
                .on_press_maybe((!busy).then_some(refresh)),
        ]
        .spacing(8)
        .into(),
        ScriptCatalogState::Ready(entries) if entries.is_empty() => column![
            text(locale.text(GuiText::NoCatalogItems)),
            keyboard_button(locale.text(GuiText::RefreshCatalog))
                .on_press_maybe((!busy).then_some(refresh)),
        ]
        .spacing(8)
        .into(),
        ScriptCatalogState::Ready(entries) => {
            let mut body = column![].spacing(8);
            for entry in entries {
                let selector = entry.selector().to_owned();
                let action = script_catalog_action(config, kind, &entry.id);
                let mut label = column![text(&entry.title)];
                if let Some(description) = entry.description.as_deref() {
                    label = label.push(text(description).size(12));
                }
                body = body.push(
                    row![
                        label.width(Length::Fill),
                        keyboard_button(locale.text(action.label())).on_press_maybe(
                            (!busy && action.actionable()).then_some(install(selector)),
                        ),
                    ]
                    .spacing(10),
                );
            }
            body.push(
                keyboard_button(locale.text(GuiText::RefreshCatalog))
                    .on_press_maybe((!busy).then_some(refresh)),
            )
            .into()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptCatalogAction {
    Install,
    Update,
    Configured,
}

impl ScriptCatalogAction {
    const fn label(self) -> GuiText {
        match self {
            Self::Install => GuiText::Install,
            Self::Update => GuiText::Update,
            Self::Configured => GuiText::Configured,
        }
    }

    const fn actionable(self) -> bool {
        !matches!(self, Self::Configured)
    }
}

fn script_catalog_action(
    config: Option<&VinpstConfig>,
    kind: LiveScriptKind,
    entry_id: &str,
) -> ScriptCatalogAction {
    let Some(config) = config else {
        return ScriptCatalogAction::Configured;
    };
    match kind {
        LiveScriptKind::AsrProvider => config
            .asr
            .providers
            .iter()
            .find(|provider| provider.id == entry_id)
            .map_or(ScriptCatalogAction::Install, |provider| {
                if managed_provider_script_path(provider).is_some() {
                    ScriptCatalogAction::Update
                } else {
                    ScriptCatalogAction::Configured
                }
            }),
        LiveScriptKind::LlmAdapter => config
            .llm
            .adapters
            .iter()
            .find(|adapter| adapter.id == entry_id)
            .map_or(ScriptCatalogAction::Install, |adapter| {
                if managed_adapter_script_path(adapter).is_some() {
                    ScriptCatalogAction::Update
                } else {
                    ScriptCatalogAction::Configured
                }
            }),
    }
}

impl ActiveScriptPreparation {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let resource = localized_resource_label(locale, self.kind);
        row![
            text(locale.resolving_script_catalog(resource, &self.selector)),
            keyboard_button(locale.text(if self.cancelling {
                GuiText::Cancelling
            } else {
                GuiText::Cancel
            }))
            .on_press_maybe((!self.cancelling).then_some(Message::CancelScriptInstall)),
        ]
        .spacing(10)
        .into()
    }
}

impl ActiveScriptInstall {
    fn view(&self, locale: GuiLocale) -> Element<'_, Message> {
        let mut body =
            column![text(progress_label(locale, self.plan.kind, &self.progress))].spacing(8);
        if let RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes: Some(total_bytes),
        } = self.progress
            && total_bytes > 0
        {
            let permille = downloaded_bytes
                .saturating_mul(1000)
                .checked_div(total_bytes)
                .unwrap_or(0)
                .min(1000);
            let permille = u16::try_from(permille).unwrap_or(1000);
            body = body.push(progress_bar(0.0..=1.0, f32::from(permille) / 1000.0));
        }
        let commit_started = matches!(
            self.progress,
            RegistryOperationProgress::UpdatingConfiguration | RegistryOperationProgress::Completed
        );
        body = body.push(
            keyboard_button(locale.text(if self.cancelling {
                GuiText::Cancelling
            } else if commit_started {
                GuiText::Finishing
            } else {
                GuiText::Cancel
            }))
            .on_press_maybe(
                (!self.cancelling && !commit_started).then_some(Message::CancelScriptInstall),
            ),
        );
        body.into()
    }
}

fn localized_resource_label(locale: GuiLocale, kind: LiveScriptKind) -> &'static str {
    locale.text(match kind {
        LiveScriptKind::AsrProvider => GuiText::AsrProviderResource,
        LiveScriptKind::LlmAdapter => GuiText::TextAdapterResource,
    })
}

fn progress_label(
    locale: GuiLocale,
    kind: LiveScriptKind,
    progress: &RegistryOperationProgress,
) -> String {
    let resource = localized_resource_label(locale, kind);
    match progress {
        RegistryOperationProgress::Preparing => {
            locale.script_progress("preparing", resource, None, None)
        }
        RegistryOperationProgress::ResolvingRegistry => {
            locale.script_progress("resolving", resource, None, None)
        }
        RegistryOperationProgress::Downloading {
            downloaded_bytes,
            total_bytes,
        } => locale.script_progress(
            "downloading",
            resource,
            Some(*downloaded_bytes),
            *total_bytes,
        ),
        RegistryOperationProgress::VerifyingChecksum => {
            locale.script_progress("verifying", resource, None, None)
        }
        RegistryOperationProgress::Extracting { .. } => {
            locale.script_progress("extracting", resource, None, None)
        }
        RegistryOperationProgress::WritingMetadata => {
            locale.script_progress("metadata", resource, None, None)
        }
        RegistryOperationProgress::Publishing => {
            locale.script_progress("publishing", resource, None, None)
        }
        RegistryOperationProgress::UpdatingConfiguration => {
            locale.script_progress("configuration", resource, None, None)
        }
        RegistryOperationProgress::Completed => {
            locale.script_progress("completed", resource, None, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, path::PathBuf};

    use vinpst_config::AsrProviderKind;
    use vinpst_registry::managed_script_relative_path;

    use super::*;

    #[test]
    fn catalog_action_distinguishes_install_managed_update_and_custom_conflict() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        let id = "provider.fixture.catalog";
        assert_eq!(
            script_catalog_action(Some(&config), LiveScriptKind::AsrProvider, id),
            ScriptCatalogAction::Install
        );

        let mut provider = config.asr.providers[0].clone();
        provider.id = id.to_owned();
        provider.kind = AsrProviderKind::Command;
        provider.command = Some("python3".to_owned());
        provider.args = vec!["/tmp/custom-provider.py".to_owned()];
        config.asr.providers.push(provider.clone());
        assert_eq!(
            script_catalog_action(Some(&config), LiveScriptKind::AsrProvider, id),
            ScriptCatalogAction::Configured
        );

        let data_home = env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .map(|home| home.join(".local/share"))
            })
            .expect("test environment has XDG_DATA_HOME or HOME");
        let managed_path = data_home
            .join("fcitx-vinpst/providers")
            .join(managed_script_relative_path(LiveScriptKind::AsrProvider, id).unwrap());
        provider.args = vec![managed_path.to_string_lossy().into_owned()];
        *config.asr.providers.last_mut().expect("fixture provider") = provider;
        assert_eq!(
            script_catalog_action(Some(&config), LiveScriptKind::AsrProvider, id),
            ScriptCatalogAction::Update
        );
    }
}
