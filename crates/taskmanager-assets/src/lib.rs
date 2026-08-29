//! Toolkit-neutral embedded SVG, font, and product identity assets for TaskForest.

#![forbid(unsafe_code)]

use std::borrow::Cow;

/// Stable user-facing product identity.
///
/// The neutral name remains useful for reports and shared copy. Desktop
/// frontends additionally receive distinct names and reverse-DNS IDs so GPUI
/// and Iced can be installed, captured, and debugged side by side.
pub mod product {
    pub const NAME: &str = "TaskForest";
    pub const ZH_NAME: &str = "任务森林";
    pub const GPUI_NAME: &str = "TaskForestG";
    pub const ICED_NAME: &str = "TaskForestI";
    pub const BEVY_NAME: &str = "TaskForestB";
    pub const TAGLINE_EN: &str = "Eye-friendly native system monitor";
    pub const TAGLINE_ZH: &str = "护眼原生系统监视器";
    pub const DESCRIPTION_EN: &str = "An eye-friendly native system monitor for processes, services, devices, and system health.";
    pub const DESCRIPTION_ZH: &str = "面向进程、服务、设备与系统健康的护眼原生系统监视器。";
    pub const GPUI_APP_ID: &str = "io.github.YellowWhiteBlackCat.TaskForestG";
    pub const ICED_APP_ID: &str = "io.github.YellowWhiteBlackCat.TaskForestI";
    pub const BEVY_APP_ID: &str = "io.github.YellowWhiteBlackCat.TaskForestB";
    pub const REPOSITORY_URL: &str = "https://github.com/YellowWhiteBlackCat/TaskForest";
}

/// Edge length of the checked-in, canonical-tray-SVG-derived bitmap.
pub const PRODUCT_TRAY_ICON_SIZE: u32 = 22;

/// TaskForest's RGBA system-tray bitmap, derived from the tray optical master
/// by `packaging/regenerate-icons.sh`.
///
/// Keeping the decoded pixels here lets GPUI and Iced hand identical owned
/// bytes to the native tray adapters without adding a runtime image decoder or
/// maintaining frontend-local placeholder artwork.
#[must_use]
pub fn product_tray_icon_rgba() -> &'static [u8] {
    include_bytes!("../assets/product/taskforest-tray-22.rgba")
}

#[derive(Clone, Copy)]
struct EmbeddedAsset {
    path: &'static str,
    bytes: &'static [u8],
}

macro_rules! asset_group {
    ($assets:ident, $paths:ident, [$($path:literal),+ $(,)?]) => {
        pub const $paths: &[&str] = &[$($path),+];
        const $assets: &[EmbeddedAsset] = &[
            $(EmbeddedAsset {
                path: $path,
                bytes: include_bytes!(concat!("../assets/", $path)),
            }),+
        ];
    };
}

asset_group!(
    COMPONENT_ASSETS,
    UI_ICON_PATHS,
    [
        "icons/a-large-small.svg",
        "icons/arrow-down.svg",
        "icons/arrow-left.svg",
        "icons/arrow-right.svg",
        "icons/arrow-up.svg",
        "icons/asterisk.svg",
        "icons/bell.svg",
        "icons/book-open.svg",
        "icons/bot.svg",
        "icons/building-2.svg",
        "icons/calendar.svg",
        "icons/case-sensitive.svg",
        "icons/chart-pie.svg",
        "icons/check.svg",
        "icons/chevron-down.svg",
        "icons/chevron-left.svg",
        "icons/chevron-right.svg",
        "icons/chevrons-up-down.svg",
        "icons/chevron-up.svg",
        "icons/circle-check.svg",
        "icons/circle-user.svg",
        "icons/circle-x.svg",
        "icons/close.svg",
        "icons/copy.svg",
        "icons/dash.svg",
        "icons/delete.svg",
        "icons/ellipsis.svg",
        "icons/ellipsis-vertical.svg",
        "icons/external-link.svg",
        "icons/eye.svg",
        "icons/eye-off.svg",
        "icons/file.svg",
        "icons/folder.svg",
        "icons/folder-closed.svg",
        "icons/folder-open.svg",
        "icons/frame.svg",
        "icons/gallery-vertical-end.svg",
        "icons/github.svg",
        "icons/globe.svg",
        "icons/heart.svg",
        "icons/heart-off.svg",
        "icons/inbox.svg",
        "icons/info.svg",
        "icons/inspector.svg",
        "icons/layout-dashboard.svg",
        "icons/loader.svg",
        "icons/loader-circle.svg",
        "icons/map.svg",
        "icons/maximize.svg",
        "icons/menu.svg",
        "icons/minimize.svg",
        "icons/minus.svg",
        "icons/moon.svg",
        "icons/palette.svg",
        "icons/panel-bottom.svg",
        "icons/panel-bottom-open.svg",
        "icons/panel-left.svg",
        "icons/panel-left-close.svg",
        "icons/panel-left-open.svg",
        "icons/panel-right.svg",
        "icons/panel-right-close.svg",
        "icons/panel-right-open.svg",
        "icons/plus.svg",
        "icons/redo.svg",
        "icons/redo-2.svg",
        "icons/replace.svg",
        "icons/resize-corner.svg",
        "icons/search.svg",
        "icons/settings.svg",
        "icons/settings-2.svg",
        "icons/sort-ascending.svg",
        "icons/sort-descending.svg",
        "icons/square-terminal.svg",
        "icons/star.svg",
        "icons/star-off.svg",
        "icons/sun.svg",
        "icons/thumbs-down.svg",
        "icons/thumbs-up.svg",
        "icons/triangle-alert.svg",
        "icons/undo.svg",
        "icons/undo-2.svg",
        "icons/user.svg",
        "icons/window-close.svg",
        "icons/window-maximize.svg",
        "icons/window-minimize.svg",
        "icons/window-restore.svg",
    ]
);

