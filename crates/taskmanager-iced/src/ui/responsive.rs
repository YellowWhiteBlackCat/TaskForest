//! Frame-local layout budget chain for the Iced frontend, ported from the
//! GPUI authority `taskmanager-gpui/src/gpui_app/root/responsive.rs`.
//!
//! [`LayoutProfile`] (horizontal capacity) and [`VerticalSpace`] (vertical
//! capacity) are two independent axes: a very wide, short window keeps its
//! wide horizontal composition while its vertical budget collapses secondary
//! content. [`PageLayoutBudget`] is computed once per frame at the viewport
//! boundary and every page consumes the same immutable decision; a page then
//! maps it exactly once into its presentation enums instead of primitives
//! each reading viewport pixels.
//!
//! Threshold parity with GPUI: width 840/1080/1600, height 700/960, rail
//! widths 54/144, Performance slot floors/ceilings, the Stacked details
//! fallback, and the Floor/Core/Charts vertical ladder. Two Iced-specific
//! seams preserve pre-port behavior and are documented at their definitions:
//! - [`ChromePresentation`] keeps the Iced root-chrome 1320px single-row
//!   threshold (GPUI's root chrome has no such breakpoint).
//! - The Performance page consumes [`PerformancePageBudget::for_perf_frame`]
//!   — the typed slot allocation from the real tracked viewport — as its
//!   single layout authority; no secondary compact-flag derivation remains.
//!
//! Pure data only: no widget types, no I/O, panic-free.

use iced::Size;

/// Navigation is a layout region, not a collection of independently sized
/// controls. These widths are the only rail-width decisions (GPUI parity).
pub const NAV_RAIL_COMPACT_WIDTH: f32 = 54.0;
pub const NAV_RAIL_WIDTH: f32 = 144.0;

/// Shared width contracts for the Performance page's semantic slots (GPUI
/// parity). Allocation floors and ceilings, not card dimensions: the frame
/// budget decides whether the device rail and statistics column can coexist
/// with a readable main viewport; renderers only consume the result.
pub const PERFORMANCE_MAIN_MIN_WIDTH: f32 = 360.0;
pub const PERFORMANCE_STATS_MIN_WIDTH: f32 = 236.0;
pub const PERFORMANCE_STATS_MAX_WIDTH: f32 = 280.0;
pub const PERFORMANCE_SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub const PERFORMANCE_SIDEBAR_MAX_WIDTH: f32 = 460.0;
pub const PERFORMANCE_SLOT_GAP: f32 = 12.0;
/// Stacked statistics rail height (GPUI `PERFORMANCE_STATS_STACK_HEIGHT`
/// parity): the detail column moves below the main viewport with one fixed
/// readable height instead of starving the primary graph.
pub const PERFORMANCE_STATS_STACK_HEIGHT: f32 = 220.0;
/// Content height the Performance core stack needs (GPUI parity): title row +
/// header band + headline tier floor + summary row + shared gaps. Below this
/// height the page degrades to the Floor rung.
pub const PERFORMANCE_RUNWAY_CORE_FLOOR: f32 = 380.0;
/// The Iced root chrome (page scaffold padding + spacing + nav row + footer)
/// that the shared rung thresholds must be measured without. GPUI's
/// `FrameBudget` already excludes chrome; Iced subtracts this typed constant
/// from the tracked viewport so the ladder classifies the region the page
/// actually paints in.
pub const PERFORMANCE_VERTICAL_CHROME_RESERVE: f32 = 128.0;
/// Scaffold horizontal padding the Performance workspace is measured without
/// (the body column's own two-sided padding).
pub const PERFORMANCE_HORIZONTAL_CHROME_RESERVE: f32 = 20.0;

const ULTRA_COMPACT_CONTENT_WIDTH: f32 = 840.0;
const COMPACT_CONTENT_WIDTH: f32 = 1080.0;
const WIDE_CONTENT_WIDTH: f32 = 1600.0;
const CONSTRAINED_CONTENT_HEIGHT: f32 = 700.0;
const GENEROUS_CONTENT_HEIGHT: f32 = 960.0;

