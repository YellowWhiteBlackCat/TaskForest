//! Small-window policy shared by the production window and headless tests.

use crate::gpui_app::elements;
use crate::gpui_app::sidebar::{
    NetworkVisibility, SelectedDevice, ordered_indices, visible_with_override,
};
use crate::gpui_app::theme::mono_font_with_fallback;
use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Pixels, Size,
    StatefulInteractiveElement, Styled, div, px, size,
};
use taskmanager_application::i18n;
use taskmanager_core::core::config::SidebarDeviceOverrideConfig;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::{PowerSupplySnapshot, SensorCenterSnapshot, SensorQuantity};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use super::{NavOrientation, RootView};

pub(crate) mod device_strip;

pub const MIN_WIDTH: f32 = 720.0;
pub const MIN_HEIGHT: f32 = 480.0;

/// Navigation is a layout region, not a collection of independently sized
/// controls. These widths are the only rail-width decisions; the page body
/// always owns the remaining space through a flex child with `min_w(0)`.
pub const NAV_RAIL_COMPACT_WIDTH: f32 = 54.0;
pub const NAV_RAIL_WIDTH: f32 = 144.0;
/// Padding around the vertical navigation rail (`8px` leading + `4px`
/// trailing). It belongs to navigation's outer slot and must be deducted
/// before a page profile is classified.
pub const NAVIGATION_HORIZONTAL_INSET: f32 = 12.0;
const ULTRA_COMPACT_CONTENT_WIDTH: f32 = 840.0;
/// Width at or above which a page column admits multi-line prose detail
/// cards: below this the column wraps prose hard enough that such cards must
/// degrade to their compact (bar-only) form regardless of height.
pub const COMPACT_CONTENT_WIDTH: f32 = 1080.0;
const WIDE_CONTENT_WIDTH: f32 = 1600.0;
// These thresholds describe the page slot, not the outer window. The root
// shell spends roughly 50px on navigation before a page sees any height, so a
// 780px window still has a normal page canvas while a 540px window remains
// genuinely constrained.
const CONSTRAINED_CONTENT_HEIGHT: f32 = 640.0;
const GENEROUS_CONTENT_HEIGHT: f32 = 900.0;
const NAVIGATION_VERTICAL_INSET: f32 = 14.0;
const ACTIVE_ALERT_HEIGHT: f32 = 32.0;

/// Shared width contracts for the Performance page's semantic slots.
///
/// These are allocation floors and ceilings, not card dimensions. The root
/// budget uses them to decide whether a device sidebar and statistics rail can
/// coexist with a readable main viewport; the renderer then only consumes the
/// resulting widths.
pub const PERFORMANCE_MAIN_MIN_WIDTH: f32 = 360.0;
pub const PERFORMANCE_STATS_MIN_WIDTH: f32 = 236.0;
pub const PERFORMANCE_STATS_MAX_WIDTH: f32 = 280.0;
pub const PERFORMANCE_SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub const PERFORMANCE_SIDEBAR_MAX_WIDTH: f32 = 460.0;
pub const PERFORMANCE_SLOT_GAP: f32 = 12.0;
/// Minimum content height for a stacked Performance details rail. Below this
/// the main headline plus the fixed 220px rail cannot both remain readable,
/// so the typed width fallback is Hidden rather than allowing a bottom clip.
const PERFORMANCE_STACKED_DETAILS_MIN_HEIGHT: f32 = 520.0;

/// Horizontal layout capacity shared by every GPUI page.
///
/// Height is deliberately not folded into this enum. A very wide, short
/// window still has enough horizontal room to combine page chrome even when
/// its vertical budget requires secondary content to collapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutProfile {
    UltraCompact,
    Compact,
    Standard,
    Wide,
}