asset_group!(
    DOMAIN_ASSETS,
    TASKMANAGER_ICON_PATHS,
    [
        "domain/cpu.svg",
        "domain/memory.svg",
        "domain/disk.svg",
        "domain/network.svg",
        "domain/gpu.svg",
        "domain/process.svg",
        "domain/service.svg",
        "domain/startup.svg",
        "domain/user.svg",
        "domain/health.svg",
        "domain/alert.svg",
        "domain/export.svg",
        "domain/refresh.svg",
        "domain/search.svg",
        "domain/settings.svg",
    ]
);

pub mod icon_path {
    pub const CPU: &str = "domain/cpu.svg";
    pub const MEMORY: &str = "domain/memory.svg";
    pub const DISK: &str = "domain/disk.svg";
    pub const NETWORK: &str = "domain/network.svg";
    pub const GPU: &str = "domain/gpu.svg";
    pub const PROCESS: &str = "domain/process.svg";
    pub const SERVICE: &str = "domain/service.svg";
    pub const STARTUP: &str = "domain/startup.svg";
    pub const USER: &str = "domain/user.svg";
    pub const HEALTH: &str = "domain/health.svg";
    pub const ALERT: &str = "domain/alert.svg";
    pub const EXPORT: &str = "domain/export.svg";
    pub const REFRESH: &str = "domain/refresh.svg";
    pub const SEARCH: &str = "domain/search.svg";
    pub const SETTINGS: &str = "domain/settings.svg";
}

// ── embedded fonts ─────────────────────────────────────────────────────────
// Bundled typefaces (see `assets/fonts/LICENSE.md` for per-font licenses).
// These are NOT part of `all_asset_paths()`: frontends register them directly
// with their own font database.

/// Font asset paths as stored under `assets/`.
pub mod font_path {
    /// Xiaomi MiSans VF variable font (SIL OFL 1.1) — CJK + Latin UI face.
    pub const MISANS_VF: &str = "fonts/MiSansVF.ttf";
    /// Roboto Mono variable font (SIL OFL 1.1) — monospace face.
    pub const ROBOTO_MONO: &str = "fonts/RobotoMono-VF.ttf";
}

const FONT_ASSETS: &[EmbeddedAsset] = &[
    EmbeddedAsset {
        path: font_path::MISANS_VF,
        bytes: include_bytes!("../assets/fonts/MiSansVF.ttf"),
    },
    EmbeddedAsset {
        path: font_path::ROBOTO_MONO,
        bytes: include_bytes!("../assets/fonts/RobotoMono-VF.ttf"),
    },
];

/// All bundled font blobs, ready for registration by a frontend font database.
pub fn embedded_fonts() -> Vec<Cow<'static, [u8]>> {
    FONT_ASSETS
        .iter()
        .map(|asset| Cow::Borrowed(asset.bytes))
        .collect()
}

/// The family names registered by [`embedded_fonts`], as loaded from the
/// fonts' name tables.
pub const EMBEDDED_FONT_FAMILIES: &[&str] = &["MiSans VF", "Roboto Mono"];

pub fn all_asset_paths() -> impl Iterator<Item = &'static str> {
    UI_ICON_PATHS.iter().chain(TASKMANAGER_ICON_PATHS).copied()
}

