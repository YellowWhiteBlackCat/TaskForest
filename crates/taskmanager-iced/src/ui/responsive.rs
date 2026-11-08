//! Iced's projection of the toolkit-neutral presentation budget.
//!
//! The vocabulary and the classification folds are owned by
//! `taskmanager-ui-contract::presentation`; this module holds only Iced's
//! measured chrome input and the `iced::Size` adapters. Every deliberate
//! difference from the reference shape is registered in
//! `presentation::thresholds`, never asserted by a parity comment.
//!
//! [`LayoutProfile`] (horizontal capacity) and [`VerticalSpace`] (vertical
//! capacity) are two independent axes: a very wide, short window keeps its
//! wide horizontal composition while its vertical budget collapses secondary
//! content. [`PageLayoutBudget`] is computed once per frame at the viewport
//! boundary and every page consumes the same immutable decision; a page then
//! maps it exactly once into its presentation enums instead of primitives
//! each reading viewport pixels.
//!
//! The 1320px root-chrome band and the 700/960/650 height bands are Iced's
//! registered divergences; the Performance page consumes the shared slot
//! allocation from its real tracked viewport as its single layout authority.
//!
//! Pure data only: no widget types, no I/O, panic-free.

use iced::Size;
use taskmanager_ui_contract::navigation::NavOrientation;

use taskmanager_ui_contract::FrontendShape;
use taskmanager_ui_contract::presentation::budget::{
    ContentSlot, PerformanceSlotRequest, vertical_space_for_height,
};
use taskmanager_ui_contract::presentation::budget::{PageLayoutBudget, PerformancePageBudget};
use taskmanager_ui_contract::presentation::thresholds::{LayoutAxis, LayoutBands, bands, number};
use taskmanager_ui_contract::presentation::vocabulary::{
    ChromePresentation, LayoutProfile, NavigationPresentation, VerticalSpace,
    layout_profile_for_width,
};

fn shape_bands() -> LayoutBands {
    bands(FrontendShape::Iced)
}

/// Iced's registered divergences resolve through the threshold registry, so
/// every number below is stated once (ADR-064): a frontend binds the band it
/// declared, it never restates it. The registry rows are the authority, and
/// the boundary tests re-probe each band at its edges, so a declaration that
/// stopped spending one of these axes fails a test instead of silently
/// painting a fabricated zero.
pub const PERFORMANCE_VERTICAL_CHROME_RESERVE: f32 =
    number(FrontendShape::Iced, LayoutAxis::VerticalChromeReserve);
pub const PERFORMANCE_HORIZONTAL_CHROME_RESERVE: f32 =
    number(FrontendShape::Iced, LayoutAxis::HorizontalChromeReserve);
pub const CHROME_SINGLE_ROW_MIN_WIDTH: f32 =
    number(FrontendShape::Iced, LayoutAxis::RootChromeSingleRowWidth);
pub const PERFORMANCE_STATS_MIN_WIDTH: f32 =
    number(FrontendShape::Iced, LayoutAxis::PerformanceStatsMinWidth);
pub const PERFORMANCE_STATS_MAX_WIDTH: f32 =
    number(FrontendShape::Iced, LayoutAxis::PerformanceStatsMaxWidth);
pub const PERFORMANCE_SIDEBAR_MIN_WIDTH: f32 =
    number(FrontendShape::Iced, LayoutAxis::PerformanceSidebarMinWidth);

#[must_use]
pub fn performance_budget_for_perf_frame(
    viewport: Size,
    sidebar_visible: bool,
) -> PerformancePageBudget {
    performance_frame_for_perf_frame(viewport, sidebar_visible).budget
}

/// The Iced Performance frame: the shared verdict plus the inputs it was
/// derived from. A capture reports BOTH, so the shared contract can verify
/// the resolution instead of trusting it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IcedPerformanceFrame {
    pub budget: PerformancePageBudget,
    /// The workspace width the slot arbitration started from.
    pub workspace_width: f32,
    /// The width the horizontal bucket was classified from.
    pub width_input: f32,
    /// The content height the rung ladder classified from.
    pub content_height: f32,
    /// The page padding the frame earned.
    pub page_padding: f32,
    /// The horizontal bucket the frame earned.
    pub profile: LayoutProfile,
}

#[must_use]
pub fn performance_frame_for_perf_frame(
    viewport: Size,
    sidebar_visible: bool,
) -> IcedPerformanceFrame {
    let workspace_width = (viewport.width - PERFORMANCE_HORIZONTAL_CHROME_RESERVE).max(0.0);
    let content_height = (viewport.height - PERFORMANCE_VERTICAL_CHROME_RESERVE).max(0.0);
    let slot = ContentSlot::for_body(workspace_width, 0.0, &shape_bands());
    let budget = PerformancePageBudget::allocate(
        PerformanceSlotRequest {
            profile: slot.profile,
            workspace_width,
            content_height,
            page_padding: slot.page_padding,
            sidebar_visible,
            persisted_sidebar_width: None,
        },
        &shape_bands(),
    );
    IcedPerformanceFrame {
        budget,
        workspace_width,
        width_input: workspace_width,
        content_height,
        page_padding: slot.page_padding,
        profile: slot.profile,
    }
}

#[must_use]
pub fn page_layout_for_viewport(viewport: Size) -> PageLayoutBudget {
    let slot = ContentSlot::for_body(viewport.width, viewport.height, &shape_bands());
    PageLayoutBudget::from_slot(
        slot,
        slot.profile.navigation_default(),
        Some(chrome_presentation_for_width(viewport.width)),
    )
}

#[must_use]
pub const fn chrome_presentation_for_width(width: f32) -> ChromePresentation {
    if width < CHROME_SINGLE_ROW_MIN_WIDTH {
        ChromePresentation::Wrapped
    } else {
        ChromePresentation::SingleRow
    }
}

#[must_use]
pub fn page_layout_for_frame(viewport: Size, orientation: NavOrientation) -> PageLayoutBudget {
    match orientation {
        NavOrientation::Horizontal => page_layout_for_viewport(viewport),
        NavOrientation::Vertical => {
            PageLayoutBudget::for_vertical_frame(viewport.width, viewport.height, &shape_bands())
        }
    }
}

#[must_use]
pub fn layout_profile(viewport: Size) -> LayoutProfile {
    layout_profile_for_width(viewport.width)
}

#[must_use]
pub fn vertical_space(viewport: Size) -> VerticalSpace {
    vertical_space_for_height(viewport.height, &shape_bands())
}

#[must_use]
pub const fn nav_rail_width(presentation: NavigationPresentation) -> f32 {
    presentation.rail_width()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/responsive_tests.rs"]
mod responsive_tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/bottom_safety_band_tests.rs"]
mod bottom_safety_band_tests;
