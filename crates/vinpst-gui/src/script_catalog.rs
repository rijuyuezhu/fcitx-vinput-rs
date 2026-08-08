//! Browsable provider and adapter registry catalogs for the Resources page.

use std::time::Duration;

use iced::Task;
use vinpst_config::VinpstConfig;
use vinpst_registry::{
    LiveScriptKind, RegistryOperationControl, RegistryTextSource, ReqwestRegistryTextSource,
};

use crate::{
    App, GuiLocale, GuiText, Message, blocking_task, model_management::fetch_registry_i18n,
    script_management::fetch_live_script_registry_with_base_from,
};

/// Display-safe metadata for one installable provider or adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryScriptSummary {
    pub(crate) id: String,
    pub(crate) short_id: Option<String>,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
}

impl RegistryScriptSummary {
    pub(crate) fn selector(&self) -> &str {
        self.short_id.as_deref().unwrap_or(&self.id)
    }
}

/// Asynchronous registry state shown on the Resources page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ScriptCatalogState {
    #[default]
    Loading,
    Ready(Vec<RegistryScriptSummary>),
    Failed(String),
}

pub(crate) fn load_registry_script_catalog(
    config: &VinpstConfig,
    locale: GuiLocale,
    kind: LiveScriptKind,
) -> Result<Vec<RegistryScriptSummary>, String> {
    let source = ReqwestRegistryTextSource::with_timeout(Duration::from_secs(30));
    fetch_registry_script_catalog_from(config, locale, kind, &source)
}

fn fetch_registry_script_catalog_from(
    config: &VinpstConfig,
    locale: GuiLocale,
    kind: LiveScriptKind,
    source: &impl RegistryTextSource,
) -> Result<Vec<RegistryScriptSummary>, String> {
    let control = RegistryOperationControl::default();
    let (registry, base) =
        fetch_live_script_registry_with_base_from(config, kind, &control, source)?;
    let i18n = fetch_registry_i18n(source, &base, locale);
    Ok(registry
        .items
        .iter()
        .map(|entry| RegistryScriptSummary {
            id: entry.id.clone(),
            short_id: entry.short_id.clone(),
            title: entry.resolved_title(i18n.as_ref()),
            description: entry.resolved_description(i18n.as_ref()),
        })
        .collect())
}

impl App {
    pub(super) fn begin_provider_catalog_refresh(&mut self) -> Task<Message> {
        let Ok(document) = &self.config else {
            self.provider_catalog = ScriptCatalogState::Failed(
                self.locale.text(GuiText::NoValidConfigLoaded).to_owned(),
            );
            return Task::none();
        };
        let config = document.config.clone();
        let locale = self.locale;
        self.provider_catalog = ScriptCatalogState::Loading;
        blocking_task::perform(
            "vinpst-gui-provider-catalog",
            move || load_registry_script_catalog(&config, locale, LiveScriptKind::AsrProvider),
            |result| {
                Message::ProviderCatalogLoaded(result.unwrap_or_else(|failure| {
                    Err(format!(
                        "ASR provider catalog worker stopped unexpectedly: {failure}"
                    ))
                }))
            },
        )
    }

    pub(super) fn begin_adapter_catalog_refresh(&mut self) -> Task<Message> {
        let Ok(document) = &self.config else {
            self.adapter_catalog = ScriptCatalogState::Failed(
                self.locale.text(GuiText::NoValidConfigLoaded).to_owned(),
            );
            return Task::none();
        };
        let config = document.config.clone();
        let locale = self.locale;
        self.adapter_catalog = ScriptCatalogState::Loading;
        blocking_task::perform(
            "vinpst-gui-adapter-catalog",
            move || load_registry_script_catalog(&config, locale, LiveScriptKind::LlmAdapter),
            |result| {
                Message::AdapterCatalogLoaded(result.unwrap_or_else(|failure| {
                    Err(format!(
                        "LLM adapter catalog worker stopped unexpectedly: {failure}"
                    ))
                }))
            },
        )
    }

    pub(super) fn finish_provider_catalog_refresh(
        &mut self,
        result: Result<Vec<RegistryScriptSummary>, String>,
    ) {
        self.provider_catalog =
            result.map_or_else(ScriptCatalogState::Failed, ScriptCatalogState::Ready);
    }

    pub(super) fn finish_adapter_catalog_refresh(
        &mut self,
        result: Result<Vec<RegistryScriptSummary>, String>,
    ) {
        self.adapter_catalog =
            result.map_or_else(ScriptCatalogState::Failed, ScriptCatalogState::Ready);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use vinpst_registry::RegistryTextSource;

    use super::*;

    struct FixtureSource {
        files: HashMap<String, String>,
    }

    impl RegistryTextSource for FixtureSource {
        fn fetch_registry_text(&self, url: &str) -> Result<String, String> {
            self.files
                .get(url)
                .cloned()
                .ok_or_else(|| format!("missing fixture `{url}`"))
        }
    }

    #[test]
    fn provider_catalog_uses_localized_titles_and_stable_selectors() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.registry.base_urls = vec!["https://registry.example".to_owned()];
        let source = FixtureSource {
            files: HashMap::from([
                (
                    "https://registry.example/registry/providers.json".to_owned(),
                    r#"{"version":1,"items":[{"id":"provider.fixture.demo","short_id":"demo","stream":false,"command":"python3","script_urls":["https://registry.example/demo.py"]}]}"#.to_owned(),
                ),
                (
                    "https://registry.example/i18n/en_US.json".to_owned(),
                    r#"{"provider.fixture.demo.title":"Demo provider","provider.fixture.demo.description":"Simple demo recognizer"}"#.to_owned(),
                ),
            ]),
        };

        let catalog = fetch_registry_script_catalog_from(
            &config,
            GuiLocale::EnUs,
            LiveScriptKind::AsrProvider,
            &source,
        )
        .expect("catalog");

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].selector(), "demo");
        assert_eq!(catalog[0].title, "Demo provider");
        assert_eq!(
            catalog[0].description.as_deref(),
            Some("Simple demo recognizer")
        );
    }

    #[test]
    fn adapter_catalog_uses_the_adapter_registry_endpoint() {
        let mut config = VinpstConfig::bundled_default().expect("bundled config");
        config.registry.base_urls = vec!["https://registry.example".to_owned()];
        let source = FixtureSource {
            files: HashMap::from([
                (
                    "https://registry.example/registry/adapters.json".to_owned(),
                    r#"{"version":1,"items":[{"id":"adapter.fixture.demo","short_id":"demo","command":"python3","script_urls":["https://registry.example/demo.py"]}]}"#.to_owned(),
                ),
                (
                    "https://registry.example/i18n/en_US.json".to_owned(),
                    r#"{"adapter.fixture.demo.title":"Demo adapter"}"#.to_owned(),
                ),
            ]),
        };

        let catalog = fetch_registry_script_catalog_from(
            &config,
            GuiLocale::EnUs,
            LiveScriptKind::LlmAdapter,
            &source,
        )
        .expect("catalog");

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].selector(), "demo");
        assert_eq!(catalog[0].title, "Demo adapter");
    }
}