/// The width from which the Iced root chrome composes routes and actions in
/// one desktop row. Iced-specific: mapping this off the GPUI profile grid
/// (single row iff `Wide`, 1600px) would flip every width in
/// `[1320, 1600)` back to the wrapped composition, so the pre-port threshold
/// is kept verbatim as a typed fact instead of a `ui.rs` literal.
pub const CHROME_SINGLE_ROW_MIN_WIDTH: f32 = 1320.0;

/// The width from which the wrapped action toolbar chunks into five columns
/// instead of three. Iced-specific (GPUI's chrome has no wrapped-row mode);
/// preserved from the pre-port `performance.rs` literal so 560px remains the
/// exact flip point.
pub const COMPACT_TOOLBAR_FIVE_COLUMN_MIN_WIDTH: f32 = 560.0;

/// Horizontal layout capacity shared by every page (GPUI parity).
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

/// Orientation of the root navigation region (GPUI `root::NavOrientation`
/// parity). A horizontal nav consumes no page-body width; a vertical rail is
/// subtracted before the profile is decided.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Root navigation rail presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationPresentation {
    IconOnly,
    Labeled,
}

/// Iced root chrome (route strip + action toolbar) row ownership for one
/// frame. The chrome wraps onto bounded rows below
/// [`CHROME_SINGLE_ROW_MIN_WIDTH`] and keeps the single desktop row above it;
/// the frame's compact fact (narrow OR short viewport) forces the wrapped
/// composition independently of this width axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromePresentation {
    /// Routes get their own bounded strip and actions get wrapped
    /// full-width rows.
    Wrapped,
    /// One desktop row: routes left, actions pinned to the trailing edge.
    SingleRow,
}

impl ChromePresentation {
    /// Map one frame width onto the chrome presentation. This is the budget
    /// mapping the root view consumes; [`PageLayoutBudget`] carries the same
    /// fact in its `chrome` field for full-viewport consumers.
    #[must_use]
    pub const fn for_width(width: f32) -> Self {
        if width < CHROME_SINGLE_ROW_MIN_WIDTH {
            Self::Wrapped
        } else {
            Self::SingleRow
        }
    }

    /// Whether this frame wraps the chrome onto bounded rows.
    #[must_use]
    pub const fn is_wrapped(self) -> bool {
        matches!(self, Self::Wrapped)
    }
}

/// Performance device navigation presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceNavigationPresentation {
    /// Horizontally windowed pill strip stacked above the detail.
    Strip,
    /// Vertical device-card rail beside the detail.
    Sidebar,
}

/// Performance detail panel presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceDetailsPresentation {
    Hidden,
    Pinned,
    /// The detail rail moves below the main viewport when horizontal capacity
    /// cannot support two readable columns. This keeps the information
    /// available without starving the primary graph (GPUI parity).
    Stacked,
}

/// Performance chart inventory presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceChartInventory {
    AggregateOnly,
    Full,
}

/// The Performance page's typed vertical degradation ladder — the height
/// axis of the minimum-space doctrine (GPUI parity, ADR-039). Each rung
/// names exactly which fixed obligations of the page may still render;
/// renderers consume the rung, never the raw pixel height.
///
/// Ordering is the degradation order: content drops from Charts down to
/// Floor, and the headline chart's floor is never touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerformanceVerticalRunway {
    /// Minimum viable page: title row + headline chart only. The header band
    /// and per-chart summary rows drop first — explicitly, not by silent
    /// clipping.
    Floor,
    /// The core stack composes: title row + header band (readouts /
    /// composition) + headline chart with its summary row. Secondary charts
    /// and the below band drop.
    Core,
    /// The full chart inventory fits: the core stack plus secondary charts
    /// and the per-core matrix. This is the rung `chart_inventory` requires
    /// from the height axis.
    Charts,
}

impl PerformanceVerticalRunway {
    /// Classify from the page-slot content height (viewport minus the typed
    /// chrome reserve). The Charts threshold is the shared Constrained page
    /// threshold; the Core threshold is the composed core-stack floor.
    #[must_use]
    pub const fn for_content_height(height: f32) -> Self {
        if height >= CONSTRAINED_CONTENT_HEIGHT {
            Self::Charts
        } else if height >= PERFORMANCE_RUNWAY_CORE_FLOOR {
            Self::Core
        } else {
            Self::Floor
        }
    }

