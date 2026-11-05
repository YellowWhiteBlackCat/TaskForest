//! The cross-shape layout parity matrix: one probe grid, one verdict fold, and
//! the rule that a differing verdict must be explained by a registered band.
//!
//! This module replaces the "(GPUI parity)" comment class of evidence. Two
//! shapes that consume the shared vocabulary must classify the same viewport to
//! the same typed verdict on every field whose governing axes they do not
//! diverge on. [`unexplained_differences`] computes exactly that set, so a
//! difference either appears in the threshold registry or fails the gate.
//!
//! The probes are the elastic-layout playbook's representative windows, not
//! implementation constants: each one is meant to land on a real boundary.

use crate::keybindings::FrontendShape;
use crate::presentation::budget::{
    PageLayoutBudget, PerformancePageBudget, PerformanceSlotRequest,
};
use crate::presentation::thresholds::{LayoutAxis, bands, divergent_axes};
use crate::presentation::vocabulary::{
    ChromePresentation, DeviceNavigationPresentation, LayoutProfile, NavigationPresentation,
    PerformanceChartInventory, PerformanceDetailsPresentation, PerformanceVerticalRunway,
    VerticalSpace,
};

/// One sampled viewport, in the shape's own logical px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutProbe {
    pub name: &'static str,
    pub width: f32,
    pub height: f32,
}

/// The probe grid every consuming shape must classify alike on shared axes.
///
/// `minimum`, `reference` and `large` are the shipped window sizes; the rest are
/// deliberately placed ON and just either side of each band boundary so a
/// classifier cannot pass by reading a constant back.
pub const PROBES: &[LayoutProbe] = &[
    LayoutProbe {
        name: "minimum",
        width: 720.0,
        height: 480.0,
    },
    LayoutProbe {
        name: "ultra-edge-low",
        width: 1007.0,
        height: 780.0,
    },
    LayoutProbe {
        name: "ultra-edge-high",
        width: 1008.0,
        height: 780.0,
    },
    LayoutProbe {
        name: "compact-edge",
        width: 1248.0,
        height: 780.0,
    },
    LayoutProbe {
        name: "reference",
        width: 1180.0,
        height: 780.0,
    },
    LayoutProbe {
        name: "desktop-1280x720",
        width: 1280.0,
        height: 720.0,
    },
    LayoutProbe {
        name: "wide-edge",
        width: 1768.0,
        height: 900.0,
    },
    LayoutProbe {
        name: "chrome-single-row-edge",
        width: 1320.0,
        height: 720.0,
    },
    LayoutProbe {
        name: "tall",
        width: 1180.0,
        height: 1400.0,
    },
    LayoutProbe {
        name: "wide-short",
        width: 2400.0,
        height: 520.0,
    },
    LayoutProbe {
        name: "narrow-tall",
        width: 900.0,
        height: 1300.0,
    },
];

/// One field of a layout verdict. Every field is governed by an explicit axis
/// set (`axes_governing`), which is what makes a difference checkable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerdictField {
    Profile,
    VerticalSpace,
    Navigation,
    Chrome,
    Runway,
    ChartInventory,
    Details,
    DeviceNavigation,
    SidebarWidth,
    StatsWidth,
    MainWidth,
}

impl VerdictField {
    /// Every field the matrix compares.
    pub const ALL: [Self; 11] = [
        Self::Profile,
        Self::VerticalSpace,
        Self::Navigation,
        Self::Chrome,
        Self::Runway,
        Self::ChartInventory,
        Self::Details,
        Self::DeviceNavigation,
        Self::SidebarWidth,
        Self::StatsWidth,
        Self::MainWidth,
    ];

    /// Stable identity for gate output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::VerticalSpace => "vertical-space",
            Self::Navigation => "navigation",
            Self::Chrome => "chrome",
            Self::Runway => "runway",
            Self::ChartInventory => "chart-inventory",
            Self::Details => "details",
            Self::DeviceNavigation => "device-navigation",
            Self::SidebarWidth => "sidebar-width",
            Self::StatsWidth => "stats-width",
            Self::MainWidth => "main-width",
        }
    }

    /// The bands allowed to explain a difference on this field. An empty set
    /// means the field is a pure shared fold: any difference is a defect.
    #[must_use]
    pub const fn axes_governing(self) -> &'static [LayoutAxis] {
        use LayoutAxis::{
            ChartsRungContentHeight, ConstrainedContentHeight, GenerousContentHeight,
            NavigationHorizontalInset, PerformanceSidebarMinWidth, PerformanceStatsMaxWidth,
            PerformanceStatsMinWidth, RootChromeSingleRowWidth, SidebarPreferenceWidth,
            StackedDetailsMinHeight,
        };
        match self {
            Self::Profile => &[NavigationHorizontalInset],
            Self::VerticalSpace => &[ConstrainedContentHeight, GenerousContentHeight],
            Self::Navigation => &[NavigationHorizontalInset],
            Self::Chrome => &[RootChromeSingleRowWidth],
            Self::Runway => &[ChartsRungContentHeight],
            Self::ChartInventory => &[ChartsRungContentHeight],
            Self::Details => &[StackedDetailsMinHeight],
            Self::DeviceNavigation => &[
                PerformanceStatsMinWidth,
                PerformanceSidebarMinWidth,
                SidebarPreferenceWidth,
            ],
            Self::SidebarWidth => &[
                PerformanceStatsMinWidth,
                PerformanceSidebarMinWidth,
                SidebarPreferenceWidth,
            ],
            Self::StatsWidth => &[
                PerformanceStatsMinWidth,
                PerformanceStatsMaxWidth,
                PerformanceSidebarMinWidth,
                SidebarPreferenceWidth,
            ],
            Self::MainWidth => &[
                PerformanceStatsMinWidth,
                PerformanceStatsMaxWidth,
                PerformanceSidebarMinWidth,
                SidebarPreferenceWidth,
            ],
        }
    }
}

