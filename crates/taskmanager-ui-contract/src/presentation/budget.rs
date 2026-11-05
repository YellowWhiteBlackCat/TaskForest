//! The shared layout budget: the folds that turn one measured slot into typed
//! page capacity, and the Performance slot arbitration that decides which
//! obligations may coexist.
//!
//! Every function here is pure arithmetic on logical pixels plus the shape's
//! registered [`LayoutBands`]. A frontend supplies what only it can know — the
//! chrome it actually spends — and consumes the verdict. It never re-derives a
//! bucket, a padding, a rung, or an admission rule. That is what makes the
//! cross-shape matrix checkable: the same function on the same bands must give
//! the same verdict for every shape.

use crate::presentation::thresholds::LayoutBands;
use crate::presentation::vocabulary::{
    ChromePresentation, DeviceNavigationPresentation, LayoutProfile, NavigationPresentation,
    PERFORMANCE_MAIN_MIN_WIDTH, PERFORMANCE_SIDEBAR_MAX_WIDTH, PERFORMANCE_SLOT_GAP,
    PERFORMANCE_ULTRA_COMPACT_MAIN_MIN_WIDTH, PerformanceChartInventory,
    PerformanceDetailsPresentation, PerformanceVerticalRunway, SystemSurfacePresentation,
    VerticalSpace, chart_inventory_from_axes, details_from_profile, device_navigation_from_profile,
    runway_from_vertical_space, system_surfaces_from_profile,
};

/// One measured page slot: the rectangle a page actually paints in, already
/// stripped of the shape's real shell chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentSlot {
    pub width: f32,
    pub height: f32,
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
}

impl ContentSlot {
    /// Classify one body rectangle into the shared capacity buckets.
    ///
    /// The horizontal profile and the padding it earns are mutually dependent
    /// (the padding token is profile-dependent, and the padding shrinks the
    /// width that earns the token), so this iterates to the fixed point instead
    /// of letting a page pick either one by hand.
    #[must_use]
    pub fn for_body(body_width: f32, body_height: f32, bands: &LayoutBands) -> Self {
        let width = body_width.max(0.0);
        let mut profile = LayoutProfile::for_content_width(width);
        for _ in 0..3 {
            let padded = (width - profile.page_padding() * 2.0).max(0.0);
            let next = LayoutProfile::for_content_width(padded);
            if next == profile {
                break;
            }
            profile = next;
        }
        let page_padding = profile.page_padding();
        let content_width = (width - page_padding * 2.0).max(0.0);
        let content_height = (body_height.max(0.0) - page_padding * 2.0).max(0.0);
        Self {
            width: content_width,
            height: content_height,
            profile,
            vertical_space: vertical_space_for_height(content_height, bands),
            page_padding,
        }
    }
}

/// Bucket one measured content height into the shared vertical capacity, using
/// the shape's registered bands.
#[must_use]
pub const fn vertical_space_for_height(height: f32, bands: &LayoutBands) -> VerticalSpace {
    if height < bands.constrained_content_height {
        VerticalSpace::Constrained
    } else if height < bands.generous_content_height {
        VerticalSpace::Standard
    } else {
        VerticalSpace::Generous
    }
}

/// The navigation fold: the rail goes icon-only exactly when the body left
/// behind by the rail no longer earns a labelled page profile. One rule for
/// every shape; what differs is only the inset the shape charges around the
/// rail, which is a registered band.
#[must_use]
pub fn navigation_for_viewport_width(
    viewport_width: f32,
    bands: &LayoutBands,
) -> NavigationPresentation {
    let body_width = viewport_width.max(0.0) - NavigationPresentation::Labeled.rail_width();
    if ContentSlot::for_body(body_width, 0.0, bands).profile == LayoutProfile::UltraCompact {
        NavigationPresentation::IconOnly
    } else {
        NavigationPresentation::Labeled
    }
}

/// Frame-local layout allocation shared by page renderers.
///
/// This is a projection, not persisted UI state: a resize computes one new
/// value and every page consumes the same immutable decision for that frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageLayoutBudget {
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub page_padding: f32,
    pub navigation: NavigationPresentation,
    /// Measured page slot width after the shape's chrome and padding.
    pub content_width: f32,
    /// Measured page slot height after the shape's chrome and padding.
    pub content_height: f32,
    /// The root chrome's row ownership, or `None` for a shape whose chrome
    /// composition is decided by measured regions instead of a width band.
    pub chrome: Option<ChromePresentation>,
}

impl PageLayoutBudget {
    /// Assemble one budget from an already classified slot.
    #[must_use]
    pub const fn from_slot(
        slot: ContentSlot,
        navigation: NavigationPresentation,
        chrome: Option<ChromePresentation>,
    ) -> Self {
        Self {
            profile: slot.profile,
            vertical_space: slot.vertical_space,
            page_padding: slot.page_padding,
            navigation,
            content_width: slot.width,
            content_height: slot.height,
            chrome,
        }
    }