    /// Whether the core stack (header band + summaries + below band) may
    /// render.
    #[must_use]
    pub const fn carries_core_stack(self) -> bool {
        matches!(self, Self::Core | Self::Charts)
    }
}

/// Fold the two typed axes into the one chart-inventory product bit (GPUI
/// parity): the width axis admits the full inventory only from the Compact
/// profile up, the height axis needs the Charts runway. One bit out, two
/// named reasons in — no axis conflation at the derivation site.
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

/// Typed Performance-page allocation derived once at the viewport boundary
/// (GPUI parity). `for_perf_frame` is the production authority — it
/// allocates the three semantic slots (device rail / statistics rail / main
/// viewport) from the real frame, exactly like GPUI's `from_frame`.
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
    /// Content inset before the pinned details divider.
    pub main_trailing_inset: f32,
    /// Effective device-rail width for this frame.
    pub sidebar_width: f32,
    /// Effective statistics-rail width for this frame.
    pub stats_width: f32,
    /// Width left for the primary Performance viewport after the slot
    /// allocation. Telemetry for tests; the flex tree stays the final
    /// geometry authority.
    pub main_width: f32,
    /// Width of the Performance workspace before its internal slots.
    pub workspace_width: f32,
}

impl PerformancePageBudget {
    /// The coarse profile-only constructor (GPUI `from_page_layout` parity):
    /// no pixel width, so the slot allocation keeps neutral defaults and the
    /// coarse vertical classification maps Constrained to the Core runway.
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
            main_trailing_inset: layout.page_padding,
            sidebar_width: match device_navigation {
                DeviceNavigationPresentation::Strip => 0.0,
                DeviceNavigationPresentation::Sidebar => PERFORMANCE_SIDEBAR_MIN_WIDTH,
            },
            stats_width: PERFORMANCE_STATS_MAX_WIDTH,
            main_width: 0.0,
            workspace_width: 0.0,
        }
    }

    /// The production authority: allocate the Performance page's semantic
    /// slots from the real tracked viewport (GPUI `from_frame` parity).
    ///
    /// A device sidebar is admitted only when the frame can preserve all
    /// three meaningful slots — device navigation, a readable main viewport,
    /// and the minimum useful statistics rail; otherwise the same devices
    /// move to the strip (including a hidden sidebar, which keeps every
    /// device reachable) without discarding the user's preference. The
    /// statistics rail is Pinned while capacity allows, Stacked below the
    /// main viewport while the main floor survives, and only then Hidden.
    #[must_use]
    pub fn for_perf_frame(viewport: Size, sidebar_visible: bool) -> Self {
        let workspace_width = (viewport.width - PERFORMANCE_HORIZONTAL_CHROME_RESERVE).max(0.0);
        let content_height = (viewport.height - PERFORMANCE_VERTICAL_CHROME_RESERVE).max(0.0);
        let body = Size::new(workspace_width, content_height);
        let layout = PageLayoutBudget::for_viewport(body);
        let profile = layout.profile;
        let inset = layout.page_padding.min(PERFORMANCE_SLOT_GAP);
        let main_min = match profile {
            LayoutProfile::UltraCompact => 320.0,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                PERFORMANCE_MAIN_MIN_WIDTH
            }
        };
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
                PERFORMANCE_SIDEBAR_MIN_WIDTH.clamp(PERFORMANCE_SIDEBAR_MIN_WIDTH, max_width)
            }
        };
        let remaining = (workspace_width - sidebar_width).max(0.0);
        let stats_capacity = remaining - main_min - inset;
        let details = if stats_capacity >= PERFORMANCE_STATS_MIN_WIDTH {
            PerformanceDetailsPresentation::Pinned
        } else if remaining >= main_min {
            PerformanceDetailsPresentation::Stacked
        } else {
            PerformanceDetailsPresentation::Hidden
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
        let vertical = PerformanceVerticalRunway::for_content_height(content_height);
        let chart_inventory = chart_inventory_from_axes(profile, vertical);
        Self {
            device_navigation,
            details,
            chart_inventory,
            vertical,
            main_trailing_inset: layout.page_padding.min(PERFORMANCE_SLOT_GAP),
            sidebar_width,
            stats_width,
            main_width,
            workspace_width,
        }
    }
}

