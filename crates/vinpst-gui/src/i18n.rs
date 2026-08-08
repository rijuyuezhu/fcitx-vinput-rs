//! Typed English and Simplified Chinese presentation strings for the Rust GUI.

use std::env;

mod en;
mod keys;
mod zh_cn;

pub(crate) use keys::GuiText;

/// Locale set required for legacy management-GUI parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiLocale {
    /// English fallback.
    EnUs,
    /// Simplified Chinese.
    ZhCn,
}

impl GuiLocale {
    pub(crate) const fn default_capture_device(self) -> &'static str {
        match self {
            Self::EnUs => "Default",
            Self::ZhCn => "默认",
        }
    }

    /// Detects the preferred GUI locale using the legacy environment priority.
    #[must_use]
    pub fn detect() -> Self {
        Self::from_candidates(
            ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .filter_map(|name| env::var(name).ok()),
        )
    }

    /// Resolves one locale name without reading process-global state.
    #[must_use]
    pub fn from_name(value: &str) -> Self {
        Self::from_candidates([value])
    }

    /// Stable locale identifier used by diagnostics.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::EnUs => "en_US",
            Self::ZhCn => "zh_CN",
        }
    }

    /// Resolves one typed static presentation string.
    #[must_use]
    pub(crate) const fn text(self, key: GuiText) -> &'static str {
        match self {
            Self::EnUs => en::text(key),
            Self::ZhCn => zh_cn::text(key),
        }
    }

    fn from_candidates(values: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        values
            .into_iter()
            .flat_map(|value| {
                value
                    .as_ref()
                    .split(':')
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .find_map(|value| normalized_locale(&value))
            .unwrap_or(Self::EnUs)
    }

    pub(crate) fn config_error(self, error: &str) -> String {
        match self {
            Self::EnUs => format!("Config error: {error}"),
            Self::ZhCn => format!("配置错误：{error}"),
        }
    }

    pub(crate) fn daemon_status(self, status: &str) -> String {
        match self {
            Self::EnUs => format!("Daemon: {status}"),
            Self::ZhCn => format!("守护进程：{status}"),
        }
    }

    pub(crate) fn duck_volume(self, percent: f32) -> String {
        match self {
            Self::EnUs => format!("Duck volume: {percent:.0}%"),
            Self::ZhCn => format!("录音时输出音量：{percent:.0}%"),
        }
    }

    pub(crate) fn input_gain(self, gain: f32) -> String {
        match self {
            Self::EnUs => format!("Input gain: {gain:.1}×"),
            Self::ZhCn => format!("输入增益：{gain:.1}×"),
        }
    }

    pub(crate) fn operation_success(self, message: &str) -> String {
        match self {
            Self::EnUs => format!("Success: {message}"),
            Self::ZhCn => format!("成功：{message}"),
        }
    }

    pub(crate) fn daemon_action_failure(self, action: DaemonActionName) -> String {
        match (self, action) {
            (Self::EnUs, DaemonActionName::Start) => {
                "Cannot start daemon: D-Bus activation did not return a valid daemon snapshot."
                    .to_owned()
            }
            (Self::ZhCn, DaemonActionName::Start) => {
                "无法启动守护进程：D-Bus 激活未返回有效的守护进程状态。".to_owned()
            }
            (Self::EnUs, DaemonActionName::Stop) => {
                "Cannot stop daemon: the user-service command was rejected or could not be executed."
                    .to_owned()
            }
            (Self::ZhCn, DaemonActionName::Stop) => {
                "无法停止守护进程：用户服务命令被拒绝或无法执行。".to_owned()
            }
            (Self::EnUs, DaemonActionName::Restart) => {
                "Cannot restart daemon: the user-service command was rejected or could not be executed."
                    .to_owned()
            }
            (Self::ZhCn, DaemonActionName::Restart) => {
                "无法重启守护进程：用户服务命令被拒绝或无法执行。".to_owned()
            }
        }
    }

    pub(crate) fn daemon_state_confirmed(self, running: bool) -> String {
        match (self, running) {
            (Self::EnUs, true) => "Daemon running state confirmed.".to_owned(),
            (Self::EnUs, false) => "Daemon stopped state confirmed.".to_owned(),
            (Self::ZhCn, true) => "已确认守护进程正在运行。".to_owned(),
            (Self::ZhCn, false) => "已确认守护进程已停止。".to_owned(),
        }
    }

    pub(crate) fn daemon_action_unconfirmed(
        self,
        action: DaemonActionName,
        unavailable: bool,
    ) -> String {
        let action = match (self, action) {
            (Self::EnUs, DaemonActionName::Start) => "start",
            (Self::EnUs, DaemonActionName::Stop) => "stop",
            (Self::EnUs, DaemonActionName::Restart) => "restart",
            (Self::ZhCn, DaemonActionName::Start) => "启动",
            (Self::ZhCn, DaemonActionName::Stop) => "停止",
            (Self::ZhCn, DaemonActionName::Restart) => "重启",
        };
        match (self, unavailable) {
            (Self::EnUs, false) => format!(
                "Daemon {action} request was accepted, but the observed owner state did not confirm it."
            ),
            (Self::EnUs, true) => format!(
                "Daemon {action} request was accepted; current owner state is unavailable and will be reconciled by D-Bus monitoring."
            ),
            (Self::ZhCn, false) => {
                format!("守护进程{action}请求已接受，但观察到的所有者状态未能确认结果。")
            }
            (Self::ZhCn, true) => format!(
                "守护进程{action}请求已接受；当前所有者状态不可用，将由 D-Bus 监控进行协调。"
            ),
        }
    }

    pub(crate) fn installed_model_row(
        self,
        title: &str,
        directory: &str,
        file_count: usize,
        active: bool,
    ) -> String {
        let state = self.text(if active {
            GuiText::Active
        } else {
            GuiText::Inactive
        });
        match self {
            Self::EnUs => format!("{title} · {directory} · {file_count} files · {state}"),
            Self::ZhCn => format!("{title} · {directory} · {file_count} 个文件 · {state}"),
        }
    }

    pub(crate) fn adapter_row(self, adapter_id: &str, runtime: &str) -> String {
        format!(
            "{adapter_id} · {} · {runtime}",
            self.text(GuiText::CommandAdapter)
        )
    }

    pub(crate) fn runtime_running_pid(self, pid: u32) -> String {
        format!("{} · pid {pid}", self.text(GuiText::Running))
    }

    pub(crate) fn model_detail_title(self, title: &str) -> String {
        format!("{} · {title}", self.text(GuiText::Model))
    }

    pub(crate) fn asr_provider_detail_title(self, id: &str) -> String {
        format!("{} · {id}", self.text(GuiText::AsrProvider))
    }

    pub(crate) fn llm_provider_detail_title(self, id: &str) -> String {
        format!("{} · {id}", self.text(GuiText::LlmProvider))
    }

    pub(crate) fn text_adapter_detail_title(self, id: &str) -> String {
        format!("{} · {id}", self.text(GuiText::TextAdapter))
    }

    pub(crate) fn configured_count(self, count: usize) -> String {
        match self {
            Self::EnUs => format!("{count} configured"),
            Self::ZhCn => format!("已配置 {count} 项"),
        }
    }

    pub(crate) fn scene_provider_choice(self, provider_id: Option<&str>) -> String {
        match provider_id {
            None => self.text(GuiText::NoProviderClearBinding).to_owned(),
            Some(provider_id) => match self {
                Self::EnUs => format!("Provider: {provider_id}"),
                Self::ZhCn => format!("提供商：{provider_id}"),
            },
        }
    }

    pub(crate) fn scene_id_immutable(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Scene id: {scene_id} (immutable)"),
            Self::ZhCn => format!("场景 ID：{scene_id}（不可修改）"),
        }
    }

    pub(crate) fn provider_identity(self, provider_id: &str, kind: &str) -> String {
        match self {
            Self::EnUs => {
                format!("Provider id: {provider_id} (immutable) · type: {kind} (immutable)")
            }
            Self::ZhCn => {
                format!("提供商 ID：{provider_id}（不可修改）· 类型：{kind}（不可修改）")
            }
        }
    }

    pub(crate) fn selected_label(self, label: &str) -> String {
        match self {
            Self::EnUs => format!("{label} (selected)"),
            Self::ZhCn => format!("{label}（已选择）"),
        }
    }

    pub(crate) fn scene_added(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Added scene `{scene_id}`."),
            Self::ZhCn => format!("已添加场景“{scene_id}”。"),
        }
    }

    pub(crate) fn scene_updated(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Updated scene `{scene_id}`."),
            Self::ZhCn => format!("已更新场景“{scene_id}”。"),
        }
    }

    pub(crate) fn scene_selected(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Selected scene `{scene_id}`."),
            Self::ZhCn => format!("已选择场景“{scene_id}”。"),
        }
    }

    pub(crate) fn scene_removed(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Removed scene `{scene_id}`."),
            Self::ZhCn => format!("已移除场景“{scene_id}”。"),
        }
    }

    pub(crate) fn asr_provider_changed(self, created: bool, provider_id: &str) -> String {
        match (self, created) {
            (Self::EnUs, true) => format!("Added ASR provider `{provider_id}`."),
            (Self::EnUs, false) => format!("Updated ASR provider `{provider_id}`."),
            (Self::ZhCn, true) => format!("已添加 ASR 提供商“{provider_id}”。"),
            (Self::ZhCn, false) => format!("已更新 ASR 提供商“{provider_id}”。"),
        }
    }

    pub(crate) fn asr_provider_removed(self, provider_id: &str) -> String {
        match self {
            Self::EnUs => format!("Removed custom ASR provider `{provider_id}`."),
            Self::ZhCn => format!("已移除自定义 ASR 提供商“{provider_id}”。"),
        }
    }

    pub(crate) fn save_receipt(
        self,
        summary: &str,
        path: &str,
        backup: Option<&str>,
        daemon_reload: &str,
    ) -> String {
        match (self, backup) {
            (Self::EnUs, Some(backup)) => {
                format!("{summary} Saved {path} (backup {backup}); {daemon_reload}")
            }
            (Self::EnUs, None) => {
                format!("{summary} Saved {path} (no previous file); {daemon_reload}")
            }
            (Self::ZhCn, Some(backup)) => {
                format!("{summary} 已保存 {path}（备份 {backup}）；{daemon_reload}")
            }
            (Self::ZhCn, None) => {
                format!("{summary} 已保存 {path}（此前无文件）；{daemon_reload}")
            }
        }
    }

    pub(crate) fn provider_id_immutable(self, provider_id: &str) -> String {
        match self {
            Self::EnUs => format!("Provider id: {provider_id} (immutable)"),
            Self::ZhCn => format!("提供商 ID：{provider_id}（不可修改）"),
        }
    }

    pub(crate) fn adapter_id_immutable(self, adapter_id: &str) -> String {
        match self {
            Self::EnUs => format!("Adapter id: {adapter_id} (immutable)"),
            Self::ZhCn => format!("适配器 ID：{adapter_id}（不可修改）"),
        }
    }

    pub(crate) fn llm_provider_changed(self, action: &str, provider_id: &str) -> String {
        match (self, action) {
            (Self::EnUs, "add") => format!("Added LLM provider `{provider_id}`."),
            (Self::EnUs, "update") => format!("Updated LLM provider `{provider_id}`."),
            (Self::EnUs, _) => format!("Removed LLM provider `{provider_id}`."),
            (Self::ZhCn, "add") => format!("已添加 LLM 提供商“{provider_id}”。"),
            (Self::ZhCn, "update") => format!("已更新 LLM 提供商“{provider_id}”。"),
            (Self::ZhCn, _) => format!("已移除 LLM 提供商“{provider_id}”。"),
        }
    }

    pub(crate) fn llm_provider_removed(self, provider_id: &str, cleared_scenes: usize) -> String {
        if cleared_scenes == 0 {
            return self.llm_provider_changed("remove", provider_id);
        }
        match self {
            Self::EnUs => format!(
                "Removed LLM provider `{provider_id}` and cleared it from {cleared_scenes} scene(s)."
            ),
            Self::ZhCn => {
                format!(
                    "已移除 LLM 提供商“{provider_id}”，并从 {cleared_scenes} 个场景中清除其引用。"
                )
            }
        }
    }

    pub(crate) fn llm_provider_test_succeeded(
        self,
        provider_id: &str,
        candidate_count: usize,
    ) -> String {
        match self {
            Self::EnUs => {
                format!("LLM provider `{provider_id}` returned {candidate_count} candidate(s).")
            }
            Self::ZhCn => {
                format!("LLM 提供商“{provider_id}”返回了 {candidate_count} 个候选结果。")
            }
        }
    }

    pub(crate) fn text_adapter_changed(self, created: bool, adapter_id: &str) -> String {
        match (self, created) {
            (Self::EnUs, true) => format!("Added text adapter `{adapter_id}`."),
            (Self::EnUs, false) => format!("Updated text adapter `{adapter_id}`."),
            (Self::ZhCn, true) => format!("已添加文本适配器“{adapter_id}”。"),
            (Self::ZhCn, false) => format!("已更新文本适配器“{adapter_id}”。"),
        }
    }

    pub(crate) fn text_adapter_removed(self, adapter_id: &str) -> String {
        match self {
            Self::EnUs => format!("Removed custom text adapter `{adapter_id}`."),
            Self::ZhCn => format!("已移除自定义文本适配器“{adapter_id}”。"),
        }
    }

    pub(crate) fn model_download_progress(self, downloaded: &str, total: Option<&str>) -> String {
        match (self, total) {
            (Self::EnUs, None) => format!("Downloading model… {downloaded} received"),
            (Self::EnUs, Some(total)) => format!("Downloading model… {downloaded} of {total}"),
            (Self::ZhCn, None) => format!("正在下载模型… 已接收 {downloaded}"),
            (Self::ZhCn, Some(total)) => format!("正在下载模型… {downloaded} / {total}"),
        }
    }

    pub(crate) fn model_extraction_progress(
        self,
        processed_entries: u64,
        extracted: &str,
    ) -> String {
        match self {
            Self::EnUs => {
                format!("Extracting model… {processed_entries} entries, {extracted}")
            }
            Self::ZhCn => format!("正在解压模型… {processed_entries} 个条目，{extracted}"),
        }
    }

    pub(crate) fn configure_script_before_install(self, resource: &str, entry_id: &str) -> String {
        match self {
            Self::EnUs => format!("Configure {resource} `{entry_id}` before installation"),
            Self::ZhCn => format!("安装前配置{resource}“{entry_id}”"),
        }
    }

    pub(crate) fn environment_requirement(self, name: &str, required: bool) -> String {
        let requirement = self.text(if required {
            GuiText::Required
        } else {
            GuiText::Optional
        });
        match self {
            Self::EnUs => format!("{name} ({requirement})"),
            Self::ZhCn => format!("{name}（{requirement}）"),
        }
    }

    pub(crate) fn resolving_script_catalog(self, resource: &str, selector: &str) -> String {
        match self {
            Self::EnUs => format!("Resolving {resource} catalog for `{selector}`…"),
            Self::ZhCn => format!("正在为“{selector}”解析{resource}目录…"),
        }
    }

    pub(crate) fn retrying_script_configuration(self, resource: &str, entry_id: &str) -> String {
        match self {
            Self::EnUs => format!("Retrying configuration for {resource} `{entry_id}`…"),
            Self::ZhCn => format!("正在重试{resource}“{entry_id}”的配置…"),
        }
    }

    pub(crate) fn script_published_at(self, resource: &str, entry_id: &str, path: &str) -> String {
        match self {
            Self::EnUs => format!("{resource} `{entry_id}` was published at {path}."),
            Self::ZhCn => format!("{resource}“{entry_id}”已发布到 {path}。"),
        }
    }

    pub(crate) fn configuration_error(self, error: &str) -> String {
        match self {
            Self::EnUs => format!("Configuration error: {error}"),
            Self::ZhCn => format!("配置错误：{error}"),
        }
    }

    pub(crate) fn script_progress(
        self,
        phase: &str,
        resource: &str,
        downloaded_bytes: Option<u64>,
        total_bytes: Option<u64>,
    ) -> String {
        match (self, phase, downloaded_bytes, total_bytes) {
            (Self::EnUs, "preparing", _, _) => format!("Preparing {resource} installation…"),
            (Self::ZhCn, "preparing", _, _) => format!("正在准备{resource}安装…"),
            (Self::EnUs, "resolving", _, _) => format!("Resolving {resource} catalog…"),
            (Self::ZhCn, "resolving", _, _) => format!("正在解析{resource}目录…"),
            (Self::EnUs, "downloading", Some(received), None) => {
                format!("Downloading {resource} script… {received} bytes received")
            }
            (Self::ZhCn, "downloading", Some(received), None) => {
                format!("正在下载{resource}脚本… 已接收 {received} 字节")
            }
            (Self::EnUs, "downloading", Some(received), Some(total)) => {
                format!("Downloading {resource} script… {received} of {total} bytes")
            }
            (Self::ZhCn, "downloading", Some(received), Some(total)) => {
                format!("正在下载{resource}脚本… {received} / {total} 字节")
            }
            (Self::EnUs, "verifying", _, _) => format!("Verifying {resource} script…"),
            (Self::ZhCn, "verifying", _, _) => format!("正在校验{resource}脚本…"),
            (Self::EnUs, "extracting", _, _) => format!("Extracting {resource} resources…"),
            (Self::ZhCn, "extracting", _, _) => format!("正在解压{resource}资源…"),
            (Self::EnUs, "metadata", _, _) => format!("Writing {resource} metadata…"),
            (Self::ZhCn, "metadata", _, _) => format!("正在写入{resource}元数据…"),
            (Self::EnUs, "publishing", _, _) => format!("Publishing {resource} script…"),
            (Self::ZhCn, "publishing", _, _) => format!("正在发布{resource}脚本…"),
            (Self::EnUs, "configuration", _, _) => {
                format!("Updating configuration for {resource}…")
            }
            (Self::ZhCn, "configuration", _, _) => format!("正在更新{resource}配置…"),
            (Self::EnUs, _, _, _) => format!("{resource} installation completed."),
            (Self::ZhCn, _, _, _) => format!("{resource}安装已完成。"),
        }
    }

    pub(crate) fn adapter_runtime_progress(self, start: bool) -> &'static str {
        self.text(if start {
            GuiText::StartingTextAdapter
        } else {
            GuiText::StoppingTextAdapter
        })
    }

    pub(crate) fn adapter_runtime_previous_owner(self, adapter_id: &str, start: bool) -> String {
        match (self, start) {
            (Self::EnUs, true) => format!(
                "Text adapter `{adapter_id}` start request completed for a previous daemon owner; refreshing the current runtime state."
            ),
            (Self::EnUs, false) => format!(
                "Text adapter `{adapter_id}` stop request completed for a previous daemon owner; refreshing the current runtime state."
            ),
            (Self::ZhCn, true) => format!(
                "文本适配器“{adapter_id}”的启动请求在先前守护进程所有者上完成；正在刷新当前运行状态。"
            ),
            (Self::ZhCn, false) => format!(
                "文本适配器“{adapter_id}”的停止请求在先前守护进程所有者上完成；正在刷新当前运行状态。"
            ),
        }
    }

    pub(crate) fn adapter_runtime_confirmed(self, adapter_id: &str, running: bool) -> String {
        match (self, running) {
            (Self::EnUs, true) => {
                format!("Text adapter `{adapter_id}` running state confirmed.")
            }
            (Self::EnUs, false) => {
                format!("Text adapter `{adapter_id}` stopped state confirmed.")
            }
            (Self::ZhCn, true) => format!("已确认文本适配器“{adapter_id}”正在运行。"),
            (Self::ZhCn, false) => format!("已确认文本适配器“{adapter_id}”已停止。"),
        }
    }

    pub(crate) fn adapter_runtime_unconfirmed(
        self,
        adapter_id: &str,
        start: bool,
        unavailable: bool,
    ) -> String {
        match (self, start, unavailable) {
            (Self::EnUs, true, false) => format!(
                "Text adapter `{adapter_id}` start request was accepted, but the refreshed state did not confirm it."
            ),
            (Self::EnUs, false, false) => format!(
                "Text adapter `{adapter_id}` stop request was accepted, but the refreshed state did not confirm it."
            ),
            (Self::EnUs, true, true) => format!(
                "Text adapter `{adapter_id}` start request was accepted; current state is unavailable. Refresh daemon status to confirm it."
            ),
            (Self::EnUs, false, true) => format!(
                "Text adapter `{adapter_id}` stop request was accepted; current state is unavailable. Refresh daemon status to confirm it."
            ),
            (Self::ZhCn, true, false) => {
                format!("文本适配器“{adapter_id}”的启动请求已接受，但刷新后的状态未能确认结果。")
            }
            (Self::ZhCn, false, false) => {
                format!("文本适配器“{adapter_id}”的停止请求已接受，但刷新后的状态未能确认结果。")
            }
            (Self::ZhCn, true, true) => format!(
                "文本适配器“{adapter_id}”的启动请求已接受；当前状态不可用。请刷新守护进程状态进行确认。"
            ),
            (Self::ZhCn, false, true) => format!(
                "文本适配器“{adapter_id}”的停止请求已接受；当前状态不可用。请刷新守护进程状态进行确认。"
            ),
        }
    }

    pub(crate) fn provider_script_edited(
        self,
        provider_id: &str,
        path: &str,
        editor: &str,
    ) -> String {
        match self {
            Self::EnUs => format!(
                "Edited managed ASR provider `{provider_id}` script at {path} with {editor}."
            ),
            Self::ZhCn => {
                format!("已使用 {editor} 编辑托管 ASR 提供商“{provider_id}”位于 {path} 的脚本。")
            }
        }
    }

    pub(crate) fn config_save_receipt(
        self,
        path: &str,
        backup: Option<&str>,
        daemon_reload: &str,
    ) -> String {
        self.save_receipt(
            self.text(GuiText::ConfigurationSaved),
            path,
            backup,
            daemon_reload,
        )
    }

    pub(crate) fn model_installed(
        self,
        title: &str,
        model_name: &str,
        checksum_verified: bool,
    ) -> String {
        let checksum = self.text(if checksum_verified {
            GuiText::ChecksumVerified
        } else {
            GuiText::RegistryNoChecksum
        });
        match self {
            Self::EnUs => {
                format!("Installed {title} into managed model `{model_name}` ({checksum}).")
            }
            Self::ZhCn => {
                format!("已将 {title} 安装到托管模型“{model_name}”（{checksum}）。")
            }
        }
    }

    pub(crate) fn model_removed(self, directory: &str) -> String {
        match self {
            Self::EnUs => format!("Removed inactive managed model `{directory}`."),
            Self::ZhCn => format!("已移除未激活的托管模型“{directory}”。"),
        }
    }

    pub(crate) fn model_selected(
        self,
        directory: &str,
        provider_id: &str,
        daemon_reload: &str,
    ) -> String {
        match self {
            Self::EnUs => format!(
                "Selected managed model `{directory}` for ASR provider `{provider_id}`; {daemon_reload}."
            ),
            Self::ZhCn => format!(
                "已为 ASR 提供商“{provider_id}”选择托管模型“{directory}”；{daemon_reload}。"
            ),
        }
    }

    pub(crate) fn script_installed(
        self,
        updated: bool,
        resource: &str,
        entry_id: &str,
        path: &str,
        daemon_reload: &str,
    ) -> String {
        match (self, updated) {
            (Self::EnUs, true) => {
                format!("Updated {resource} `{entry_id}` at {path}; {daemon_reload}.")
            }
            (Self::EnUs, false) => {
                format!("Installed {resource} `{entry_id}` at {path}; {daemon_reload}.")
            }
            (Self::ZhCn, true) => {
                format!("已更新{resource}“{entry_id}”，路径为 {path}；{daemon_reload}。")
            }
            (Self::ZhCn, false) => {
                format!("已安装{resource}“{entry_id}”，路径为 {path}；{daemon_reload}。")
            }
        }
    }

    pub(crate) fn script_removed(
        self,
        resource: &str,
        entry_id: &str,
        script_path: &str,
        cleanup_error: Option<&str>,
        removed_file: bool,
        daemon_reload: &str,
    ) -> String {
        match (self, cleanup_error, removed_file) {
            (Self::EnUs, Some(error), _) => format!(
                "Removed {resource} `{entry_id}` from config; managed script cleanup failed: {error}; {daemon_reload}."
            ),
            (Self::EnUs, None, true) => format!(
                "Removed {resource} `{entry_id}` and managed script {script_path}; {daemon_reload}."
            ),
            (Self::EnUs, None, false) => format!(
                "Removed {resource} `{entry_id}`; managed script was already absent; {daemon_reload}."
            ),
            (Self::ZhCn, Some(error), _) => format!(
                "已从配置中移除{resource}“{entry_id}”；清理托管脚本失败：{error}；{daemon_reload}。"
            ),
            (Self::ZhCn, None, true) => {
                format!("已移除{resource}“{entry_id}”及托管脚本 {script_path}；{daemon_reload}。")
            }
            (Self::ZhCn, None, false) => {
                format!("已移除{resource}“{entry_id}”；托管脚本此前已不存在；{daemon_reload}。")
            }
        }
    }

    pub(crate) fn hotword_path_changed(self, provider_id: &str, set: bool) -> String {
        match (self, set) {
            (Self::EnUs, true) => {
                format!("Updated hotword path for provider `{provider_id}`.")
            }
            (Self::EnUs, false) => {
                format!("Cleared hotword path for provider `{provider_id}`.")
            }
            (Self::ZhCn, true) => format!("已更新提供商“{provider_id}”的热词路径。"),
            (Self::ZhCn, false) => format!("已清除提供商“{provider_id}”的热词路径。"),
        }
    }

    pub(crate) fn script_configuration_completed(
        self,
        resource: &str,
        entry_id: &str,
        path: &str,
        daemon_reload: &str,
    ) -> String {
        match self {
            Self::EnUs => format!(
                "Completed configuration for {resource} `{entry_id}` using the existing script at {path}; {daemon_reload}."
            ),
            Self::ZhCn => format!(
                "已使用位于 {path} 的现有脚本完成{resource}“{entry_id}”的配置；{daemon_reload}。"
            ),
        }
    }

    pub(crate) fn script_removal_worker_failed(self, resource: &str) -> String {
        match self {
            Self::EnUs => format!("{resource} removal worker stopped unexpectedly."),
            Self::ZhCn => format!("{resource}移除 worker 意外停止。"),
        }
    }

    pub(crate) fn registry_selector_required(self, resource: &str) -> String {
        match self {
            Self::EnUs => format!("Choose a {resource} to install."),
            Self::ZhCn => format!("请选择要安装的{resource}。"),
        }
    }

    pub(crate) fn required_environment_value(self, name: &str) -> String {
        match self {
            Self::EnUs => format!(
                "Enter a value for required environment variable `{name}` before installing."
            ),
            Self::ZhCn => format!("安装前请输入必填环境变量“{name}”的值。"),
        }
    }
}

