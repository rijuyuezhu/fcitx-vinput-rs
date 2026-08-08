//! Resources and LLM page rendering.

use crate::keyboard_action::keyboard_button;

use iced::{
    Element, Length,
    widget::{column, container, row, scrollable, text, text_input},
};
use vinpst_config::AsrProviderKind;
use vinpst_registry::InstalledModelInfo;

use crate::{
    App, GuiLocale, GuiText, Message, model_is_active, model_is_selected_by_active_provider,
    model_management::{
        ModelCatalogState, RegistryModelSummary, active_provider_can_use_managed_models,
    },
    script_management::{managed_adapter_script_path, managed_provider_script_path},
};

impl App {
    pub(super) fn resources_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let resource_controls_busy = busy
            || self.asr_provider_editor.is_some()
            || self.adapter_config_editor.is_some()
            || self.ensure_no_unsaved_config_draft().is_err();
        let mut body = column![
            text(self.locale.text(GuiText::Resources)).size(30),
            text(self.locale.text(GuiText::ManagedAsrModels)).size(22),
            text(self.locale.text(GuiText::InstalledModels)).size(18),
            self.installed_models_view(resource_controls_busy),
            row![
                text(self.locale.text(GuiText::AvailableModels))
                    .size(18)
                    .width(Length::Fill),
                keyboard_button(self.locale.text(GuiText::RefreshCatalog)).on_press_maybe(
                    (!resource_controls_busy
                        && !matches!(self.model_catalog, ModelCatalogState::Loading))
                    .then_some(Message::RefreshModelCatalog),
                ),
            ]
            .spacing(10),
            text_input(self.locale.text(GuiText::FilterModels), &self.model_filter,)
                .on_input(Message::ModelFilterChanged),
            self.available_models_view(resource_controls_busy),
            text(self.locale.text(GuiText::ManagedCommandAsrProviders)).size(22),
            self.provider_install_controls(resource_controls_busy),
            text(self.locale.text(GuiText::ManagedTextAdapters)).size(22),
            self.adapter_install_controls(resource_controls_busy),
        ]
        .spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        scrollable(body).into()
    }

    fn installed_models_view(&self, busy: bool) -> Element<'_, Message> {
        let mut body = column![].spacing(12);
        let can_select_model = self
            .config
            .as_ref()
            .is_ok_and(|document| active_provider_can_use_managed_models(&document.config));
        match &self.installed_models {
            Ok(models) if models.is_empty() => {
                body = body.push(text(self.locale.text(GuiText::NoManagedModelsInstalled)));
            }
            Ok(models) => {
                if self.config.is_ok() && !can_select_model {
                    body = body.push(text(
                        self.locale
                            .text(GuiText::SelectLocalProviderForManagedModel),
                    ));
                }
                for model in models {
                    let (selected, referenced) =
                        self.config.as_ref().map_or((false, false), |document| {
                            (
                                model_is_selected_by_active_provider(
                                    &document.config,
                                    &model.model_dir,
                                ),
                                model_is_active(&document.config, &model.model_dir),
                            )
                        });
                    body = body.push(installed_model_row(
                        self.locale,
                        model,
                        selected,
                        !busy && can_select_model && !selected,
                        !busy && !referenced,
                    ));
                }
            }
            Err(_) => {
                body = body.push(
                    row![
                        text(self.locale.text(GuiText::CatalogUnavailable)),
                        keyboard_button(self.locale.text(GuiText::Retry))
                            .on_press_maybe((!busy).then_some(Message::RefreshInstalledModels)),
                    ]
                    .spacing(10),
                );
            }
        }
        body.into()
    }

    fn available_models_view(&self, busy: bool) -> Element<'_, Message> {
        match &self.model_catalog {
            ModelCatalogState::Loading => {
                text(self.locale.text(GuiText::LoadingModelCatalog)).into()
            }
            ModelCatalogState::Failed(_) => column![
                text(self.locale.text(GuiText::CatalogUnavailable)),
                keyboard_button(self.locale.text(GuiText::RefreshCatalog))
                    .on_press_maybe((!busy).then_some(Message::RefreshModelCatalog)),
            ]
            .spacing(8)
            .into(),
            ModelCatalogState::Ready(models) => {
                let filter = self.model_filter.trim();
                let installed = self.installed_models.as_ref().ok();
                let mut body = column![].spacing(10);
                let mut visible = 0_usize;
                for model in models {
                    if !registry_model_matches_filter(model, filter) {
                        continue;
                    }
                    visible += 1;
                    let is_installed = installed.is_some_and(|installed| {
                        installed
                            .iter()
                            .any(|candidate| candidate.stable_model_id() == model.id)
                    });
                    body = body.push(registry_model_row(self.locale, model, is_installed, busy));
                }
                if visible == 0 {
                    body = body.push(text(self.locale.text(GuiText::NoRegistryModelsAvailable)));
                }
                body.into()
            }
        }
    }

    pub(super) fn configured_asr_providers_view(&self, busy: bool) -> Element<'_, Message> {
        let provider_controls_busy = busy
            || self.asr_provider_editor.is_some()
            || self.ensure_no_unsaved_config_draft().is_err();
        let mut body = column![].spacing(12);
        match &self.config {
            Ok(document) => {
                body = body.push(
                    row![
                        text(self.locale.text(GuiText::AsrProviders))
                            .size(22)
                            .width(Length::Fill),
                        keyboard_button(self.locale.text(GuiText::AddCustomProvider))
                            .on_press_maybe((!provider_controls_busy).then_some(
                                Message::AsrProvider(crate::AsrProviderMessage::BeginAdd,)
                            ),),
                    ]
                    .spacing(10),
                );
                for provider in &document.config.asr.providers {
                    let kind = self.locale.text(match provider.kind {
                        AsrProviderKind::Local => GuiText::Local,
                        AsrProviderKind::Remote => GuiText::Remote,
                        AsrProviderKind::Command => GuiText::Command,
                    });
                    let model = provider
                        .model
                        .as_deref()
                        .unwrap_or_else(|| self.locale.text(GuiText::UnselectedModel));
                    let active = provider.id == document.config.asr.active_provider;
                    let label = if active {
                        format!(
                            "{} · {kind} · {model} · {}",
                            provider.id,
                            self.locale.text(GuiText::Active)
                        )
                    } else {
                        format!("{} · {kind} · {model}", provider.id)
                    };
                    let managed = managed_provider_script_path(provider).is_some();
                    body = body.push(provider_row(
                        self.locale,
                        label,
                        &provider.id,
                        provider_controls_busy,
                        managed,
                        active,
                    ));
                }
                if let Some(editor) = self.asr_provider_editor_view(busy) {
                    body = body.push(editor);
                }
            }
            Err(error) => body = body.push(text(self.locale.config_error(error))),
        }
        body.into()
    }

    pub(super) fn llm_page(&self) -> Element<'_, Message> {
        let busy = self.is_busy();
        let adapter_controls_busy =
            busy || self.llm_provider_editor.is_some() || self.adapter_config_editor.is_some();
        let mut body = column![text(self.locale.text(GuiText::Llm)).size(30)].spacing(12);
        if let Some(notice) = self.operation_notice() {
            body = body.push(notice);
        }
        match &self.config {
            Ok(document) => {
                body = body.push(self.llm_provider_management_view(busy));

                body = body.push(
                    row![
                        text(self.locale.text(GuiText::Adapters))
                            .size(22)
                            .width(Length::Fill),
                        keyboard_button(self.locale.text(GuiText::AddCustomAdapter))
                            .on_press_maybe((!adapter_controls_busy).then_some(
                                Message::AdapterConfig(crate::AdapterConfigMessage::BeginAdd,)
                            ),),
                        keyboard_button(self.locale.text(GuiText::RefreshRuntime)).on_press_maybe(
                            (!adapter_controls_busy).then_some(Message::RefreshDaemon),
                        ),
                    ]
                    .spacing(10),
                );
                for adapter in &document.config.llm.adapters {
                    let managed = managed_adapter_script_path(adapter).is_some();
                    body = body.push(adapter_row(
                        self.locale,
                        &adapter.id,
                        &self.adapter_runtime_view_state(&adapter.id),
                        adapter_controls_busy,
                        managed,
                    ));
                }
                if let Some(editor) = self.adapter_config_editor_view(busy) {
                    body = body.push(editor);
                }
                if document.config.llm.adapters.is_empty() {
                    body = body.push(text(self.locale.text(GuiText::NoTextAdaptersConfigured)));
                }
                body = body.push(
                    text_input(
                        self.locale.text(GuiText::FilterProvidersAndScenes),
                        &self.filter,
                    )
                    .on_input(Message::FilterChanged),
                );
                body = body.push(self.scene_management_view(adapter_controls_busy));
            }
            Err(error) => body = body.push(text(self.locale.config_error(error))),
        }
        scrollable(body).into()
    }
}