/// Independent vertical capacity. Page-specific projections use this axis to
/// collapse secondary regions without discarding available horizontal space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerticalSpace {
    Constrained,
    Standard,
    Generous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationPresentation {
    IconOnly,
    Labeled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceNavigationPresentation {
    Strip,
    Sidebar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceDetailsPresentation {
    Hidden,
    Pinned,
    /// The detail rail moves below the main viewport when horizontal capacity
    /// cannot support two readable columns. This keeps the information
    /// available without starving the primary graph.
    Stacked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceChartInventory {
    AggregateOnly,
    Full,
}

/// The Performance page's typed vertical degradation ladder — the height
/// axis of the minimum-space doctrine (ADR-039). Each rung names exactly
/// which fixed obligations of the page may still render; the renderer
/// consumes the rung, never the raw pixel height or the coarse page-wide
/// [`VerticalSpace`] classification.
///
/// Ordering is the degradation order: content drops from Charts down to
/// Floor, and the headline card's tier floor is never touched (that is the
/// window layer's guarantee, not this ladder's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerformanceVerticalRunway {
    /// Minimum viable page: title row + headline card only. The header band
    /// and per-chart summary rows are dropped first — explicitly, not by
    /// silent clipping.
    Floor,
    /// The core stack composes: title row + header band (readouts /
    /// composition) + headline card with its summary row. Secondary charts
    /// and below content are dropped so the fixed viewport cannot clip them.
    Core,
    /// The full chart inventory fits: the core stack plus secondary charts
    /// and the per-core matrix. This is the rung `chart_inventory` requires
    /// from the height axis.
    Charts,
}

impl PerformanceVerticalRunway {
    /// Classify from the page-slot content height. The Charts threshold is
    /// the shared Constrained page threshold; the Core threshold is the
    /// composed core-stack floor (title + header band + headline floor +
    /// summary + gaps).
    ///
    /// The Charts threshold is deliberately LOWER than the coarse
    /// [`VerticalSpace::Constrained`] boundary (640): the rung only admits
    /// the full inventory — whether a band actually composes is decided by
    /// the per-page numeric fit checks against `content_height`. A normal
    /// 1280x720 window (content ~628) must keep its full chart inventory.
    #[must_use]
    pub const fn for_content_height(height: f32) -> Self {
        if height >= PERFORMANCE_RUNWAY_CHARTS_HEIGHT {
            Self::Charts
        } else if height >= PERFORMANCE_RUNWAY_CORE_FLOOR {
            Self::Core
        } else {
            Self::Floor
        }
    }

    /// Whether the core stack (header band + summaries) may render.
    #[must_use]
    pub const fn carries_core_stack(self) -> bool {
        matches!(self, Self::Core | Self::Charts)
    }

    /// Whether a page may admit content below its headline surface. Only the
    /// full Charts runway has enough named capacity for that optional band;
    /// Core is intentionally headline-only, which makes the no-scroll
    /// Performance contract explicit at the shared boundary.
    #[must_use]
    pub const fn carries_below(self) -> bool {
        matches!(self, Self::Charts)
    }
}

/// Content height the core stack needs: title row + header band + headline
/// tier floor + summary row + the shared gaps.
pub const PERFORMANCE_RUNWAY_CORE_FLOOR: f32 = 380.0;

/// Content height at which the full chart inventory is ADMITTED (the rung
/// gate only). Deliberately below the coarse Constrained boundary (640): a
/// normal 1280x720 window lands at ~628 content height and must keep its
/// charts; the per-page numeric fit checks decide actual composition.
const PERFORMANCE_RUNWAY_CHARTS_HEIGHT: f32 = 520.0;

/// Fold the two typed axes into the one chart-inventory product bit: the
/// width axis admits the full inventory only from the Compact profile up,
/// the height axis needs the Charts runway. One bit out, two named reasons
/// in — no axis conflation at the derivation site.
const fn chart_inventory_from_axes(
    profile: LayoutProfile,
    vertical: PerformanceVerticalRunway,
) -> PerformanceChartInventory {
    match (profile, vertical) {
        (
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide,
            PerformanceVerticalRunway::Charts,
        ) => PerformanceChartInventory::Full,
        (LayoutProfile::UltraCompact, _)
        | (_, PerformanceVerticalRunway::Core | PerformanceVerticalRunway::Floor) => {
            PerformanceChartInventory::AggregateOnly
        }
    }
}

/// Typed Performance-page allocation derived once at the viewport boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformancePageBudget {
    pub device_navigation: DeviceNavigationPresentation,
    pub details: PerformanceDetailsPresentation,
    pub chart_inventory: PerformanceChartInventory,
    /// The typed height-axis degradation ladder (see
    /// [`PerformanceVerticalRunway`]). `chart_inventory` is one product bit
    /// derived from BOTH axes; this field keeps the height reason typed and
    /// consumable on its own.
    pub vertical: PerformanceVerticalRunway,
    /// Numeric page-slot content height backing `vertical`. The coarse rung
    /// gates whether a band MAY render; per-page policies additionally check
    /// this number against the band's summed minimum heights to decide
    /// whether it DOES (a band that cannot meet its floors hides whole, it
    /// never squeezes). `0.0` = unknown (the legacy constructor) — policies
    /// must fall back to the rung-only behavior then.
    pub content_height: f32,
    /// Content inset before the pinned details divider. The outer PageFrame
    /// trailing inset remains zero so the pinned rail itself reaches the edge.
    pub main_trailing_inset: f32,
    /// Effective device-sidebar width for this frame. Persisted user width is
    /// treated as a preference and is clamped by the available page slot.
    pub sidebar_width: f32,
    /// Effective statistics-rail width for this frame.
    pub stats_width: f32,
    /// Width left for the primary Performance viewport after the top-level
    /// slot allocation. This is telemetry for tests and future renderers; the
    /// flex tree remains the final geometry authority.
    pub main_width: f32,
    /// Width of the Performance workspace before its internal slots.
    pub workspace_width: f32,
}

impl PerformancePageBudget {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let device_navigation = match layout.profile {
            LayoutProfile::UltraCompact => DeviceNavigationPresentation::Strip,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                DeviceNavigationPresentation::Sidebar
            }
        };
        let details = match layout.profile {
            LayoutProfile::UltraCompact => PerformanceDetailsPresentation::Hidden,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                PerformanceDetailsPresentation::Pinned
            }
        };
        // Per-axis derivation: the width axis admits the full chart
        // inventory only from the Compact profile up, the height axis needs
        // the Charts runway. Two typed reasons in, one product bit out —
        // the legacy constructor has no pixel height, so the coarse
        // classification maps Constrained to the Core runway.
        let vertical = match layout.vertical_space {
            VerticalSpace::Constrained => PerformanceVerticalRunway::Core,
            VerticalSpace::Standard | VerticalSpace::Generous => PerformanceVerticalRunway::Charts,
        };
        let chart_inventory = chart_inventory_from_axes(layout.profile, vertical);
        Self {
            device_navigation,
            details,
            chart_inventory,
            vertical,
            content_height: 0.0,
            main_trailing_inset: layout.page_padding,
            sidebar_width: match device_navigation {
                DeviceNavigationPresentation::Strip => 0.0,
                DeviceNavigationPresentation::Sidebar => 260.0,
            },
            stats_width: PERFORMANCE_STATS_MAX_WIDTH,
            main_width: 0.0,
            workspace_width: 0.0,
        }
    }

    /// Allocate Performance's semantic slots from the actual page frame.
    ///
    /// The Performance PageFrame deliberately removes its outer trailing
    /// padding so the pinned statistics rail can meet the page edge. Restore
    /// that one padding value here before calculating the inner workspace; all
    /// other pages keep the ordinary two-sided [`ContentBudget`] width.
    #[must_use]
    pub fn from_frame(
        frame: FrameBudget,
        sidebar_visible: bool,
        persisted_sidebar_width: f32,
    ) -> Self {
        let workspace_width =
            (f32::from(frame.content.size.width) + frame.content.page_padding).max(0.0);
        let inset = frame.content.page_padding.min(PERFORMANCE_SLOT_GAP);
        let profile = frame.content.profile;
        let main_min = match profile {
            LayoutProfile::UltraCompact => 320.0,
            LayoutProfile::Compact => PERFORMANCE_MAIN_MIN_WIDTH,
            LayoutProfile::Standard | LayoutProfile::Wide => PERFORMANCE_MAIN_MIN_WIDTH,
        };

        // A sidebar is admitted only when the page can preserve all three
        // meaningful slots: device navigation, a readable main viewport, and
        // the minimum useful statistics rail. Otherwise the same devices move
        // to the strip without changing the page or the user's preference.
        let can_pin_stats_with_sidebar = workspace_width
            >= PERFORMANCE_SIDEBAR_MIN_WIDTH + PERFORMANCE_STATS_MIN_WIDTH + main_min + inset;
        let device_navigation = if sidebar_visible && can_pin_stats_with_sidebar {
            DeviceNavigationPresentation::Sidebar
        } else {
            DeviceNavigationPresentation::Strip
        };
        let sidebar_width = match device_navigation {
            DeviceNavigationPresentation::Strip => 0.0,
            DeviceNavigationPresentation::Sidebar => {
                let max_width = (workspace_width - PERFORMANCE_STATS_MIN_WIDTH - main_min - inset)
                    .clamp(PERFORMANCE_SIDEBAR_MIN_WIDTH, PERFORMANCE_SIDEBAR_MAX_WIDTH);
                let preferred = if persisted_sidebar_width.is_finite() {
                    persisted_sidebar_width
                } else {
                    PERFORMANCE_SIDEBAR_MIN_WIDTH
                };
                preferred.clamp(PERFORMANCE_SIDEBAR_MIN_WIDTH, max_width)
            }
        };
        let remaining = (workspace_width - sidebar_width).max(0.0);
        let stats_capacity = remaining - main_min - inset;
        let width_details = if stats_capacity >= PERFORMANCE_STATS_MIN_WIDTH {
            PerformanceDetailsPresentation::Pinned
        } else if remaining >= main_min {
            PerformanceDetailsPresentation::Stacked
        } else {
            PerformanceDetailsPresentation::Hidden
        };
        let details = if width_details == PerformanceDetailsPresentation::Stacked
            && f32::from(frame.content.size.height) < PERFORMANCE_STACKED_DETAILS_MIN_HEIGHT
        {
            PerformanceDetailsPresentation::Hidden
        } else {
            width_details
        };
        let stats_width = if details == PerformanceDetailsPresentation::Pinned {
            stats_capacity.clamp(PERFORMANCE_STATS_MIN_WIDTH, PERFORMANCE_STATS_MAX_WIDTH)
        } else {
            PERFORMANCE_STATS_MIN_WIDTH
        };
        let main_width = match details {
            PerformanceDetailsPresentation::Pinned => (remaining - stats_width - inset).max(0.0),
            PerformanceDetailsPresentation::Stacked | PerformanceDetailsPresentation::Hidden => {
                remaining
            }
        };
        // Height axis, typed from the real pixel height (the coarse
        // `vertical_space` classification no longer carries this decision
        // alone); `chart_inventory` folds both axes into the one product bit.
        let vertical =
            PerformanceVerticalRunway::for_content_height(f32::from(frame.content.size.height));
        let chart_inventory = chart_inventory_from_axes(profile, vertical);

        Self {
            device_navigation,
            details,
            chart_inventory,
            vertical,
            content_height: f32::from(frame.content.size.height).max(0.0),
            main_trailing_inset: if details == PerformanceDetailsPresentation::Pinned {
                inset
            } else {
                0.0
            },
            sidebar_width,
            stats_width,
            main_width,
            workspace_width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemSurfacePresentation {
    SingleColumn,
    MultiColumn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemPageBudget {
    pub surfaces: SystemSurfacePresentation,
}

impl SystemPageBudget {
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        let surfaces = match layout.profile {
            LayoutProfile::UltraCompact => SystemSurfacePresentation::SingleColumn,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                SystemSurfacePresentation::MultiColumn
            }
        };
        Self { surfaces }
    }

    #[must_use]
    pub fn from_frame(frame: FrameBudget) -> Self {
        Self::from_page_layout(frame.page_layout())
    }
}

/// The page's actual content slot after navigation and shell chrome have been
/// removed. This is the only horizontal/vertical capacity input that page
/// policy should consume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentBudget {
    pub size: Size<Pixels>,
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
}