/// Return the embedded bytes for a known SVG asset path.
pub fn asset_bytes(path: &str) -> Option<&'static [u8]> {
    COMPONENT_ASSETS
        .iter()
        .chain(DOMAIN_ASSETS)
        .find(|asset| asset.path == path)
        .map(|asset| asset.bytes)
}

/// Edge length in pixels of every checked-in UI icon bitmap. The bitmaps
/// rasterize at 2x of the largest logical draw size (18px), so 1x-scale
/// drawing downsamples crisply and 2x-scale drawing is exact.
pub const UI_ICON_RGBA_SIZE: u32 = 36;

/// Checked-in white RGBA bitmaps keyed by the SAME asset path vocabulary as
/// [`asset_bytes`] — `domain/cpu.svg`, `icons/chevron-up.svg`, … — so a
/// frontend resolves one semantic icon through one path string and picks the
/// SVG (vector renderers) or the bitmap (raster renderers) from the same key.
/// Derived only by `packaging/regenerate-ui-icons.sh`; never hand-edited.
const UI_ICON_RGBA: &[(&str, &[u8])] = &[
    (
        icon_path::CPU,
        include_bytes!("../assets/icons-rgba/domain-cpu.rgba"),
    ),
    (
        icon_path::MEMORY,
        include_bytes!("../assets/icons-rgba/domain-memory.rgba"),
    ),
    (
        icon_path::DISK,
        include_bytes!("../assets/icons-rgba/domain-disk.rgba"),
    ),
    (
        icon_path::NETWORK,
        include_bytes!("../assets/icons-rgba/domain-network.rgba"),
    ),
    (
        icon_path::GPU,
        include_bytes!("../assets/icons-rgba/domain-gpu.rgba"),
    ),
    (
        icon_path::PROCESS,
        include_bytes!("../assets/icons-rgba/domain-process.rgba"),
    ),
    (
        icon_path::SERVICE,
        include_bytes!("../assets/icons-rgba/domain-service.rgba"),
    ),
    (
        icon_path::STARTUP,
        include_bytes!("../assets/icons-rgba/domain-startup.rgba"),
    ),
    (
        icon_path::USER,
        include_bytes!("../assets/icons-rgba/domain-user.rgba"),
    ),
    (
        icon_path::ALERT,
        include_bytes!("../assets/icons-rgba/domain-alert.rgba"),
    ),
    (
        icon_path::SEARCH,
        include_bytes!("../assets/icons-rgba/domain-search.rgba"),
    ),
    (
        icon_path::SETTINGS,
        include_bytes!("../assets/icons-rgba/domain-settings.rgba"),
    ),
    (
        "icons/chart-pie.svg",
        include_bytes!("../assets/icons-rgba/icons-chart-pie.rgba"),
    ),
    (
        "icons/info.svg",
        include_bytes!("../assets/icons-rgba/icons-info.rgba"),
    ),
    (
        "icons/layout-dashboard.svg",
        include_bytes!("../assets/icons-rgba/icons-layout-dashboard.rgba"),
    ),
    (
        "icons/arrow-up.svg",
        include_bytes!("../assets/icons-rgba/icons-arrow-up.rgba"),
    ),
    (
        "icons/arrow-down.svg",
        include_bytes!("../assets/icons-rgba/icons-arrow-down.rgba"),
    ),
];

/// Every bitmap key, for registry tests that keep the table and the SVG set
/// honest against each other.
pub const UI_ICON_RGBA_PATHS: &[&str] = &[
    icon_path::CPU,
    icon_path::MEMORY,
    icon_path::DISK,
    icon_path::NETWORK,
    icon_path::GPU,
    icon_path::PROCESS,
    icon_path::SERVICE,
    icon_path::STARTUP,
    icon_path::USER,
    icon_path::ALERT,
    icon_path::SEARCH,
    icon_path::SETTINGS,
    "icons/chart-pie.svg",
    "icons/info.svg",
    "icons/layout-dashboard.svg",
    "icons/arrow-up.svg",
    "icons/arrow-down.svg",
];

/// Return the decoded RGBA bytes for a known icon asset path. Bitmaps are
/// white with alpha, sized [`UI_ICON_RGBA_SIZE`]²; renderers tint at draw
/// time. `None` is a typed fallback for a path with no bitmap yet — callers
/// must degrade honestly, never fabricate a placeholder glyph.
pub fn ui_icon_rgba(path: &str) -> Option<&'static [u8]> {
    UI_ICON_RGBA
        .iter()
        .find(|(key, _)| *key == path)
        .map(|(_, bytes)| *bytes)
}
