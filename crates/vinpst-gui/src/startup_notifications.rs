//! Current upstream startup notification feed and persistent read state.

use crate::keyboard_action::keyboard_button;

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use iced::{
    Element, Length, Task,
    widget::{column, container, row, text},
};
use rustix::fs::{Mode, OFlags};
use serde_json::Value;

use crate::{App, GuiText, Message};

const DEFAULT_NOTIFICATION_URL: &str =
    "https://raw.githubusercontent.com/rijuyuezhu/fcitx-vinpst/main/notification.json";
const NOTIFICATION_URL_ENV: &str = "VINPST_NOTIFICATION_URL";
const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_NOTIFICATION_TITLE_BYTES: usize = 4 * 1024;
const MAX_NOTIFICATION_TEXT_BYTES: usize = 16 * 1024;
const MAX_READ_STATE_BYTES: u64 = 64 * 1024;
const READ_STATE_FILE: &str = "read_notifications";
static NEXT_READ_STATE_FILE_ID: AtomicU64 = AtomicU64::new(1);

/// One startup-notification interaction.
#[derive(Clone)]
pub enum StartupNotificationMessage {
    /// Complete the background fetch and parse operation.
    Loaded(StartupNotificationLoadOutcome),
    /// Mark the current notification as read and close it.
    Acknowledge,
    /// Mark the current notification as read and open its validated details URL.
    OpenDetails,
}

impl fmt::Debug for StartupNotificationMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loaded(outcome) => formatter.debug_tuple("Loaded").field(outcome).finish(),
            Self::Acknowledge => formatter.write_str("Acknowledge"),
            Self::OpenDetails => formatter.write_str("OpenDetails"),
        }
    }
}

/// Secret-safe outcome of loading the optional startup notification.
#[derive(Clone, PartialEq, Eq)]
pub enum StartupNotificationLoadOutcome {
    /// No newer valid notification is available.
    Hidden,
    /// One newer notification is ready for presentation.
    Ready(StartupNotification),
}

impl fmt::Debug for StartupNotificationLoadOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hidden => formatter.write_str("Hidden"),
            Self::Ready(notification) => {
                formatter.debug_tuple("Ready").field(notification).finish()
            }
        }
    }
}

/// Validated startup notification displayed by the GUI.
#[derive(Clone, PartialEq, Eq)]
pub struct StartupNotification {
    id: u64,
    title: String,
    text: String,
    details_url: Option<String>,
}