    /// Classify a full-viewport frame: no navigation rail is deducted from the
    /// width, so the rail form follows the profile the frame itself earns.
    #[must_use]
    pub fn for_viewport(width: f32, height: f32, bands: &LayoutBands) -> Self {
        let slot = ContentSlot::for_body(width, height, bands);
        Self::from_slot(
            slot,
            slot.profile.navigation_default(),
            ChromePresentation::for_width(width, bands.chrome_single_row_min_width),
        )
    }

    /// Classify the page body left behind a vertical navigation rail. The rail
    /// presentation and the page profile are separate facts: a narrow window
    /// can need an icon-only rail while its remaining body still earns a
    /// labelled page profile, so the rail is decided first and the body is
    /// classified after the deduction the shape actually spends.
    #[must_use]
    pub fn for_vertical_frame(width: f32, height: f32, bands: &LayoutBands) -> Self {
        let navigation = navigation_for_viewport_width(width, bands);
        let body_width =
            (width.max(0.0) - navigation.rail_width() - bands.navigation_horizontal_inset).max(0.0);
        let slot = ContentSlot::for_body(body_width, height, bands);
        Self::from_slot(
            slot,
            navigation,
            ChromePresentation::for_width(width, bands.chrome_single_row_min_width),
        )
    }
}

impl LayoutProfile {
    /// The navigation form a frame implies when nothing was deducted from it.
    #[must_use]
    pub const fn navigation_default(self) -> NavigationPresentation {
        match self {
            Self::UltraCompact => NavigationPresentation::IconOnly,
            Self::Compact | Self::Standard | Self::Wide => NavigationPresentation::Labeled,
        }
    }

    /// The main-viewport floor Performance keeps readable in this bucket.
    #[must_use]
    pub const fn performance_main_floor(self) -> f32 {
        match self {
            Self::UltraCompact => PERFORMANCE_ULTRA_COMPACT_MAIN_MIN_WIDTH,
            Self::Compact | Self::Standard | Self::Wide => PERFORMANCE_MAIN_MIN_WIDTH,
        }
    }
}

/// What a shape hands the Performance slot arbitration: the extents its own
/// chrome leaves behind, the bucket the page earned, and whatever device-rail
/// preference it persists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformanceSlotRequest {
    /// The horizontal bucket the page slot earned.
    pub profile: LayoutProfile,
    /// Performance workspace width before its internal slots.
    pub workspace_width: f32,
    /// Page-slot content height the rung ladder classifies.
    pub content_height: f32,
    /// The page padding the frame already earned; the inset is clamped by it.
    pub page_padding: f32,
    /// Whether the user's device rail is currently shown.
    pub sidebar_visible: bool,
    /// The persisted device-rail width preference, when the shape has one.
    pub persisted_sidebar_width: Option<f32>,
}

/// Typed Performance-page allocation derived once at the viewport boundary.
///
/// [`PerformancePageBudget::allocate`] is the whole product contract for this
/// page: a device rail is admitted only when the frame can keep all three
/// meaningful slots, and the statistics rail is Pinned while capacity allows,
/// Stacked while the main floor survives, and only then Hidden. Nothing here is
/// toolkit state, so one implementation serves every shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerformancePageBudget {
    pub device_navigation: DeviceNavigationPresentation,
    pub details: PerformanceDetailsPresentation,
    pub chart_inventory: PerformanceChartInventory,
    /// The typed height-axis degradation ladder. `chart_inventory` is one
    /// product bit derived from BOTH axes; this field keeps the height reason
    /// typed and consumable on its own.
    pub vertical: PerformanceVerticalRunway,
    /// Numeric page-slot content height backing `vertical`. The coarse
    /// constructor reports `0.0` = unknown, and policies must then fall back to
    /// the rung-only behavior.
    pub content_height: f32,
    /// Content inset before the pinned details divider. The outer page frame's
    /// trailing inset stays zero so the pinned rail itself reaches the edge.
    pub main_trailing_inset: f32,
    /// Effective device-rail width for this frame.
    pub sidebar_width: f32,
    /// Effective statistics-rail width for this frame.
    pub stats_width: f32,
    /// Width left for the primary viewport after the top-level slot
    /// allocation. Telemetry for tests and renderers; the toolkit's layout tree
    /// stays the final geometry authority.
    pub main_width: f32,
    /// Width of the Performance workspace before its internal slots.
    pub workspace_width: f32,
}