impl ContentBudget {
    fn from_body(body: Size<Pixels>) -> Self {
        let body_width = f32::from(body.width).max(0.0);
        // Resolve the profile against the post-padding width. Iterate because
        // the padding token itself is profile-dependent at the boundary.
        let mut profile = layout_profile(size(px(body_width), px(0.0)));
        for _ in 0..3 {
            let padding = page_padding(profile);
            let content_width = (body_width - padding * 2.0).max(0.0);
            let next = layout_profile(size(px(content_width), px(0.0)));
            if next == profile {
                let content_height = (f32::from(body.height) - padding * 2.0).max(0.0);
                return Self {
                    size: size(px(content_width), px(content_height)),
                    profile,
                    vertical_space: vertical_space(size(px(content_width), px(content_height))),
                    page_padding: padding,
                };
            }
            profile = next;
        }
        let padding = page_padding(profile);
        let content_width = (body_width - padding * 2.0).max(0.0);
        let content_height = (f32::from(body.height) - padding * 2.0).max(0.0);
        Self {
            size: size(px(content_width), px(content_height)),
            profile,
            vertical_space: vertical_space(size(px(content_width), px(content_height))),
            page_padding: padding,
        }
    }
}

/// Root-level geometry projection. The root computes this once per render and
/// passes the immutable value to every page. It intentionally contains no
/// page state, persistence, or toolkit widgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudget {
    pub viewport: Size<Pixels>,
    pub body: Size<Pixels>,
    pub content: ContentBudget,
    pub navigation: NavigationPresentation,
    pub navigation_width: f32,
}