impl fmt::Debug for StartupNotification {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartupNotification")
            .field("id", &self.id)
            .field("title", &"<redacted remote text>")
            .field("text", &"<redacted remote text>")
            .field("has_details", &self.details_url.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupNotificationState {
    Loading,
    Hidden,
    Ready(StartupNotification),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NotificationEnvironment {
    feed_url: String,
    locale: String,
    read_state_path: Option<PathBuf>,
}

impl NotificationEnvironment {
    fn from_process() -> Self {
        let feed_url = std::env::var(NOTIFICATION_URL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_NOTIFICATION_URL.to_owned());
        Self {
            feed_url,
            locale: process_locale(),
            read_state_path: default_read_state_path(),
        }
    }

    #[cfg(test)]
    fn new(
        feed_url: impl Into<String>,
        locale: impl Into<String>,
        read_state_path: Option<PathBuf>,
    ) -> Self {
        Self {
            feed_url: feed_url.into(),
            locale: locale.into(),
            read_state_path,
        }
    }
}

trait NotificationTextSource {
    fn fetch(&self, url: &str) -> Result<String, ()>;
}

#[derive(Debug, Clone, Copy)]
struct HttpNotificationTextSource;

impl NotificationTextSource for HttpNotificationTextSource {
    fn fetch(&self, url: &str) -> Result<String, ()> {
        vinpst_http::fetch_json_text(url, NOTIFICATION_TIMEOUT).map_err(|_| ())
    }
}

impl App {
    pub(crate) fn begin_startup_notification_load(&mut self) -> Task<Message> {
        self.startup_notification = StartupNotificationState::Loading;
        Task::perform(load_startup_notification(), |outcome| {
            Message::StartupNotification(StartupNotificationMessage::Loaded(outcome))
        })
    }

    pub(crate) fn intercept_startup_notification_message(
        &mut self,
        message: &Message,
    ) -> Option<Task<Message>> {
        let Message::StartupNotification(message) = message else {
            return None;
        };
        Some(match message {
            StartupNotificationMessage::Loaded(outcome) => {
                self.startup_notification = match outcome {
                    StartupNotificationLoadOutcome::Hidden => StartupNotificationState::Hidden,
                    StartupNotificationLoadOutcome::Ready(notification) => {
                        StartupNotificationState::Ready(notification.clone())
                    }
                };
                Task::none()
            }
            StartupNotificationMessage::Acknowledge => self.finish_startup_notification(false),
            StartupNotificationMessage::OpenDetails => self.finish_startup_notification(true),
        })
    }

    pub(crate) fn startup_notification_view(&self) -> Option<Element<'_, Message>> {
        let StartupNotificationState::Ready(notification) = &self.startup_notification else {
            return None;
        };
        let mut actions = row![];
        if notification.details_url.is_some() {
            actions = actions.push(
                keyboard_button(self.locale.text(GuiText::Details)).on_press(
                    Message::StartupNotification(StartupNotificationMessage::OpenDetails),
                ),
            );
        }
        actions = actions.push(keyboard_button(self.locale.text(GuiText::Ok)).on_press(
            Message::StartupNotification(StartupNotificationMessage::Acknowledge),
        ));
        Some(
            container(
                column![
                    text(&notification.title).size(20),
                    text(&notification.text).width(Length::Fill),
                    actions.spacing(8),
                ]
                .spacing(8),
            )
            .padding(12)
            .width(Length::Fill)
            .into(),
        )
    }

    fn finish_startup_notification(&mut self, open_details: bool) -> Task<Message> {
        let read_state_path = default_read_state_path();
        self.finish_startup_notification_with_path(open_details, read_state_path.as_deref())
    }

    fn finish_startup_notification_with_path(
        &mut self,
        open_details: bool,
        read_state_path: Option<&Path>,
    ) -> Task<Message> {
        let StartupNotificationState::Ready(notification) = &self.startup_notification else {
            return Task::none();
        };
        let id = notification.id;
        let details_url = open_details
            .then(|| notification.details_url.clone())
            .flatten();
        if let Some(path) = read_state_path {
            let _ = write_last_read_id(path, id);
        }
        self.startup_notification = StartupNotificationState::Hidden;
        if let Some(url) = details_url {
            return self.begin_notification_details_open(url);
        }
        Task::none()
    }
}

async fn load_startup_notification() -> StartupNotificationLoadOutcome {
    let environment = NotificationEnvironment::from_process();
    crate::blocking_task::run("vinpst-gui-notification-fetch", move || {
        load_startup_notification_with(&HttpNotificationTextSource, &environment)
    })
    .await
    .unwrap_or(StartupNotificationLoadOutcome::Hidden)
}

fn load_startup_notification_with(
    source: &impl NotificationTextSource,
    environment: &NotificationEnvironment,
) -> StartupNotificationLoadOutcome {
    let Ok(body) = source.fetch(&environment.feed_url) else {
        return StartupNotificationLoadOutcome::Hidden;
    };
    let Some(parsed) = parse_notification(&body, &environment.locale) else {
        return StartupNotificationLoadOutcome::Hidden;
    };
    let local_id = environment
        .read_state_path
        .as_deref()
        .and_then(|path| read_last_read_id(path).ok())
        .unwrap_or(0);
    if parsed.id <= local_id {
        return StartupNotificationLoadOutcome::Hidden;
    }
    StartupNotificationLoadOutcome::Ready(parsed.into_notification())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotification {
    id: u64,
    title: String,
    text: String,
    details_url: Option<String>,
}

impl ParsedNotification {
    fn into_notification(self) -> StartupNotification {
        StartupNotification {
            id: self.id,
            title: self.title,
            text: self.text,
            details_url: self.details_url,
        }
    }
}

fn parse_notification(body: &str, locale: &str) -> Option<ParsedNotification> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    let id = value.get("id")?.as_u64()?;
    if id == 0 {
        return None;
    }
    let title = localized_text(value.get("title")?, locale)?;
    let text = localized_text(value.get("text")?, locale)?;
    if !valid_display_text(&title, MAX_NOTIFICATION_TITLE_BYTES)
        || !valid_display_text(&text, MAX_NOTIFICATION_TEXT_BYTES)
    {
        return None;
    }
    let details_url = value
        .get("url")
        .and_then(Value::as_str)
        .and_then(valid_details_url);
    Some(ParsedNotification {
        id,
        title,
        text,
        details_url,
    })
}

fn localized_text(value: &Value, locale: &str) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_owned());
    }
    let values = value.as_object()?;
    let language = locale.split_once('_').map(|(language, _)| language);
    values
        .get(locale)
        .and_then(Value::as_str)
        .or_else(|| language.and_then(|language| values.get(language).and_then(Value::as_str)))
        .or_else(|| values.get("en").and_then(Value::as_str))
        .or_else(|| values.values().find_map(Value::as_str))
        .map(str::to_owned)
}