/// The complete typed verdict one shape reaches for one probe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutVerdict {
    pub profile: LayoutProfile,
    pub vertical_space: VerticalSpace,
    pub navigation: NavigationPresentation,
    pub chrome: Option<ChromePresentation>,
    pub runway: PerformanceVerticalRunway,
    pub chart_inventory: PerformanceChartInventory,
    pub details: PerformanceDetailsPresentation,
    pub device_navigation: DeviceNavigationPresentation,
    pub sidebar_width: f32,
    pub stats_width: f32,
    pub main_width: f32,
}

/// Classify one probe for one shape through the shared budget path.
///
/// The Performance request is derived from the page slot itself (workspace =
/// slot width + one padding, height = slot height), which is the neutral
/// statement of what each shape's own chrome deduction is expected to produce.
/// A shape feeds the same fold from its measured chrome in production.
#[must_use]
pub fn verdict(frontend: FrontendShape, probe: LayoutProbe) -> LayoutVerdict {
    let shape_bands = bands(frontend);
    let page = PageLayoutBudget::for_viewport(probe.width, probe.height, &shape_bands);
    let performance = PerformancePageBudget::allocate(
        PerformanceSlotRequest {
            profile: page.profile,
            workspace_width: page.content_width + page.page_padding,
            content_height: page.content_height,
            page_padding: page.page_padding,
            sidebar_visible: true,
            persisted_sidebar_width: None,
        },
        &shape_bands,
    );
    LayoutVerdict {
        profile: page.profile,
        vertical_space: page.vertical_space,
        navigation: page.navigation,
        chrome: page.chrome,
        runway: performance.vertical,
        chart_inventory: performance.chart_inventory,
        details: performance.details,
        device_navigation: performance.device_navigation,
        sidebar_width: performance.sidebar_width,
        stats_width: performance.stats_width,
        main_width: performance.main_width,
    }
}

/// One field one pair of shapes disagrees on, with the axes that could explain
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnexplainedDifference {
    pub left: FrontendShape,
    pub right: FrontendShape,
    pub probe: &'static str,
    pub field: VerdictField,
}

/// Every verdict difference that NO registered band can explain.
///
/// This is the shape ARCH § 8.3 demands of a parity claim: it is computed from
/// the two classifiers, not asserted by prose, and an empty result is the pass
/// condition.
#[must_use]
pub fn unexplained_differences(
    left: FrontendShape,
    right: FrontendShape,
) -> Vec<UnexplainedDifference> {
    let registered = divergent_axes(left, right);
    let mut differences = Vec::new();
    for probe in PROBES {
        let left_verdict = verdict(left, *probe);
        let right_verdict = verdict(right, *probe);
        for field in VerdictField::ALL {
            if field_values_differ(field, &left_verdict, &right_verdict)
                && !field_is_registered(field, &registered)
            {
                differences.push(UnexplainedDifference {
                    left,
                    right,
                    probe: probe.name,
                    field,
                });
            }
        }
    }
    differences
}

fn field_is_registered(field: VerdictField, registered: &[LayoutAxis]) -> bool {
    field
        .axes_governing()
        .iter()
        .any(|axis| registered.contains(axis))
}

fn field_values_differ(field: VerdictField, left: &LayoutVerdict, right: &LayoutVerdict) -> bool {
    match field {
        VerdictField::Profile => left.profile != right.profile,
        VerdictField::VerticalSpace => left.vertical_space != right.vertical_space,
        VerdictField::Navigation => left.navigation != right.navigation,
        VerdictField::Chrome => left.chrome != right.chrome,
        VerdictField::Runway => left.runway != right.runway,
        VerdictField::ChartInventory => left.chart_inventory != right.chart_inventory,
        VerdictField::Details => left.details != right.details,
        VerdictField::DeviceNavigation => left.device_navigation != right.device_navigation,
        VerdictField::SidebarWidth => left.sidebar_width != right.sidebar_width,
        VerdictField::StatsWidth => left.stats_width != right.stats_width,
        VerdictField::MainWidth => left.main_width != right.main_width,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_parity.rs"]
mod tests;
