//! [`IconId`] → embedded SVG asset path resolution.
//!
//! Domain icons resolve to the `domain/*.svg` set; the remaining semantic ids
//! share the generic UI-chrome glyphs from the `icons/*.svg` set (the same
//! glyphs the old `gpui_component::IconName` registry shipped).

use taskmanager_assets::icon_path;
use taskmanager_ui_contract::IconId;

/// Resolve a semantic icon to the embedded asset path used by GPUI.
#[must_use]
pub const fn path(icon: IconId) -> &'static str {
    match icon {
        IconId::Cpu => icon_path::CPU,
        IconId::Memory => icon_path::MEMORY,
        IconId::Disk => icon_path::DISK,
        IconId::Network => icon_path::NETWORK,
        IconId::Gpu => icon_path::GPU,
        IconId::Process | IconId::Applications => icon_path::PROCESS,
        IconId::Service | IconId::Services => icon_path::SERVICE,
        IconId::Startup => icon_path::STARTUP,
        IconId::User | IconId::Users => icon_path::USER,
        IconId::Health => icon_path::HEALTH,
        IconId::Alert => icon_path::ALERT,
        IconId::Export => icon_path::EXPORT,
        IconId::Settings => icon_path::SETTINGS,
        IconId::Search => icon_path::SEARCH,
        IconId::More => "icons/ellipsis.svg",
        IconId::Refresh => icon_path::REFRESH,
        IconId::Performance => "icons/chart-pie.svg",
        IconId::System | IconId::Properties => "icons/info.svg",
        IconId::NavigateUp => "icons/arrow-up.svg",
        IconId::NavigateDown => "icons/arrow-down.svg",
        IconId::Focus => "icons/frame.svg",
        IconId::EndTask | IconId::Close | IconId::CircleX => "icons/circle-x.svg",
        IconId::Pause => "icons/dash.svg",
        IconId::Sidebar => "icons/panel-left.svg",
        IconId::CircleCheck => "icons/circle-check.svg",
        IconId::TriangleAlert => "icons/triangle-alert.svg",
        // App-history: a dedicated glyph asset is not bundled, so the page
        // borrows the analytics-dashboard glyph — visually distinct from every
        // other nav-tab icon (chart-pie/process/service/startup/user/info) and
        // thematic for a per-app resource-trend page. The semantic identity
        // stays `IconId::History` (declared in ui-contract); only the GPUI
        // raster fallback reuses an existing tintable SVG.
        IconId::History => "icons/layout-dashboard.svg",
    }
}

/// Return the embedded SVG bytes for a semantic icon.
///
/// The path table and the asset bundle are checked together by the registry
/// tests. `None` is retained as a typed fallback for a future optional asset;
/// renderers must not turn an asset lookup failure into a panic.
#[must_use]
pub fn asset_bytes(icon: IconId) -> Option<&'static [u8]> {
    taskmanager_assets::asset_bytes(path(icon))
}

#[cfg(test)]
#[path = "../tests/headless/icon_registry.rs"]
mod tests;