impl PerformancePageBudget {
    /// The coarse, profile-only allocation: no pixel width is known, so the
    /// slot forms follow the buckets alone and the legacy vertical
    /// classification maps Constrained to the Core runway.
    #[must_use]
    pub fn coarse(layout: PageLayoutBudget, bands: &LayoutBands) -> Self {
        let device_navigation = device_navigation_from_profile(layout.profile);
        let vertical = runway_from_vertical_space(layout.vertical_space);
        Self {
            device_navigation,
            details: details_from_profile(layout.profile),
            chart_inventory: chart_inventory_from_axes(layout.profile, vertical),
            vertical,
            content_height: 0.0,
            main_trailing_inset: layout.page_padding,
            sidebar_width: match device_navigation {
                DeviceNavigationPresentation::Strip => 0.0,
                DeviceNavigationPresentation::Sidebar => bands.sidebar_preference_width,
            },
            stats_width: bands.performance_stats_max_width,
            main_width: 0.0,
            workspace_width: 0.0,
        }
    }

    /// Allocate Performance's semantic slots from one measured frame. This is
    /// the production authority for every shape that spends the budget.
    #[must_use]
    pub fn allocate(request: PerformanceSlotRequest, bands: &LayoutBands) -> Self {
        let workspace_width = request.workspace_width.max(0.0);
        let content_height = request.content_height.max(0.0);
        let inset = request.page_padding.min(PERFORMANCE_SLOT_GAP);
        let main_min = request.profile.performance_main_floor();

        // A device rail is admitted only when the page can preserve all three
        // meaningful slots: device navigation, a readable main viewport, and
        // the minimum useful statistics rail. Otherwise the same devices move
        // to the strip without changing the page or the user's preference.
        let can_pin_with_sidebar = workspace_width
            >= bands.performance_sidebar_min_width
                + bands.performance_stats_min_width
                + main_min
                + inset;
        let device_navigation = if request.sidebar_visible && can_pin_with_sidebar {
            DeviceNavigationPresentation::Sidebar
        } else {
            DeviceNavigationPresentation::Strip
        };
        let sidebar_width = match device_navigation {
            DeviceNavigationPresentation::Strip => 0.0,
            DeviceNavigationPresentation::Sidebar => {
                let max_width =
                    (workspace_width - bands.performance_stats_min_width - main_min - inset).clamp(
                        bands.performance_sidebar_min_width,
                        PERFORMANCE_SIDEBAR_MAX_WIDTH,
                    );
                let preferred = request
                    .persisted_sidebar_width
                    .unwrap_or(bands.sidebar_preference_width);
                preferred.clamp(bands.performance_sidebar_min_width, max_width)
            }
        };
        let remaining = (workspace_width - sidebar_width).max(0.0);
        let stats_capacity = remaining - main_min - inset;
        let width_details = if stats_capacity >= bands.performance_stats_min_width {
            PerformanceDetailsPresentation::Pinned
        } else if remaining >= main_min {
            PerformanceDetailsPresentation::Stacked
        } else {
            PerformanceDetailsPresentation::Hidden
        };
        // A stacked rail spends its own readable height on top of the headline
        // floor; a shape that registers no such floor keeps the stacked form.
        // The band decides; this function holds no opinion of its own.
        let details = if width_details == PerformanceDetailsPresentation::Stacked
            && bands
                .stacked_details_min_height
                .is_some_and(|floor| content_height < floor)
        {
            PerformanceDetailsPresentation::Hidden
        } else {
            width_details
        };
        let stats_width = if details == PerformanceDetailsPresentation::Pinned {
            stats_capacity.clamp(
                bands.performance_stats_min_width,
                bands.performance_stats_max_width,
            )
        } else {
            bands.performance_stats_min_width
        };
        let main_width = match details {
            PerformanceDetailsPresentation::Pinned => (remaining - stats_width - inset).max(0.0),
            PerformanceDetailsPresentation::Stacked | PerformanceDetailsPresentation::Hidden => {
                remaining
            }
        };
        // The height axis is typed from the real pixel height; `chart_inventory`
        // folds both axes into the one product bit.
        let vertical = PerformanceVerticalRunway::for_content_height(
            content_height,
            bands.charts_rung_content_height,
        );
        Self {
            device_navigation,
            details,
            chart_inventory: chart_inventory_from_axes(request.profile, vertical),
            vertical,
            content_height,
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

/// Typed System-page allocation derived once at the viewport boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemPageBudget {
    pub surfaces: SystemSurfacePresentation,
}

impl SystemPageBudget {
    /// The system page spends only the horizontal bucket.
    #[must_use]
    pub const fn from_page_layout(layout: PageLayoutBudget) -> Self {
        Self {
            surfaces: system_surfaces_from_profile(layout.profile),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_budget.rs"]
mod tests;
