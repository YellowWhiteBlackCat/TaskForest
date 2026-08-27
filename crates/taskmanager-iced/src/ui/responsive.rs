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
//! widths 54/144. Two Iced-specific seams preserve pre-port behavior and are
//! documented at their definitions:
//! - [`ChromePresentation`] keeps the Iced root-chrome 1320px single-row
//!   threshold (GPUI's root chrome has no such breakpoint).
//! - [`DeviceNavigationPresentation::for_compact_frame`] keys on the Iced
//!   compact flag (820x540, width OR height) instead of the width-only GPUI
//!   profile grid, because a short-but-wide window must keep its strip.
//!
//! Pure data only: no widget types, no I/O, panic-free.

use iced::Size;

/// Navigation is a layout region, not a collection of independently sized
/// controls. These widths are the only rail-width decisions (GPUI parity).
pub const NAV_RAIL_COMPACT_WIDTH: f32 = 54.0;
pub const NAV_RAIL_WIDTH: f32 = 144.0;

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

impl DeviceNavigationPresentation {
    /// Iced Performance-page seam: map the frame's compact fact onto the
    /// device navigation presentation. GPUI derives this from
    /// [`LayoutProfile`] (Strip iff UltraCompact); the Iced page keeps its
    /// pre-port authority — the compact flag (820px width OR 540px height) —
    /// because the width-only profile grid would flip short-but-wide windows
    /// (for example 900x500) from the stacked strip to the side rail.
    /// Existing behavior wins over grid parity at this seam.
    #[must_use]
    pub const fn for_compact_frame(compact: bool) -> Self {
        if compact { Self::Strip } else { Self::Sidebar }
    }
}

/// Performance detail panel presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceDetailsPresentation {
    Hidden,
    Pinned,
}

/// Performance chart inventory presentation (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerformanceChartInventory {
    AggregateOnly,
    Full,
}

/// Typed Performance-page allocation derived once at the viewport boundary
/// (GPUI parity).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformancePageBudget {
    pub device_navigation: DeviceNavigationPresentation,
    pub details: PerformanceDetailsPresentation,
    pub chart_inventory: PerformanceChartInventory,
    /// Content inset before the pinned details divider.
    pub main_trailing_inset: f32,
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
        let chart_inventory = match (layout.profile, layout.vertical_space) {
            (LayoutProfile::UltraCompact, _) | (_, VerticalSpace::Constrained) => {
                PerformanceChartInventory::AggregateOnly
            }
            (
                LayoutProfile::Compact | LayoutProfile::Standard | LayoutProfile::Wide,
                VerticalSpace::Standard | VerticalSpace::Generous,
            ) => PerformanceChartInventory::Full,
        };
        Self {
            device_navigation,
            details,
            chart_inventory,
            main_trailing_inset: layout.page_padding,
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