fn registry_model_matches_filter(model: &RegistryModelSummary, filter: &str) -> bool {
    let filter = filter.trim().to_ascii_lowercase();
    if filter.is_empty() {
        return true;
    }
    [
        Some(model.id.as_str()),
        model.short_id.as_deref(),
        Some(model.title.as_str()),
        model.description.as_deref(),
        model.model_type.as_deref(),
        model.language.as_deref(),
        model.runtime.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_ascii_lowercase().contains(&filter))
}

fn registry_model_row(
    locale: GuiLocale,
    model: &RegistryModelSummary,
    installed: bool,
    busy: bool,
) -> Element<'static, Message> {
    let model_type = model
        .model_type
        .as_deref()
        .or(model.runtime.as_deref())
        .unwrap_or_else(|| locale.text(GuiText::NotDeclared));
    let language = model
        .language
        .as_deref()
        .unwrap_or_else(|| locale.text(GuiText::NotDeclared));
    let size = model.size_bytes.map_or_else(
        || locale.text(GuiText::NotDeclared).to_owned(),
        format_model_size,
    );
    let hotwords = locale.text(if model.supports_hotwords {
        GuiText::Yes
    } else {
        GuiText::No
    });
    let status = locale.text(if installed {
        GuiText::Installed
    } else if model.supported {
        GuiText::Available
    } else {
        GuiText::Unsupported
    });
    let metadata = format!(
        "{}: {model_type} · {}: {language} · {}: {size} · {}: {hotwords} · {}: {status}",
        locale.text(GuiText::Kind),
        locale.text(GuiText::Language),
        locale.text(GuiText::DeclaredSize),
        locale.text(GuiText::Hotwords),
        locale.text(GuiText::Status),
    );
    let mut details = column![text(model.title.clone()).size(18)].spacing(4);
    if let Some(description) = model
        .description
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        details = details.push(text(description.clone()));
    }
    details = details.push(text(metadata));

    let action = keyboard_button(locale.text(if installed {
        GuiText::Update
    } else {
        GuiText::Install
    }))
    .on_press_maybe(
        (!busy && model.supported)
            .then(|| Message::InstallRegistryModel(model.selector().to_owned())),
    );
    container(row![details.width(Length::Fill), action].spacing(12))
        .padding(12)
        .width(Length::Fill)
        .style(container::rounded_box)
        .into()
}

