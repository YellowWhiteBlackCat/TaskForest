//! Renderer-local language preference and a minimal dictionary for the
//! surfaces this frontend owns (settings / about / export / health).
//!
//! The choice persists through the shared `Config::language` field ("en" /
//! "zh", G-22): the settings picker writes the token, and
//! [`crate::app::IcedApp::load_config`] applies it (and pins the shared
//! catalog) at startup. The dictionary deliberately covers only the strings
//! this frontend introduced; the pre-existing pages keep their historical
//! English labels. Key names mirror the `locales/*.json` vocabulary where one
//! exists.

/// The two supported UI languages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    En,
    Zh,
}

impl Language {
    /// Every supported language.
    pub const ALL: [Language; 2] = [Language::En, Language::Zh];

    /// Self-name in its own tongue (the universal language-picker
    /// convention: `English` / `中文`).
    pub const fn label(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Zh => "中文",
        }
    }

    /// The persisted `Config::language` token for this language (G-22).
    pub const fn token(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Zh => "zh",
        }
    }

    /// Parse a persisted `Config::language` token. An empty, unknown, or
    /// missing token yields `None` so the caller keeps its default — the
    /// first-launch "no recorded preference" sentinel stays distinct from an
    /// explicit choice (mirrors the core field's `Option<String>` shape).
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "en" => Some(Language::En),
            "zh" => Some(Language::Zh),
            _ => None,
        }
    }
}

/// Dictionary keys for the iced-owned surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Settings,
    Containers,
    Health,
    Export,
    About,
    Close,
    Appearance,
    Skin,
    Mode,
    HighContrast,
    On,
    Off,
    Fonts,
    FontFamily,
    FontChoose,
    Density,
    Units,
    Language,
    Version,
    SystemInfo,
    AlertRules,
    DeviceSummary,
    ExportDone,
    ExportNoData,
    ExportFailed,
    ContainersUnavailable,
    ContainersUnsupported,
    ContainersPermissionDenied,
    ContainersNoContainers,
    ContainersWaiting,
    ContainersHeader,
    HealthWaiting,
    HealthNoData,
    DetailsTitle,
    Confirm,
    Cancel,
    // Chooser pill labels (GPUI parity: the settings pills must translate
    // under Zh exactly like the shared catalog renders the section titles).
    Light,
    Dark,
    EyeForest,
    System,
    Bundled,
    Comfortable,
    Compact,
    Bytes,
    Bits,
    Default,
    Subpixel,
    Grayscale,
    RememberLast,
    Performance,
    Applications,
}

impl Key {
    /// The stable key string (matches the `locales/*.json` spelling where
    /// one exists).
    pub const fn code(self) -> &'static str {
        match self {
            Key::Settings => "settings.title",
            Key::Containers => "containers.title",
            Key::Health => "health.title",
            Key::Export => "export.title",
            Key::About => "about.title",
            Key::Close => "chrome.close",
            Key::Appearance => "settings.appearance",
            Key::Skin => "settings.skin",
            Key::Mode => "settings.mode",
            Key::HighContrast => "settings.high_contrast",
            Key::On => "settings.on",
            Key::Off => "settings.off",
            Key::Fonts => "settings.fonts",
            Key::FontFamily => "settings.font_family",
            Key::FontChoose => "settings.font_choose",
            Key::Density => "settings.density",
            Key::Units => "settings.units",
            Key::Language => "settings.language",
            Key::Version => "about.version",
            Key::SystemInfo => "about.system_info",
            Key::AlertRules => "health.alert_rules",
            Key::DeviceSummary => "health.device_summary",
            Key::ExportDone => "export.done",
            Key::ExportNoData => "export.no_data",
            Key::ExportFailed => "export.failed",
            Key::ContainersUnavailable => "containers.unavailable",
            Key::ContainersUnsupported => "containers.unsupported",
            Key::ContainersPermissionDenied => "containers.permission_denied",
            Key::ContainersNoContainers => "containers.no_containers",
            Key::ContainersWaiting => "containers.waiting",
            Key::ContainersHeader => "containers.header",
            Key::HealthWaiting => "health.waiting",
            Key::HealthNoData => "health.no_data",
            Key::DetailsTitle => "details.title",
            Key::Confirm => "chrome.confirm",
            Key::Cancel => "chrome.cancel",
            Key::Light => "settings.light",
            Key::Dark => "settings.dark",
            Key::EyeForest => "settings.eyeforest",
            Key::System => "settings.system",
            Key::Bundled => "settings.bundled",
            Key::Comfortable => "settings.comfortable",
            Key::Compact => "settings.compact",
            Key::Bytes => "settings.bytes",
            Key::Bits => "settings.bits",
            Key::Default => "settings.default",
            Key::Subpixel => "settings.subpixel",
            Key::Grayscale => "settings.grayscale",
            Key::RememberLast => "settings.remember_last",
            Key::Performance => "settings.startup_performance",
            Key::Applications => "settings.startup_processes",
        }
    }
}

