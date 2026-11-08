//! The layout threshold registry: the single authority for the measured bands
//! each shape spends, and the reason-checked census for every shape that
//! spends a different band.
//!
//! A shared value is stated once (`LayoutAxis::baseline`). A shape that
//! deviates states its own number WITH the architecture driver, so the
//! difference is a registered decision rather than prose — the same discipline
//! the component capability registry uses. A deviation whose driver nobody
//! recorded is still recorded: it enters the debt census, whose set equality is
//! gate-checked, so new debt fails the build and retired debt must shrink the
//! baseline.

use crate::keybindings::FrontendShape;

/// The marker a `reason` carries when the driver is not actually known. It is
/// the debt census's only input: a divergent band whose reason starts with this
/// prefix is a convergence candidate, not a justified difference.
pub const UNRECORDED_DEBT_PREFIX: &str = "unrecorded:";

/// Every measured layout band a shape must answer for. The set is exhaustive
/// over the shapes: one entry per axis per frontend, never a silent omission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutAxis {
    /// Content height below which the vertical axis is Constrained.
    ConstrainedContentHeight,
    /// Content height from which the vertical axis is Generous.
    GenerousContentHeight,
    /// Content height from which the full Performance chart inventory admits.
    ChartsRungContentHeight,
    /// Content height a stacked Performance details rail needs to stay
    /// readable; `Absent` shapes keep a stacked rail at any height.
    StackedDetailsMinHeight,
    /// Frame width from which the root chrome composes in one desktop row.
    RootChromeSingleRowWidth,
    /// Minimum useful statistics-rail width.
    PerformanceStatsMinWidth,
    /// Maximum useful statistics-rail width.
    PerformanceStatsMaxWidth,
    /// Minimum useful device-rail width.
    PerformanceSidebarMinWidth,
    /// Device-rail width the coarse (no-pixel) constructor reports.
    SidebarPreferenceWidth,
    /// Horizontal inset charged around the vertical navigation rail.
    NavigationHorizontalInset,
    /// Smallest window width the shape accepts.
    MinimumWindowWidth,
    /// Smallest window height the shape accepts.
    MinimumWindowHeight,
    /// Vertical shell chrome the shape subtracts from the tracked viewport
    /// before classifying, when it subtracts a constant instead of measuring.
    VerticalChromeReserve,
    /// Horizontal shell chrome the shape subtracts before classifying.
    HorizontalChromeReserve,
}

impl LayoutAxis {
    /// Every axis in the census, in stable order.
    pub const ALL: [Self; 14] = [
        Self::ConstrainedContentHeight,
        Self::GenerousContentHeight,
        Self::ChartsRungContentHeight,
        Self::StackedDetailsMinHeight,
        Self::RootChromeSingleRowWidth,
        Self::PerformanceStatsMinWidth,
        Self::PerformanceStatsMaxWidth,
        Self::PerformanceSidebarMinWidth,
        Self::SidebarPreferenceWidth,
        Self::NavigationHorizontalInset,
        Self::MinimumWindowWidth,
        Self::MinimumWindowHeight,
        Self::VerticalChromeReserve,
        Self::HorizontalChromeReserve,
    ];

    /// Stable identity for reports and the debt baseline.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ConstrainedContentHeight => "constrained-content-height",
            Self::GenerousContentHeight => "generous-content-height",
            Self::ChartsRungContentHeight => "charts-rung-content-height",
            Self::StackedDetailsMinHeight => "stacked-details-min-height",
            Self::RootChromeSingleRowWidth => "root-chrome-single-row-width",
            Self::PerformanceStatsMinWidth => "performance-stats-min-width",
            Self::PerformanceStatsMaxWidth => "performance-stats-max-width",
            Self::PerformanceSidebarMinWidth => "performance-sidebar-min-width",
            Self::SidebarPreferenceWidth => "sidebar-preference-width",
            Self::NavigationHorizontalInset => "navigation-horizontal-inset",
            Self::MinimumWindowWidth => "minimum-window-width",
            Self::MinimumWindowHeight => "minimum-window-height",
            Self::VerticalChromeReserve => "vertical-chrome-reserve",
            Self::HorizontalChromeReserve => "horizontal-chrome-reserve",
        }
    }

    /// The shared reference value for this axis. `None` means there is no
    /// common value: only a shape that actually spends a number may declare
    /// `Shared` here, and any other shape spends `Absent`.
    #[must_use]
    pub const fn baseline(self) -> Option<f32> {
        match self {
            Self::ConstrainedContentHeight => Some(640.0),
            Self::GenerousContentHeight => Some(900.0),
            Self::ChartsRungContentHeight => Some(520.0),
            Self::StackedDetailsMinHeight => Some(520.0),
            Self::RootChromeSingleRowWidth => None,
            Self::PerformanceStatsMinWidth => Some(236.0),
            Self::PerformanceStatsMaxWidth => Some(280.0),
            Self::PerformanceSidebarMinWidth => Some(220.0),
            Self::SidebarPreferenceWidth => Some(260.0),
            Self::NavigationHorizontalInset => Some(12.0),
            Self::MinimumWindowWidth => Some(720.0),
            Self::MinimumWindowHeight => Some(480.0),
            Self::VerticalChromeReserve => None,
            Self::HorizontalChromeReserve => None,
        }
    }

    /// Whether the axis names a band that always needs a number. Optional axes
    /// are genuinely absent in some shapes (a measured-chrome root has no
    /// constant chrome reserve); required axes report the shared baseline when
    /// a shape does not spend them at all.
    #[must_use]
    pub const fn is_optional(self) -> bool {
        matches!(
            self,
            Self::StackedDetailsMinHeight
                | Self::RootChromeSingleRowWidth
                | Self::MinimumWindowWidth
                | Self::MinimumWindowHeight
                | Self::VerticalChromeReserve
                | Self::HorizontalChromeReserve
        )
    }
}