fn valid_display_text(value: &str, max_bytes: usize) -> bool {
    value.len() <= max_bytes
        && value
            .chars()
            .all(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
}

fn valid_details_url(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    (url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.into())
}

fn process_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .map_or_else(String::new, |value| normalize_locale(&value))
}

fn normalize_locale(locale: &str) -> String {
    let locale = locale
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('-', "_");
    if matches!(locale.as_str(), "C" | "POSIX") {
        String::new()
    } else {
        locale
    }
}

fn default_read_state_path() -> Option<PathBuf> {
    let cache_home = std::env::var_os("XDG_CACHE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".cache"))
        })?;
    Some(cache_home.join("fcitx-vinpst").join(READ_STATE_FILE))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadStateError {
    Unsafe,
    Invalid,
}

fn read_last_read_id(path: &Path) -> Result<u64, ReadStateError> {
    let fd = match rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(0),
        Err(_) => return Err(ReadStateError::Unsafe),
    };
    let file = File::from(fd);
    let metadata = file.metadata().map_err(|_| ReadStateError::Unsafe)?;
    if !metadata.is_file() || metadata.len() > MAX_READ_STATE_BYTES {
        return Err(ReadStateError::Unsafe);
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| ReadStateError::Unsafe)?;
    let mut contents = String::with_capacity(capacity);
    file.take(MAX_READ_STATE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_| ReadStateError::Invalid)?;
    if contents.len() as u64 > MAX_READ_STATE_BYTES {
        return Err(ReadStateError::Unsafe);
    }
    let Some(value) = contents.split_whitespace().next() else {
        return Ok(0);
    };
    value.parse().map_err(|_| ReadStateError::Invalid)
}