/// Shell dimensions supplied by the root renderer. The native titlebar is not
/// part of the GPUI viewport, so `has_csd_titlebar` controls whether it is
/// deducted; the horizontal app navigation strip is always deducted when it
/// is present.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameChromeBudget {
    pub titlebar_height: f32,
    pub has_csd_titlebar: bool,
    pub alert_height: f32,
}

impl FrameChromeBudget {
    #[must_use]
    pub fn new(titlebar_height: f32, has_csd_titlebar: bool, alert_visible: bool) -> Self {
        Self {
            titlebar_height: titlebar_height.max(0.0),
            has_csd_titlebar,
            alert_height: if alert_visible {
                ACTIVE_ALERT_HEIGHT
            } else {
                0.0
            },
        }
    }
}

impl FrameBudget {
    /// Build the production root allocation after shell chrome is known.
    #[must_use]
    pub fn for_root(
        viewport: Size<Pixels>,
        orientation: NavOrientation,
        chrome: FrameChromeBudget,
    ) -> Self {
        let navigation = navigation_for_width(viewport, orientation);
        let navigation_width = if matches!(orientation, NavOrientation::Vertical) {
            nav_rail_width(navigation) + NAVIGATION_HORIZONTAL_INSET
        } else {
            0.0
        };
        let horizontal_navigation_height = if matches!(orientation, NavOrientation::Horizontal) {
            chrome.titlebar_height + NAVIGATION_VERTICAL_INSET
        } else {
            0.0
        };
        let shell_height = if chrome.has_csd_titlebar {
            chrome.titlebar_height
        } else {
            0.0
        } + chrome.alert_height
            + horizontal_navigation_height;
        let body = size(
            px((f32::from(viewport.width) - navigation_width).max(0.0)),
            px((f32::from(viewport.height) - shell_height).max(0.0)),
        );
        Self {
            viewport,
            body,
            content: ContentBudget::from_body(body),
            navigation,
            navigation_width,
        }
    }