/// One shape's answer for one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisDecision {
    /// The shape spends [`LayoutAxis::baseline`]. No reason needed: this is
    /// the shared fact, owned here.
    Shared,
    /// The shape spends its own measured number, with the driver.
    Own { value: f32, reason: &'static str },
    /// The axis does not exist in this shape, with the driver. `reason` names
    /// the mechanism that replaces it (a measured chrome, a cell grid).
    Absent { reason: &'static str },
}

impl AxisDecision {
    /// The explanation this decision must carry: `None` only for `Shared`.
    #[must_use]
    pub const fn reason(self) -> Option<&'static str> {
        match self {
            Self::Shared => None,
            Self::Own { reason, .. } | Self::Absent { reason } => Some(reason),
        }
    }

    /// Whether this decision is an unexplained legacy difference — the debt
    /// census's input.
    #[must_use]
    pub fn is_debt(self) -> bool {
        match self.reason() {
            Some(reason) => reason.starts_with(UNRECORDED_DEBT_PREFIX),
            None => false,
        }
    }
}

/// One declared axis answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutThresholdEntry {
    pub axis: LayoutAxis,
    pub decision: AxisDecision,
}

/// A shape's complete threshold declaration. The production authority is a
/// borrowed slice, so a gate can pass a mutated table through the same fold
/// without leaking a second type or a test-only constructor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutThresholdDeclaration<'a> {
    pub frontend: FrontendShape,
    pub entries: &'a [LayoutThresholdEntry],
}

/// The resolved bands one shape classifies with. Every band is a plain logical
/// px value so a shape can feed it to its own length type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutBands {
    pub constrained_content_height: f32,
    pub generous_content_height: f32,
    pub charts_rung_content_height: f32,
    pub stacked_details_min_height: Option<f32>,
    pub chrome_single_row_min_width: Option<f32>,
    pub performance_stats_min_width: f32,
    pub performance_stats_max_width: f32,
    pub performance_sidebar_min_width: f32,
    pub sidebar_preference_width: f32,
    pub navigation_horizontal_inset: f32,
    pub min_window_width: Option<f32>,
    pub min_window_height: Option<f32>,
    pub vertical_chrome_reserve: Option<f32>,
    pub horizontal_chrome_reserve: Option<f32>,
}

mod declarations;
mod findings;

pub use declarations::DECLARATIONS;
pub use findings::{
    LayoutDebtCell, LayoutThresholdFinding, LayoutThresholdFindingKind, declaration_findings,
    divergent_axes, effective_band, layout_debt_census, layout_threshold_findings,
};

/// The stable const-comparable code of one shape.
///
/// The registry resolvers are `const fn` so a frontend can bind a declared
/// band into its own `const` (see [`number`]); derived `PartialEq` is not a
/// const call, so the const path compares these codes instead of `==`.
/// `LayoutAxis::ALL` and `FrontendShape::ALL` order is the code order.
const fn shape_code(frontend: FrontendShape) -> u8 {
    match frontend {
        FrontendShape::Gpui => 0,
        FrontendShape::Iced => 1,
        FrontendShape::Tui => 2,
        FrontendShape::Bevy => 3,
    }
}