fn write_last_read_id(path: &Path, id: u64) -> Result<(), ()> {
    let current = match read_last_read_id(path) {
        Ok(current) => current,
        Err(ReadStateError::Invalid) => 0,
        Err(ReadStateError::Unsafe) => return Err(()),
    };
    if current >= id {
        return Ok(());
    }
    let parent = path.parent().ok_or(())?;
    fs::create_dir_all(parent).map_err(|_| ())?;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.is_file()
    {
        return Err(());
    }
    let temporary_path = read_state_temporary_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary_path)
            .map_err(|_| ())?;
        writeln!(file, "{id}").map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
        fs::rename(&temporary_path, path).map_err(|_| ())?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| ())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn read_state_temporary_path(path: &Path) -> PathBuf {
    let sequence = NEXT_READ_STATE_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct StaticSource(Result<String, ()>);

    impl NotificationTextSource for StaticSource {
        fn fetch(&self, _url: &str) -> Result<String, ()> {
            self.0.clone()
        }
    }

    fn environment(path: Option<PathBuf>, locale: &str) -> NotificationEnvironment {
        NotificationEnvironment::new("https://feed.invalid/notification.json", locale, path)
    }

    fn feed(id: u64) -> String {
        serde_json::json!({
            "id": id,
            "title": {"en_US": "English title", "zh_CN": "中文标题"},
            "text": {"en_US": "English text", "zh_CN": "中文正文"},
            "url": "https://example.invalid/details?from=vinpst"
        })
        .to_string()
    }

    #[test]
    fn parser_matches_current_upstream_locale_and_schema() {
        let parsed = parse_notification(&feed(4), "zh_CN").expect("valid notification");
        assert_eq!(parsed.id, 4);
        assert_eq!(parsed.title, "中文标题");
        assert_eq!(parsed.text, "中文正文");
        assert!(parsed.details_url.is_some());

        let language_fallback = parse_notification(
            r#"{"id":5,"title":{"zh":"标题"},"text":{"zh":"正文"}}"#,
            "zh_TW",
        )
        .expect("language fallback");
        assert_eq!(language_fallback.title, "标题");
        assert_eq!(language_fallback.text, "正文");
    }

    #[test]
    fn parser_accepts_string_fields_and_drops_unsafe_details() {
        let parsed = parse_notification(
            r#"{
                "id":6,
                "title":"Title",
                "text":"Text",
                "url":"http://user:pass@example.invalid/details"
            }"#,
            "en_US",
        )
        .expect("string fields remain compatible");
        assert!(parsed.details_url.is_none());
    }

    #[test]
    fn parser_rejects_zero_ids_controls_and_oversized_text() {
        assert!(parse_notification(r#"{"id":0,"title":"Title","text":"Text"}"#, "en_US").is_none());
        assert!(
            parse_notification(
                r#"{"id":1,"title":"Title","text":"bad\u0000text"}"#,
                "en_US"
            )
            .is_none()
        );
        let oversized = "x".repeat(MAX_NOTIFICATION_TEXT_BYTES + 1);
        let body = serde_json::json!({"id":1,"title":"Title","text":oversized});
        assert!(parse_notification(&body.to_string(), "en_US").is_none());
    }

    #[test]
    fn newer_notification_is_ready_until_acknowledged() {
        let directory = tempfile::tempdir().expect("notification state fixture");
        let path = directory
            .path()
            .join("cache/fcitx-vinpst/read_notifications");
        fs::create_dir_all(path.parent().unwrap()).expect("state parent");
        fs::write(&path, "3").expect("initial read state");
        let source = StaticSource(Ok(feed(4)));
        let environment = environment(Some(path.clone()), "en_US");

        assert!(matches!(
            load_startup_notification_with(&source, &environment),
            StartupNotificationLoadOutcome::Ready(StartupNotification { id: 4, .. })
        ));
        assert_eq!(read_last_read_id(&path), Ok(3));

        write_last_read_id(&path, 4).expect("acknowledge notification");
        assert_eq!(read_last_read_id(&path), Ok(4));
        assert_eq!(
            load_startup_notification_with(&source, &environment),
            StartupNotificationLoadOutcome::Hidden
        );
    }

    #[test]
    fn older_remote_ids_and_fetch_failures_remain_silent() {
        let directory = tempfile::tempdir().expect("notification state fixture");
        let path = directory.path().join("read_notifications");
        fs::write(&path, "9\n").expect("read state");
        assert_eq!(
            load_startup_notification_with(
                &StaticSource(Ok(feed(9))),
                &environment(Some(path), "en_US")
            ),
            StartupNotificationLoadOutcome::Hidden
        );
        assert_eq!(
            load_startup_notification_with(&StaticSource(Err(())), &environment(None, "en_US")),
            StartupNotificationLoadOutcome::Hidden
        );
    }

    #[test]
    fn read_state_is_atomic_monotonic_and_heals_invalid_text() {
        let directory = tempfile::tempdir().expect("notification state fixture");
        let path = directory.path().join("read_notifications");
        fs::write(&path, "invalid").expect("invalid read state");
        assert_eq!(read_last_read_id(&path), Err(ReadStateError::Invalid));
        write_last_read_id(&path, 4).expect("replace invalid state");
        assert_eq!(read_last_read_id(&path), Ok(4));
        write_last_read_id(&path, 3).expect("lower id is a no-op");
        assert_eq!(read_last_read_id(&path), Ok(4));
    }

    #[test]
    fn read_state_rejects_symlinks_and_oversized_files() {
        let directory = tempfile::tempdir().expect("notification state fixture");
        let target = directory.path().join("target");
        fs::write(&target, "1").expect("target fixture");
        let link = directory.path().join("read_notifications");
        std::os::unix::fs::symlink(&target, &link).expect("symlink fixture");
        assert_eq!(read_last_read_id(&link), Err(ReadStateError::Unsafe));
        assert!(write_last_read_id(&link, 2).is_err());

        let oversized = directory.path().join("oversized");
        let file = File::create(&oversized).expect("oversized fixture");
        file.set_len(MAX_READ_STATE_BYTES + 1)
            .expect("extend oversized fixture");
        assert_eq!(read_last_read_id(&oversized), Err(ReadStateError::Unsafe));
        assert!(write_last_read_id(&oversized, 2).is_err());
    }

    #[test]
    fn message_debug_redacts_remote_title_text_and_url() {
        let notification = parse_notification(&feed(4), "en_US")
            .expect("notification")
            .into_notification();
        let debug = format!(
            "{:?}",
            StartupNotificationMessage::Loaded(StartupNotificationLoadOutcome::Ready(notification))
        );
        assert!(!debug.contains("English title"));
        assert!(!debug.contains("English text"));
        assert!(!debug.contains("example.invalid"));
    }

    #[test]
    fn app_loaded_acknowledge_and_details_transitions_match_dialog_semantics() {
        let mut app = crate::test_support::GuiHarness::new();
        let notification = parse_notification(&feed(4), "en_US")
            .expect("notification")
            .into_notification();
        drop(
            app.intercept_startup_notification_message(&Message::StartupNotification(
                StartupNotificationMessage::Loaded(StartupNotificationLoadOutcome::Ready(
                    notification.clone(),
                )),
            )),
        );
        assert!(matches!(
            app.startup_notification,
            StartupNotificationState::Ready(StartupNotification { id: 4, .. })
        ));

        let directory = tempfile::tempdir().expect("notification state fixture");
        let acknowledge_path = directory.path().join("acknowledge/read_notifications");
        drop(app.finish_startup_notification_with_path(false, Some(&acknowledge_path)));
        assert!(matches!(
            app.startup_notification,
            StartupNotificationState::Hidden
        ));
        assert_eq!(read_last_read_id(&acknowledge_path), Ok(4));

        app.startup_notification = StartupNotificationState::Ready(notification);
        let details_path = directory.path().join("details/read_notifications");
        drop(app.finish_startup_notification_with_path(true, Some(&details_path)));
        assert!(matches!(
            app.startup_notification,
            StartupNotificationState::Hidden
        ));
        assert_eq!(read_last_read_id(&details_path), Ok(4));
        assert!(matches!(app.operation, crate::OperationState::Running(_)));
    }

    #[test]
    fn locale_normalization_matches_current_upstream() {
        assert_eq!(normalize_locale("zh_CN.UTF-8"), "zh_CN");
        assert_eq!(normalize_locale("en-US@custom"), "en_US");
        assert_eq!(normalize_locale("C"), "");
    }
}