fn format_model_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    const TIB: u64 = GIB * 1024;
    let (divisor, unit) = if bytes >= TIB {
        (TIB, "TiB")
    } else if bytes >= GIB {
        (GIB, "GiB")
    } else if bytes >= MIB {
        (MIB, "MiB")
    } else if bytes >= KIB {
        (KIB, "KiB")
    } else {
        return format!("{bytes} B");
    };
    let whole = bytes / divisor;
    let tenth = (bytes % divisor).saturating_mul(10) / divisor;
    format!("{whole}.{tenth} {unit}")
}

fn installed_model_row(
    locale: GuiLocale,
    model: &InstalledModelInfo,
    selected: bool,
    use_enabled: bool,
    remove_enabled: bool,
) -> Element<'static, Message> {
    let locale_code = locale.code().to_owned();
    let title = model
        .display_title(&[locale_code])
        .unwrap_or_else(|| model.stable_model_id());
    let directory = model
        .model_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("managed-model");
    row![
        text(locale.installed_model_row(title, directory, model.file_count, selected))
            .width(Length::Fill),
        keyboard_button(locale.text(GuiText::Details))
            .on_press(Message::SelectInstalledModelDetail(model.model_dir.clone())),
        keyboard_button(locale.text(GuiText::Use)).on_press_maybe(
            use_enabled.then_some(Message::UseInstalledModel(model.model_dir.clone())),
        ),
        keyboard_button(locale.text(GuiText::Remove)).on_press_maybe(remove_enabled.then_some(
            Message::RequestRemoveInstalledModel(model.model_dir.clone())
        ),),
    ]
    .spacing(10)
    .into()
}

