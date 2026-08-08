//! Simplified Chinese GUI presentation strings.

use super::GuiText;

pub(super) const fn text(key: GuiText) -> &'static str {
    if (key as u16) <= GuiText::SourceBundledDefault as u16 {
        simplified_chinese_core(key)
    } else if (key as u16) <= GuiText::InvalidUtf8HotwordPath as u16 {
        simplified_chinese_hotwords(key)
    } else if (key as u16) <= GuiText::DetailsOpenedOnHost as u16 {
        simplified_chinese_desktop(key)
    } else if (key as u16) <= GuiText::AdapterLocal as u16 {
        simplified_chinese_resources(key)
    } else if (key as u16) <= GuiText::RemoteTitle as u16 {
        simplified_chinese_forms(key)
    } else if (key as u16) <= GuiText::SaveOrCancelAdapterBeforeRemoval as u16 {
        simplified_chinese_llm_adapter_forms(key)
    } else if (key as u16) <= GuiText::StoppingTextAdapter as u16 {
        simplified_chinese_install(key)
    } else {
        simplified_chinese_install_tail(key)
    }
}

const fn simplified_chinese_core(key: GuiText) -> &'static str {
    match key {
        GuiText::ApplicationTitle => "Vinpst 配置",
        GuiText::Control => "控制",
        GuiText::Resources => "资源",
        GuiText::Llm => "LLM",
        GuiText::Hotwords => "热词",
        GuiText::OpenConfig => "打开配置",
        GuiText::DaemonService => "守护进程",
        GuiText::ReloadConfig => "重新加载配置",
        GuiText::SavingConfiguration => "正在保存…",
        GuiText::StartingRecording => "正在开始录音…",
        GuiText::StoppingRecording => "正在停止录音…",
        GuiText::RecordingStarted => "录音已开始。",
        GuiText::RecordingStopped => "录音已停止；识别结果已交付给前端。",
        GuiText::DaemonLoading => "守护进程：加载中…",
        GuiText::DaemonStatusUnavailable => "守护进程：状态不可用",
        GuiText::RefreshDaemon => "刷新守护进程",
        GuiText::StartDaemon => "启动守护进程",
        GuiText::StopDaemon => "停止守护进程",
        GuiText::RestartDaemon => "重启守护进程",
        GuiText::StartingDaemon => "正在启动守护进程…",
        GuiText::StoppingDaemon => "正在停止守护进程…",
        GuiText::RestartingDaemon => "正在重启守护进程…",
        GuiText::AudioAndVad => "音频",
        GuiText::NormalizeAudio => "自动调整录音音量",
        GuiText::CaptureDevice => "录音设备",
        GuiText::AudioDevicesUnavailable => "无法刷新麦克风列表。",
        GuiText::LockedWhileFinishing => "请先等待当前操作完成",
        GuiText::DuckOutput => "录音时降低输出音量",
        GuiText::EnableVad => "启用语音活动检测",
        GuiText::SaveConfiguration => "保存",
        GuiText::ResetChanges => "重置",
        GuiText::UnsavedChanges => "有未保存的更改",
        GuiText::ConfigurationUpToDate => "已保存",
        GuiText::ConfigDraftUnavailable => "配置草稿不可用。",
        GuiText::SourceBundledDefault => "内置默认值；保存后将创建用户文件",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_hotwords(key: GuiText) -> &'static str {
    match key {
        GuiText::Details => "查看详情",
        GuiText::Ok => "确定",
        GuiText::ErrorDialogTitle => "错误",
        GuiText::NoValidConfig => "未加载有效配置。",
        GuiText::NoHotwordProvider => "没有支持热词文件的本地或命令 ASR 提供商。",
        GuiText::NoHotwordProviderSelected => "未选择支持热词的提供商。",
        GuiText::SaveOrResetHotwordBeforeSelecting => {
            "选择其他文件前，请先保存或重置已编辑的热词内容。"
        }
        GuiText::SaveOrResetHotwordBeforeProvider => "选择其他提供商前，请先保存或重置热词更改。",
        GuiText::SelectedHotwordProviderUnavailable => "所选热词提供商不可用。",
        GuiText::SaveHotwordBeforePathChange => "更改已配置路径前，请先保存已编辑的热词内容。",
        GuiText::HotwordPathCannotBeEmpty => "热词文件路径不能为空。",
        GuiText::SaveHotwordBeforePathClear => "清除已配置路径前，请先保存已编辑的热词内容。",
        GuiText::SaveHotwordBeforeReload => "重新加载前，请先保存已编辑的热词内容。",
        GuiText::SetOrResetPathBeforeLoad => "加载内容前，请先设置或重置热词路径。",
        GuiText::SetPathBeforeLoad => "加载内容前，请先设置热词文件路径。",
        GuiText::LoadingHotwordContent => "正在加载热词内容…",
        GuiText::DiscardedStaleHotwordContent => "已丢弃先前选择对应的过期热词内容。",
        GuiText::LoadedHotwordContent => "已加载配置的热词内容。",
        GuiText::MissingHotwordFileEmptyEditor => "配置的热词文件尚不存在；已加载空编辑器。",
        GuiText::SetOrResetPathBeforeSave => "保存内容前，请先设置或重置热词路径。",
        GuiText::LoadHotwordBeforeSave => "保存内容前，请先加载已配置的热词文件。",
        GuiText::SetPathBeforeSave => "保存内容前，请先设置热词文件路径。",
        GuiText::SavingHotwordContent => "正在保存热词内容…",
        GuiText::NoPendingHotwordActivation => "没有待重试的已保存热词激活。",
        GuiText::SelectPendingHotwordProvider => "重试前，请选择存在待处理热词激活的提供商。",
        GuiText::RetryingHotwordActivation => "正在重试热词激活…",
        GuiText::SettingHotwordPath => "正在设置热词路径…",
        GuiText::ClearingHotwordPath => "正在清除热词路径…",
        GuiText::NoProviderSelected => "未选择提供商",
        GuiText::AsrProvider => "ASR 提供商",
        GuiText::HotwordFile => "热词文件",
        GuiText::HotwordPathPlaceholder => "UTF-8 热词文件路径",
        GuiText::Browse => "浏览…",
        GuiText::SetPath => "使用此文件",
        GuiText::ClearPath => "停用",
        GuiText::LoadContent => "重新加载",
        GuiText::SaveContent => "保存",
        GuiText::RetryActivation => "应用到 ASR",
        GuiText::OneHotwordPerLine => "每行一个热词；可选权重：词语 2.0",
        GuiText::HotwordActivationRetryable => "已保存；请重新应用到 ASR",
        GuiText::UnsavedHotwordContent => "热词内容尚未保存",
        GuiText::HotwordContentUnchanged => "已保存",
        GuiText::LoadConfiguredHotwordFile => "重新加载文件后即可编辑",
        GuiText::SelectHotwordsFile => "选择热词文件",
        GuiText::TextFiles => "文本文件",
        GuiText::AllFiles => "所有文件",
        GuiText::SelectingHotwordFile => "正在选择热词文件…",
        GuiText::SelectedHotwordFile => "已选择热词文件；请选择“使用此文件”验证并应用。",
        GuiText::InvalidUtf8HotwordPath => "所选热词路径不是有效的 UTF-8。",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_desktop(key: GuiText) -> &'static str {
    match key {
        GuiText::OpeningConfig => "正在打开配置文件…",
        GuiText::OpeningNotificationDetails => "正在打开通知详情…",
        GuiText::NoValidConfigLoaded => "未加载有效配置。",
        GuiText::ConfigOpenLaunchFailed => "无法打开配置文件：桌面打开程序无法启动。",
        GuiText::ConfigOpenReaperFailed => "无法打开配置文件：无法安全监管桌面打开程序。",
        GuiText::DetailsOpenLaunchFailed => "无法打开通知详情：桌面打开程序无法启动。",
        GuiText::DetailsOpenReaperFailed => "无法打开通知详情：无法安全监管桌面打开程序。",
        GuiText::ConfigOpened => "已将配置文件交给桌面打开程序。",
        GuiText::ConfigOpenedOnHost => "已将配置文件交给宿主桌面打开程序。",
        GuiText::DetailsOpened => "已将通知详情交给桌面打开程序。",
        GuiText::DetailsOpenedOnHost => "已将通知详情交给宿主桌面打开程序。",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_resources(key: GuiText) -> &'static str {
    match key {
        GuiText::FilterProvidersAndScenes => "筛选场景",
        GuiText::FilterModels => "筛选模型",
        GuiText::ManagedAsrModels | GuiText::Model => "模型",
        GuiText::InstalledModels => "已安装模型",
        GuiText::AvailableModels => "可用模型",
        GuiText::RefreshCatalog => "刷新列表",
        GuiText::LoadingModelCatalog => "正在加载模型列表…",
        GuiText::LoadingCatalog => "正在加载…",
        GuiText::CatalogUnavailable => "无法加载此列表。",
        GuiText::NoCatalogItems => "暂无可用项目。",
        GuiText::NoRegistryModelsAvailable => "没有符合筛选条件的模型。",
        GuiText::Install => "安装",
        GuiText::Update => "更新",
        GuiText::Continue => "继续",
        GuiText::ManagedCommandAsrProviders | GuiText::AsrProviders => "ASR 提供商",
        GuiText::NoManagedModelsInstalled => "尚未安装模型。",
        GuiText::SelectLocalProviderForManagedModel => {
            "请先选择本地 ASR 提供商，再选择已安装模型。"
        }
        GuiText::AddCustomProvider => "添加自定义提供商",
        GuiText::ManagedTextAdapters | GuiText::Adapters => "LLM 适配器",
        GuiText::AddCustomAdapter => "添加自定义适配器",
        GuiText::RefreshRuntime => "刷新状态",
        GuiText::NoTextAdaptersConfigured => "尚未配置 LLM 适配器。",
        GuiText::Remove => "移除",
        GuiText::Edit => "编辑",
        GuiText::EditScript => "编辑脚本",
        GuiText::Start => "启动",
        GuiText::Stop => "停止",
        GuiText::Local => "本地",
        GuiText::Remote => "远程",
        GuiText::Command => "命令",
        GuiText::UnselectedModel => "未选择模型",
        GuiText::Active => "已激活",
        GuiText::Inactive => "未激活",
        GuiText::CommandAdapter => "命令适配器",
        GuiText::RuntimeUnavailable => "状态不可用",
        GuiText::NotReportedByDaemon => "守护进程未报告",
        GuiText::Running => "运行中",
        GuiText::Stopped => "已停止",
        GuiText::CloseDetails => "关闭详情",
        GuiText::ResourceDetailsUnavailable => "资源详情不可用",
        GuiText::StableId => "稳定 ID",
        GuiText::Status => "状态",
        GuiText::Backend => "后端",
        GuiText::Runtime => "运行时",
        GuiText::Family => "系列",
        GuiText::Language => "语言",
        GuiText::DeclaredSize => "声明大小",
        GuiText::RegularFiles => "普通文件",
        GuiText::Supported => "支持",
        GuiText::Installed => "已安装",
        GuiText::Unsupported => "不支持",
        GuiText::NotDeclared => "未声明",
        GuiText::InstallDirectory => "安装目录",
        GuiText::MetadataFile => "元数据文件",
        GuiText::Kind => "类型",
        GuiText::Timeout => "超时",
        GuiText::Endpoint => "端点",
        GuiText::ManagedScript => "托管脚本",
        GuiText::Arguments => "参数",
        GuiText::Environment => "环境变量",
        GuiText::LlmProvider => "LLM 提供商",
        GuiText::TextAdapter => "文本适配器",
        GuiText::Credential => "凭据",
        GuiText::ExtraBodyFields => "额外请求体字段",
        GuiText::ExtensionFields => "扩展字段",
        GuiText::WorkingDirectory => "工作目录",
        GuiText::NotConfigured => "未配置",
        GuiText::Configured => "已配置",
        GuiText::Yes => "是",
        GuiText::No => "否",
        GuiText::AdapterLocal => "适配器/本地",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_forms(key: GuiText) -> &'static str {
    match key {
        GuiText::Scenes => "场景",
        GuiText::AddScene => "添加场景",
        GuiText::Available => "可用",
        GuiText::NoScenesMatch => "没有场景匹配当前筛选条件。",
        GuiText::Use => "使用",
        GuiText::SceneId => "场景 ID",
        GuiText::StableUniqueId => "稳定且唯一的 ID",
        GuiText::LabelField => "标签",
        GuiText::DisplayLabelPlaceholder => "显示标签",
        GuiText::PromptField => "提示词",
        GuiText::OptionalPromptTemplate => "可选提示词模板",
        GuiText::NoProviderClearBinding => "不使用提供商（清除绑定）",
        GuiText::ModelOverride => "模型覆盖",
        GuiText::OptionalModelId => "可选模型 ID",
        GuiText::CandidateCount => "候选数量",
        GuiText::ZeroTo32 => "0 到 32",
        GuiText::TimeoutMsLabel => "超时（毫秒）",
        GuiText::BlankLegacyDefault => "留空使用旧版默认值",
        GuiText::ContextLines => "上下文行数",
        GuiText::UpdateScene => "更新场景",
        GuiText::Cancel => "取消",
        GuiText::SavingSceneConfiguration => "正在保存场景配置…",
        GuiText::SelectingScene => "正在选择场景…",
        GuiText::RemovingScene => "正在移除场景…",
        GuiText::AddCustomAsrProvider => "添加自定义 ASR 提供商",
        GuiText::EditAsrProvider => "编辑 ASR 提供商",
        GuiText::AddProvider => "添加提供商",
        GuiText::UpdateProvider => "更新提供商",
        GuiText::ResetForm => "重置表单",
        GuiText::UnsavedProviderChanges => "提供商更改尚未保存",
        GuiText::ProviderFormUnchanged => "提供商表单未更改",
        GuiText::ProviderId => "提供商 ID",
        GuiText::CustomProviderPlaceholder => "custom-provider",
        GuiText::ProviderType => "提供商类型",
        GuiText::BlankBackendDefault => "留空使用后端默认值",
        GuiText::HotwordsManagedOnPage => "热词路径和内容仍在“热词”页面管理。",
        GuiText::CommandField | GuiText::CommandTitle => "命令",
        GuiText::ProviderCommandPlaceholder => "/path/to/provider",
        GuiText::JsonStringArray => "JSON 字符串数组",
        GuiText::AddVariable => "添加变量",
        GuiText::NoEnvironmentVariables => "没有环境变量。",
        GuiText::VariableName => "变量名",
        GuiText::Value => "值",
        GuiText::SavingAsrProvider => "正在保存 ASR 提供商…",
        GuiText::RemovingAsrProvider => "正在移除 ASR 提供商…",
        GuiText::SaveOrCancelProviderBeforeRemoval => {
            "移除提供商前，请先保存或取消当前 ASR 提供商表单。"
        }
        GuiText::LocalTitle => "本地",
        GuiText::RemoteTitle => "远程",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_llm_adapter_forms(key: GuiText) -> &'static str {
    match key {
        GuiText::ProvidersTitle => "提供商",
        GuiText::TestInput => "测试输入",
        GuiText::TestInputPlaceholder => "简短的连通性测试文本",
        GuiText::DefaultModel => "默认模型",
        GuiText::NoLlmProviders => "没有 LLM 提供商。",
        GuiText::Test => "测试",
        GuiText::BaseUrl => "基础 URL",
        GuiText::BaseUrlPlaceholder => "https://provider.example/v1",
        GuiText::ApiKey => "API 密钥",
        GuiText::OptionalKeyExpression => "可选密钥或环境变量表达式",
        GuiText::ExtraBody => "额外请求体",
        GuiText::MaskedJsonObjectBlank => "已遮蔽的 JSON 对象；留空表示 {}",
        GuiText::TestingLlmProvider => "正在测试 LLM 提供商…",
        GuiText::ConnectivityInputRequired => "LLM 提供商连通性测试输入不能为空。",
        GuiText::SavingLlmProvider => "正在保存 LLM 提供商…",
        GuiText::AddCustomTextAdapter => "添加自定义文本适配器",
        GuiText::EditTextAdapter => "编辑文本适配器",
        GuiText::AdapterId => "适配器 ID",
        GuiText::CustomAdapterPlaceholder => "custom-adapter",
        GuiText::AdapterCommandPlaceholder => "/path/to/adapter",
        GuiText::JsonStringObject => "JSON 字符串对象",
        GuiText::OptionalWorkingDirectory => "可选绝对路径或已配置路径",
        GuiText::AddAdapter => "添加适配器",
        GuiText::UpdateAdapter => "更新适配器",
        GuiText::UnsavedAdapterChanges => "适配器更改尚未保存",
        GuiText::AdapterFormUnchanged => "适配器表单未更改",
        GuiText::SavingTextAdapter => "正在保存文本适配器…",
        GuiText::RemovingTextAdapter => "正在移除文本适配器…",
        GuiText::SaveOrCancelAdapterBeforeRemoval => {
            "移除适配器前，请先保存或取消当前文本适配器表单。"
        }
        _ => unreachable!(),
    }
}

const fn simplified_chinese_install(key: GuiText) -> &'static str {
    match key {
        GuiText::Retry => "重试",
        GuiText::Cancelling => "正在取消…",
        GuiText::Finishing => "正在完成…",
        GuiText::ModelInstallationCancelled => "模型安装已取消。",
        GuiText::PreparingModelInstallation => "正在准备模型安装…",
        GuiText::ResolvingModelCatalog => "正在解析模型目录…",
        GuiText::VerifyingModelChecksum => "正在校验模型校验和…",
        GuiText::WritingModelMetadata => "正在写入模型元数据…",
        GuiText::PublishingModelAtomically => "正在原子发布模型…",
        GuiText::UpdatingConfigurationProgress => "正在保存设置…",
        GuiText::ModelInstallationCompleted => "模型安装已完成。",
        GuiText::ValuesStoredHidden => "这些值将保存在用户配置中，并在诊断信息里隐藏。",
        GuiText::Required => "必填",
        GuiText::Optional => "可选",
        GuiText::EnterEnvironmentValue => "输入环境变量值",
        GuiText::ReusingPublishedScript => "正在复用已发布脚本；不会重新下载。",
        GuiText::ScriptPublishedConfigurationIncomplete => "脚本已安装，但设置未保存",
        GuiText::RecoveryInstructions => {
            "解决外部更改或权限问题后重新加载配置并重试。脚本不会再次下载；关闭恢复面板将保留已发布文件。"
        }
        GuiText::RetryConfigurationUpdate => "重试保存设置",
        GuiText::DismissKeepScript => "关闭（保留脚本）",
        GuiText::ScriptInstallationCancelled => "脚本安装已取消。",
        GuiText::AsrProviderResource => "ASR 提供商",
        GuiText::TextAdapterResource => "文本适配器",
        GuiText::EditingManagedProviderScript => "正在打开提供商脚本…",
        GuiText::StartingTextAdapter => "正在启动文本适配器…",
        GuiText::StoppingTextAdapter => "正在停止文本适配器…",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_install_tail(key: GuiText) -> &'static str {
    match key {
        GuiText::ConfigurationSaved => "配置已保存。",
        GuiText::RemovingModel => "正在移除模型…",
        GuiText::RemovingProvider => "正在移除提供商…",
        GuiText::RemovingAdapter => "正在移除适配器…",
        GuiText::HotwordChangesBlocked => "继续前，请先保存或重置热词更改。",
        GuiText::HotwordActivationNotApplied => {
            "已保存的热词路径配置尚未应用到当前守护进程；可以重试激活。"
        }
        GuiText::ChecksumVerified => "校验和已验证",
        GuiText::RegistryNoChecksum => "校验和不可用",
        GuiText::SelectingModel => "正在选择模型…",
        _ => unreachable!(),
    }
}