/// System-page surface presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemSurfacePresentation {
    SingleColumn,
    MultiColumn,
}

/// Typed System-page allocation derived once at the viewport boundary (GPUI
/// parity).
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
}

/// Frame-local layout allocation shared by page renderers.
///
/// This is a projection, not persisted UI state: resize computes one new value
/// and every page consumes the same immutable decision for that frame. The
/// `chrome` field is the Iced-specific expansion of the GPUI shape (the root
/// chrome row ownership), computed from the same frame width in every
/// constructor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageLayoutBudget {
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
    pub navigation: NavigationPresentation,
    pub chrome: ChromePresentation,
}

/// Bucket one viewport's available width into the shared horizontal profile.
#[must_use]
pub fn layout_profile(viewport: Size) -> LayoutProfile {
    match viewport.width {
        width if width < ULTRA_COMPACT_CONTENT_WIDTH => LayoutProfile::UltraCompact,
        width if width < COMPACT_CONTENT_WIDTH => LayoutProfile::Compact,
        width if width < WIDE_CONTENT_WIDTH => LayoutProfile::Standard,
        _ => LayoutProfile::Wide,
    }
}

/// Bucket one viewport's available height into the shared vertical capacity.
#[must_use]
pub fn vertical_space(viewport: Size) -> VerticalSpace {
    match viewport.height {
        height if height < CONSTRAINED_CONTENT_HEIGHT => VerticalSpace::Constrained,
        height if height < GENEROUS_CONTENT_HEIGHT => VerticalSpace::Standard,
        _ => VerticalSpace::Generous,
    }
}

impl PageLayoutBudget {
    /// Allocate a full-viewport frame (no navigation width consumed).
    #[must_use]
    pub fn for_viewport(viewport: Size) -> Self {
        let profile = layout_profile(viewport);
        let vertical_space = vertical_space(viewport);
        let navigation = match profile {
            LayoutProfile::UltraCompact => NavigationPresentation::IconOnly,
            LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide => {
                NavigationPresentation::Labeled
            }
        };
        Self::for_capacity(
            profile,
            vertical_space,
            navigation,
            ChromePresentation::for_width(viewport.width),
        )
    }

    /// Allocate the page body after accounting for a vertical navigation
    /// rail. The rail presentation and page profile are separate facts: a
    /// 900px window can need an icon-only rail while its remaining body still
    /// earns the Compact profile.
    #[must_use]
    pub fn for_frame(viewport: Size, orientation: NavOrientation) -> Self {
        match orientation {
            NavOrientation::Horizontal => Self::for_viewport(viewport),
            NavOrientation::Vertical => {
                let viewport_width = viewport.width;
                let navigation = if viewport_width - NAV_RAIL_WIDTH >= ULTRA_COMPACT_CONTENT_WIDTH {
                    NavigationPresentation::Labeled
                } else {
                    NavigationPresentation::IconOnly
                };
                let body_viewport = Size::new(
                    (viewport_width - nav_rail_width(navigation)).max(0.0),
                    viewport.height,
                );
                Self::for_capacity(
                    layout_profile(body_viewport),
                    vertical_space(body_viewport),
                    navigation,
                    ChromePresentation::for_width(viewport_width),
                )
            }
        }
    }

    fn for_capacity(
        profile: LayoutProfile,
        vertical_space: VerticalSpace,
        navigation: NavigationPresentation,
        chrome: ChromePresentation,
    ) -> Self {
        let page_padding = match profile {
            LayoutProfile::UltraCompact => 8.0,
            LayoutProfile::Compact => 12.0,
            LayoutProfile::Standard | LayoutProfile::Wide => 16.0,
        };
        Self {
            profile,
            vertical_space,
            page_padding,
            navigation,
            chrome,
        }
    }
}

/// The one rail-width decision per root navigation presentation (GPUI
/// parity).
#[must_use]
pub const fn nav_rail_width(presentation: NavigationPresentation) -> f32 {
    match presentation {
        NavigationPresentation::IconOnly => NAV_RAIL_COMPACT_WIDTH,
        NavigationPresentation::Labeled => NAV_RAIL_WIDTH,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/responsive_tests.rs"]
mod responsive_tests;