/// English rendering of every key.
const fn en(key: Key) -> &'static str {
    match key {
        Key::Settings => "Settings",
        Key::Containers => "Containers",
        Key::Health => "Health",
        Key::Export => "Export",
        Key::About => "About",
        Key::Close => "Close",
        Key::Appearance => "Appearance",
        Key::Skin => "Skin",
        Key::Mode => "Mode",
        Key::HighContrast => "High contrast",
        Key::On => "On",
        Key::Off => "Off",
        Key::Fonts => "Fonts",
        Key::FontFamily => "Installed family",
        Key::FontChoose => "Choose an installed font…",
        Key::Density => "Row density",
        Key::Units => "Memory units",
        Key::Language => "Language",
        Key::Version => "Version",
        Key::SystemInfo => "System information",
        Key::AlertRules => "Alert rules",
        Key::DeviceSummary => "Device summary",
        Key::ExportDone => "Snapshot exported",
        Key::ExportNoData => "No snapshot data to export yet",
        Key::ExportFailed => "Export failed",
        Key::ContainersUnavailable => "Container rollup unavailable",
        Key::ContainersUnsupported => "Containers unsupported on this host",
        Key::ContainersPermissionDenied => "Container collection denied",
        Key::ContainersNoContainers => "No containers running",
        Key::ContainersWaiting => "Waiting for the container rollup…",
        Key::ContainersHeader => "Per-container CPU and memory rollup",
        Key::HealthWaiting => "Waiting for telemetry…",
        Key::HealthNoData => "No health facts reported yet",
        Key::DetailsTitle => "Process details",
        Key::Confirm => "Confirm",
        Key::Cancel => "Cancel",
        Key::Light => "Light",
        Key::Dark => "Dark",
        Key::EyeForest => "EyeForest",
        Key::System => "System",
        Key::Bundled => "Bundled",
        Key::Comfortable => "Comfortable",
        Key::Compact => "Compact",
        Key::Bytes => "Bytes",
        Key::Bits => "Bits",
        Key::Default => "Default",
        Key::Subpixel => "Subpixel",
        Key::Grayscale => "Grayscale",
        Key::RememberLast => "Remember last",
        Key::Performance => "Performance",
        Key::Applications => "Applications",
    }
}

/// Chinese rendering of the localized keys.
const fn zh(key: Key) -> &'static str {
    match key {
        Key::Settings => "设置",
        Key::Containers => "容器",
        Key::Health => "健康",
        Key::Export => "导出",
        Key::About => "关于",
        Key::Close => "关闭",
        Key::Appearance => "外观",
        Key::Skin => "皮肤",
        Key::Mode => "模式",
        Key::HighContrast => "高对比度",
        Key::On => "开",
        Key::Off => "关",
        Key::Fonts => "字体",
        Key::FontFamily => "已安装字体",
        Key::FontChoose => "选择已安装字体…",
        Key::Density => "行密度",
        Key::Units => "内存单位",
        Key::Language => "语言",
        Key::Version => "版本",
        Key::SystemInfo => "系统信息",
        Key::AlertRules => "告警规则",
        Key::DeviceSummary => "设备摘要",
        Key::ExportDone => "快照已导出",
        Key::ExportNoData => "尚无快照数据可导出",
        Key::ExportFailed => "导出失败",
        Key::ContainersUnavailable => "容器汇总不可用",
        Key::ContainersUnsupported => "此主机不支持容器",
        Key::ContainersPermissionDenied => "容器采集被拒绝",
        Key::ContainersNoContainers => "没有正在运行的容器",
        Key::ContainersWaiting => "等待容器汇总…",
        Key::ContainersHeader => "按容器的 CPU 与内存汇总",
        Key::HealthWaiting => "等待遥测数据…",
        Key::HealthNoData => "暂无健康数据",
        Key::DetailsTitle => "进程详情",
        Key::Confirm => "确认",
        Key::Cancel => "取消",
        Key::Light => "浅色",
        Key::Dark => "深色",
        Key::EyeForest => "护眼森林",
        Key::System => "跟随系统",
        Key::Bundled => "内置",
        Key::Comfortable => "舒适",
        Key::Compact => "紧凑",
        Key::Bytes => "字节",
        Key::Bits => "比特",
        Key::Default => "默认",
        Key::Subpixel => "次像素",
        Key::Grayscale => "灰度",
        Key::RememberLast => "记住上次",
        Key::Performance => "性能",
        Key::Applications => "进程",
    }
}

/// Translate one key into the active language. Keys without a translation
/// fall back to English so no label can render empty.
#[must_use]
pub fn t(language: Language, key: Key) -> &'static str {
    match language {
        Language::En => en(key),
        Language::Zh => zh(key),
    }
}

/// Mirror the renderer-local language choice into the shared i18n global so
/// the shared-page body strings resolved via
/// `taskmanager_application::i18n::t` follow the same language this frontend
/// renders. Called on construction (to pin the shared default to this
/// frontend's default [`Language::En`]) and on every language settings change;
/// without it a host whose `LANG` implies Chinese would render the shared-page
/// body in Chinese while the iced-owned modal chrome stays English.
pub fn sync_shared_language(language: Language) {
    let shared = match language {
        Language::En => taskmanager_application::i18n::Language::En,
        Language::Zh => taskmanager_application::i18n::Language::Zh,
    };
    taskmanager_application::i18n::set_language(shared);
}

#[cfg(test)]
#[path = "../tests/gui/i18n_tests.rs"]
mod tests;
