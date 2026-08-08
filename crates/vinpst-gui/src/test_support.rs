use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use vinpst_config::VinpstConfig;

use crate::{App, ConfigDocument, Message, Page};

/// Crate-local semantic driver for GUI state tests.
///
/// This deliberately stops below the Iced window/widget boundary. Tests can drive typed
/// application messages and inspect state, while real GUI interaction remains a manual check.
pub(crate) struct GuiHarness {
    app: App,
}

impl GuiHarness {
    #[must_use]
    pub(crate) fn new() -> Self {
        let (app, boot_task) = App::boot();
        drop(boot_task);
        Self { app }
    }

    #[must_use]
    pub(crate) fn with_config(config: VinpstConfig, path: impl Into<PathBuf>, page: Page) -> Self {
        let mut harness = Self::new();
        harness.app.replace_config(Ok(ConfigDocument {
            path: path.into(),
            from_disk: false,
            config,
        }));
        harness.app.page = page;
        harness
    }

    pub(crate) fn send(&mut self, message: Message) {
        drop(self.app.update(message));
    }
}

impl Deref for GuiHarness {
    type Target = App;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl DerefMut for GuiHarness {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}