/// The stable const-comparable code of one axis, matching `LayoutAxis::ALL`.
const fn axis_code(axis: LayoutAxis) -> u8 {
    match axis {
        LayoutAxis::ConstrainedContentHeight => 0,
        LayoutAxis::GenerousContentHeight => 1,
        LayoutAxis::ChartsRungContentHeight => 2,
        LayoutAxis::StackedDetailsMinHeight => 3,
        LayoutAxis::RootChromeSingleRowWidth => 4,
        LayoutAxis::PerformanceStatsMinWidth => 5,
        LayoutAxis::PerformanceStatsMaxWidth => 6,
        LayoutAxis::PerformanceSidebarMinWidth => 7,
        LayoutAxis::SidebarPreferenceWidth => 8,
        LayoutAxis::NavigationHorizontalInset => 9,
        LayoutAxis::MinimumWindowWidth => 10,
        LayoutAxis::MinimumWindowHeight => 11,
        LayoutAxis::VerticalChromeReserve => 12,
        LayoutAxis::HorizontalChromeReserve => 13,
    }
}

/// The declaration for one shape.
#[must_use]
pub const fn declaration(
    frontend: FrontendShape,
) -> Option<&'static LayoutThresholdDeclaration<'static>> {
    let wanted = shape_code(frontend);
    let mut index = 0;
    while index < DECLARATIONS.len() {
        let candidate = &DECLARATIONS[index];
        if shape_code(candidate.frontend) == wanted {
            return Some(candidate);
        }
        index += 1;
    }
    None
}

/// One shape's decision for one axis.
#[must_use]
pub const fn decision(frontend: FrontendShape, axis: LayoutAxis) -> Option<AxisDecision> {
    let wanted_shape = shape_code(frontend);
    let wanted_axis = axis_code(axis);
    let mut shape = 0;
    while shape < DECLARATIONS.len() {
        let candidate = &DECLARATIONS[shape];
        if shape_code(candidate.frontend) == wanted_shape {
            let mut entry = 0;
            while entry < candidate.entries.len() {
                let declared = &candidate.entries[entry];
                if axis_code(declared.axis) == wanted_axis {
                    return Some(declared.decision);
                }
                entry += 1;
            }
        }
        shape += 1;
    }
    None
}

/// Resolve one shape's classification bands. A required axis a shape does not
/// spend reports the shared baseline, so the resolver is total and never
/// fabricates a zero; [`layout_threshold_findings`] is what proves the
/// declaration is complete.
#[must_use]
pub const fn bands(frontend: FrontendShape) -> LayoutBands {
    LayoutBands {
        constrained_content_height: number(frontend, LayoutAxis::ConstrainedContentHeight),
        generous_content_height: number(frontend, LayoutAxis::GenerousContentHeight),
        charts_rung_content_height: number(frontend, LayoutAxis::ChartsRungContentHeight),
        stacked_details_min_height: optional(frontend, LayoutAxis::StackedDetailsMinHeight),
        chrome_single_row_min_width: optional(frontend, LayoutAxis::RootChromeSingleRowWidth),
        performance_stats_min_width: number(frontend, LayoutAxis::PerformanceStatsMinWidth),
        performance_stats_max_width: number(frontend, LayoutAxis::PerformanceStatsMaxWidth),
        performance_sidebar_min_width: number(frontend, LayoutAxis::PerformanceSidebarMinWidth),
        sidebar_preference_width: number(frontend, LayoutAxis::SidebarPreferenceWidth),
        navigation_horizontal_inset: number(frontend, LayoutAxis::NavigationHorizontalInset),
        min_window_width: optional(frontend, LayoutAxis::MinimumWindowWidth),
        min_window_height: optional(frontend, LayoutAxis::MinimumWindowHeight),
        vertical_chrome_reserve: optional(frontend, LayoutAxis::VerticalChromeReserve),
        horizontal_chrome_reserve: optional(frontend, LayoutAxis::HorizontalChromeReserve),
    }
}

/// The effective number one shape spends on an axis: its own value when it
/// declares one, otherwise the shared baseline, otherwise `0.0` only for an
/// axis with no baseline at all (which `Shared` may not claim).
///
/// `const fn` on purpose: a frontend that needs the band at a compile-time
/// boundary (a `const` it binds into its own length type) writes
/// `number(FrontendShape::X, LayoutAxis::Y)` there, so the registry stays the
/// ONLY address for the number. A local mirror constant that restates the same
/// value is a second authority and a gate finding.
#[must_use]
pub const fn number(frontend: FrontendShape, axis: LayoutAxis) -> f32 {
    match decision(frontend, axis) {
        Some(AxisDecision::Own { value, .. }) => value,
        _ => match axis.baseline() {
            Some(value) => value,
            None => 0.0,
        },
    }
}

/// The effective optional band: `None` when the shape does not spend the axis.
#[must_use]
pub const fn optional(frontend: FrontendShape, axis: LayoutAxis) -> Option<f32> {
    match decision(frontend, axis) {
        Some(AxisDecision::Own { value, .. }) => Some(value),
        Some(AxisDecision::Absent { .. }) => None,
        Some(AxisDecision::Shared) | None => axis.baseline(),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_thresholds.rs"]
mod tests;