    /// Build the content-only projection used by legacy/headless callers that
    /// do not model the root chrome. Production rendering uses [`Self::for_root`].
    #[must_use]
    pub fn for_content(viewport: Size<Pixels>, orientation: NavOrientation) -> Self {
        let navigation = navigation_for_width(viewport, orientation);
        let navigation_width = if matches!(orientation, NavOrientation::Vertical) {
            nav_rail_width(navigation) + NAVIGATION_HORIZONTAL_INSET
        } else {
            0.0
        };
        let body = size(
            px((f32::from(viewport.width) - navigation_width).max(0.0)),
            viewport.height,
        );
        Self {
            viewport,
            body,
            content: ContentBudget::from_body(body),
            navigation,
            navigation_width,
        }
    }

    #[must_use]
    pub const fn page_layout(self) -> PageLayoutBudget {
        PageLayoutBudget {
            profile: self.content.profile,
            vertical_space: self.content.vertical_space,
            page_padding: self.content.page_padding,
            navigation: self.navigation,
        }
    }
}

fn page_padding(profile: LayoutProfile) -> f32 {
    match profile {
        LayoutProfile::UltraCompact => 8.0,
        LayoutProfile::Compact => 12.0,
        LayoutProfile::Standard | LayoutProfile::Wide => 16.0,
    }
}

fn navigation_for_width(
    viewport: Size<Pixels>,
    orientation: NavOrientation,
) -> NavigationPresentation {
    let width = f32::from(viewport.width).max(0.0);
    match orientation {
        NavOrientation::Horizontal => {
            if ContentBudget::from_body(size(px(width), px(0.0))).profile
                == LayoutProfile::UltraCompact
            {
                NavigationPresentation::IconOnly
            } else {
                NavigationPresentation::Labeled
            }
        }
        NavOrientation::Vertical => {
            let labeled_body = size(
                px((width - NAV_RAIL_WIDTH - NAVIGATION_HORIZONTAL_INSET).max(0.0)),
                px(0.0),
            );
            if ContentBudget::from_body(labeled_body).profile == LayoutProfile::UltraCompact {
                NavigationPresentation::IconOnly
            } else {
                NavigationPresentation::Labeled
            }
        }
    }
}