fn provider_row(
    locale: GuiLocale,
    label: String,
    provider_id: &str,
    busy: bool,
    managed: bool,
    active: bool,
) -> Element<'static, Message> {
    row![
        text(label).width(Length::Fill),
        keyboard_button(locale.text(GuiText::Details))
            .on_press(Message::SelectAsrProviderDetail(provider_id.to_owned())),
        keyboard_button(locale.text(GuiText::Edit)).on_press_maybe((!busy).then_some(
            Message::AsrProvider(crate::AsrProviderMessage::BeginEdit(provider_id.to_owned()),)
        )),
        keyboard_button(locale.text(GuiText::EditScript)).on_press_maybe(
            (!busy && managed).then_some(Message::EditProviderScript(provider_id.to_owned())),
        ),
        keyboard_button(locale.text(GuiText::Use)).on_press_maybe(
            (!busy && !active).then_some(Message::UseAsrProvider(provider_id.to_owned())),
        ),
        keyboard_button(locale.text(GuiText::Remove)).on_press_maybe((!busy && !active).then_some(
            Message::RequestRemoveAsrProvider {
                id: provider_id.to_owned(),
                managed,
            }
        ),),
    ]
    .spacing(10)
    .into()
}

fn adapter_row(
    locale: GuiLocale,
    adapter_id: &str,
    runtime: &crate::adapter_runtime::AdapterRuntimeViewState,
    busy: bool,
    managed: bool,
) -> Element<'static, Message> {
    let start_id = adapter_id.to_owned();
    let stop_id = adapter_id.to_owned();
    row![
        text(locale.adapter_row(adapter_id, &runtime.label)).width(Length::Fill),
        keyboard_button(locale.text(GuiText::Details))
            .on_press(Message::SelectLlmAdapterDetail(adapter_id.to_owned())),
        keyboard_button(locale.text(GuiText::Edit)).on_press_maybe((!busy).then_some(
            Message::AdapterConfig(crate::AdapterConfigMessage::BeginEdit(
                adapter_id.to_owned()
            ),)
        )),
        keyboard_button(locale.text(GuiText::Start)).on_press_maybe(
            (!busy && runtime.can_start).then_some(Message::AdapterRuntime(
                crate::AdapterRuntimeMessage::Start(start_id),
            )),
        ),
        keyboard_button(locale.text(GuiText::Stop)).on_press_maybe(
            (!busy && runtime.can_stop).then_some(Message::AdapterRuntime(
                crate::AdapterRuntimeMessage::Stop(stop_id),
            )),
        ),
        keyboard_button(locale.text(GuiText::Remove)).on_press_maybe((!busy).then_some(
            Message::RequestRemoveTextAdapter {
                id: adapter_id.to_owned(),
                managed,
            }
        ),),
    ]
    .spacing(10)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_model() -> RegistryModelSummary {
        RegistryModelSummary {
            id: "model.test.zh.streaming".to_owned(),
            short_id: Some("zh-stream".to_owned()),
            title: "Chinese Streaming Model".to_owned(),
            description: Some("Low-latency Mandarin recognition".to_owned()),
            model_type: Some("zipformer2_ctc".to_owned()),
            language: Some("zh".to_owned()),
            size_bytes: Some(21_264_113),
            runtime: Some("online".to_owned()),
            supports_hotwords: true,
            supported: true,
        }
    }

    #[test]
    fn registry_model_filter_searches_user_visible_metadata_and_ids() {
        let model = fixture_model();
        assert!(registry_model_matches_filter(&model, ""));
        assert!(registry_model_matches_filter(&model, "mandarin"));
        assert!(registry_model_matches_filter(&model, "ZIPFORMER"));
        assert!(registry_model_matches_filter(&model, "zh-stream"));
        assert!(!registry_model_matches_filter(&model, "english-only"));
    }

    #[test]
    fn registry_model_sizes_use_stable_binary_units_without_float_rounding() {
        assert_eq!(format_model_size(999), "999 B");
        assert_eq!(format_model_size(1536), "1.5 KiB");
        assert_eq!(format_model_size(21_264_113), "20.2 MiB");
        assert_eq!(format_model_size(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }
}