fn normalized_locale(value: &str) -> Option<GuiLocale> {
    let value = value
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('-', "_");
    if value.is_empty() || matches!(value.as_str(), "C" | "POSIX") {
        return None;
    }
    value
        .to_ascii_lowercase()
        .starts_with("zh")
        .then_some(GuiLocale::ZhCn)
        .or(Some(GuiLocale::EnUs))
}

/// Stable daemon action names used by localized result templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DaemonActionName {
    Start,
    Stop,
    Restart,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection_normalizes_legacy_names_and_falls_back_to_english() {
        assert_eq!(GuiLocale::from_name("zh_CN.UTF-8"), GuiLocale::ZhCn);
        assert_eq!(GuiLocale::from_name("zh-Hans@variant"), GuiLocale::ZhCn);
        assert_eq!(GuiLocale::from_name("en_US.UTF-8"), GuiLocale::EnUs);
        assert_eq!(GuiLocale::from_name("C.UTF-8"), GuiLocale::EnUs);
    }

    #[test]
    fn localized_form_templates_preserve_machine_ids_and_raw_details() {
        let scene_id = "scene-machine-id";
        let provider_id = "provider-machine-id";
        let path = "/tmp/config-machine-path";
        let reload = "raw daemon reload detail";

        let english_scene = GuiLocale::EnUs.scene_added(scene_id);
        let chinese_scene = GuiLocale::ZhCn.scene_added(scene_id);
        assert!(english_scene.contains(scene_id));
        assert!(chinese_scene.contains(scene_id));
        assert_ne!(english_scene, chinese_scene);

        let english_provider = GuiLocale::EnUs.asr_provider_changed(true, provider_id);
        let chinese_provider = GuiLocale::ZhCn.asr_provider_changed(true, provider_id);
        assert!(english_provider.contains(provider_id));
        assert!(chinese_provider.contains(provider_id));
        assert_ne!(english_provider, chinese_provider);

        let adapter_id = "adapter-machine-id";
        for locale in [GuiLocale::EnUs, GuiLocale::ZhCn] {
            let llm_summary = locale.llm_provider_changed("add", provider_id);
            let test_summary = locale.llm_provider_test_succeeded(provider_id, 3);
            let adapter_summary = locale.text_adapter_changed(true, adapter_id);
            let receipt = locale.save_receipt(&llm_summary, path, None, reload);
            assert!(llm_summary.contains(provider_id));
            assert!(test_summary.contains(provider_id));
            assert!(test_summary.contains('3'));
            assert!(adapter_summary.contains(adapter_id));
            assert!(receipt.contains(provider_id));
            assert!(receipt.contains(path));
            assert!(receipt.contains(reload));

            let model_progress = locale.model_download_progress("2 MiB", Some("8 MiB"));
            let script_progress = locale.script_progress(
                "downloading",
                locale.text(GuiText::AsrProviderResource),
                Some(2048),
                Some(8192),
            );
            let recovery = locale.script_configuration_completed(
                locale.text(GuiText::TextAdapterResource),
                adapter_id,
                path,
                reload,
            );
            let removal = locale.script_removed(
                locale.text(GuiText::AsrProviderResource),
                provider_id,
                path,
                Some("raw cleanup detail"),
                false,
                reload,
            );
            let edit = locale.provider_script_edited(provider_id, path, "editor-command");
            assert!(model_progress.contains("2 MiB"));
            assert!(model_progress.contains("8 MiB"));
            assert!(script_progress.contains("2048"));
            assert!(script_progress.contains("8192"));
            assert!(recovery.contains(adapter_id));
            assert!(recovery.contains(path));
            assert!(recovery.contains(reload));
            assert!(removal.contains(provider_id));
            assert!(removal.contains("raw cleanup detail"));
            assert!(removal.contains(reload));
            assert!(edit.contains(provider_id));
            assert!(edit.contains(path));
            assert!(edit.contains("editor-command"));
        }
    }

    #[test]
    fn translated_key_set_is_complete_and_nonempty() {
        for locale in [GuiLocale::EnUs, GuiLocale::ZhCn] {
            assert!(
                GuiText::ALL
                    .into_iter()
                    .all(|key| !locale.text(key).is_empty())
            );
        }
        assert!(
            GuiText::ALL
                .into_iter()
                .any(|key| GuiLocale::EnUs.text(key) != GuiLocale::ZhCn.text(key))
        );
    }
}