/// Frame-local layout allocation shared by page renderers.
///
/// This is a projection, not persisted UI state: resize computes one new value
/// and every page consumes the same immutable decision for that frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageLayoutBudget {
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
    pub navigation: NavigationPresentation,
}

#[must_use]
pub fn layout_profile(viewport: Size<Pixels>) -> LayoutProfile {
    match f32::from(viewport.width) {
        width if width < ULTRA_COMPACT_CONTENT_WIDTH => LayoutProfile::UltraCompact,
        width if width < COMPACT_CONTENT_WIDTH => LayoutProfile::Compact,
        width if width < WIDE_CONTENT_WIDTH => LayoutProfile::Standard,
        _ => LayoutProfile::Wide,
    }
}

#[must_use]
pub fn vertical_space(viewport: Size<Pixels>) -> VerticalSpace {
    match f32::from(viewport.height) {
        height if height < CONSTRAINED_CONTENT_HEIGHT => VerticalSpace::Constrained,
        height if height < GENEROUS_CONTENT_HEIGHT => VerticalSpace::Standard,
        _ => VerticalSpace::Generous,
    }
}

impl PageLayoutBudget {
    #[must_use]
    pub fn for_viewport(viewport: Size<Pixels>) -> Self {
        FrameBudget::for_content(viewport, NavOrientation::Horizontal).page_layout()
    }

    /// Allocate the page body after accounting for a vertical navigation rail
    /// and its outer padding. The returned profile describes the remaining
    /// padded content slot, not the raw window width.
    #[must_use]
    pub fn for_frame(viewport: Size<Pixels>, orientation: NavOrientation) -> Self {
        FrameBudget::for_content(viewport, orientation).page_layout()
    }
}

#[must_use]
pub const fn nav_rail_width(presentation: NavigationPresentation) -> f32 {
    match presentation {
        NavigationPresentation::IconOnly => NAV_RAIL_COMPACT_WIDTH,
        NavigationPresentation::Labeled => NAV_RAIL_WIDTH,
    }
}

pub fn settings_content_max_height(viewport: Size<Pixels>) -> f32 {
    (f32::from(viewport.height) - 200.0).max(220.0)
}

/// Parse `WIDTHxHEIGHT` for deterministic capture runs. Invalid values retain
/// the normal 1180x780 launch size; valid values are clamped to the UI contract.
pub fn parse_window_size(value: &str) -> Option<Size<Pixels>> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let width = width.trim().parse::<f32>().ok()?;
    let height = height.trim().parse::<f32>().ok()?;
    if !width.is_finite() || !height.is_finite() {
        return None;
    }
    Some(size(
        px(width.clamp(MIN_WIDTH, 3840.0)),
        px(height.clamp(MIN_HEIGHT, 2160.0)),
    ))
}

pub fn initial_window_size() -> Size<Pixels> {
    std::env::var("TM_WINDOW_SIZE")
        .ok()
        .and_then(|value| parse_window_size(&value))
        .unwrap_or_else(|| size(px(1180.0), px(780.0)))
}

pub fn disconnected_device(theme: &Theme, stable_id: Option<&str>) -> impl IntoElement {
    let mut card = div()
        .max_w(px(460.0))
        .p(px(18.0))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::card_radius(theme),
        ))
        .border_1()
        .border_color(taskmanager_ui::theme_binding::hsla(theme.gpu))
        .bg(taskmanager_ui::theme_binding::fill(theme.sidebar_card_bg))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(
            div()
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_SEMIBOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(i18n::t("device.disconnected")),
        )
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("device.reconnect_hint")),
        )
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .font(mono_font_with_fallback(theme))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(
                    stable_id.map_or_else(crate::gpui_app::formatting::missing_value, String::from),
                ),
        );
    card = card.debug_selector(|| "tm-device-disconnected".to_string());
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(card)
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_responsive_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_layout_profile_parity_tests.rs"]
mod profile_parity_tests;
