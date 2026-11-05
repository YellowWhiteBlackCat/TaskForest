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

const fn entry(axis: LayoutAxis, decision: AxisDecision) -> LayoutThresholdEntry {
    LayoutThresholdEntry { axis, decision }
}

const GPUI_ENTRIES: &[LayoutThresholdEntry] = &[
    entry(LayoutAxis::ConstrainedContentHeight, AxisDecision::Shared),
    entry(LayoutAxis::GenerousContentHeight, AxisDecision::Shared),
    entry(LayoutAxis::ChartsRungContentHeight, AxisDecision::Shared),
    entry(LayoutAxis::StackedDetailsMinHeight, AxisDecision::Shared),
    entry(
        LayoutAxis::RootChromeSingleRowWidth,
        AxisDecision::Absent {
            reason: "the GPUI root composes chrome from the measured titlebar, alert band and \
                     navigation orientation in `FrameBudget`, so no width threshold decides it",
        },
    ),
    entry(LayoutAxis::PerformanceStatsMinWidth, AxisDecision::Shared),
    entry(LayoutAxis::PerformanceStatsMaxWidth, AxisDecision::Shared),
    entry(LayoutAxis::PerformanceSidebarMinWidth, AxisDecision::Shared),
    entry(LayoutAxis::SidebarPreferenceWidth, AxisDecision::Shared),
    entry(LayoutAxis::NavigationHorizontalInset, AxisDecision::Shared),
    entry(LayoutAxis::MinimumWindowWidth, AxisDecision::Shared),
    entry(LayoutAxis::MinimumWindowHeight, AxisDecision::Shared),
    entry(
        LayoutAxis::VerticalChromeReserve,
        AxisDecision::Absent {
            reason: "the GPUI frame budget subtracts the real titlebar, alert and horizontal \
                     navigation heights per frame; a constant reserve would double-count them",
        },
    ),
    entry(
        LayoutAxis::HorizontalChromeReserve,
        AxisDecision::Absent {
            reason: "the GPUI frame budget subtracts the real navigation rail width plus its \
                     measured inset",
        },
    ),
];

const ICED_ENTRIES: &[LayoutThresholdEntry] = &[
    entry(
        LayoutAxis::ConstrainedContentHeight,
        AxisDecision::Own {
            value: 700.0,
            reason: "the iced scaffold spends its own nav row plus footer band, so the shared \
                     page-slot height is measured after the typed chrome reserve",
        },
    ),
    entry(
        LayoutAxis::GenerousContentHeight,
        AxisDecision::Own {
            value: 960.0,
            reason: "keeps the same gap above the Constrained band the iced scaffold measures",
        },
    ),
    entry(
        LayoutAxis::ChartsRungContentHeight,
        AxisDecision::Own {
            value: 650.0,
            reason: "the iced per-core matrix composes one band lower than the coarse \
                     Constrained boundary, the window where GPUI shows it (ICED-024-8)",
        },
    ),
    entry(
        LayoutAxis::StackedDetailsMinHeight,
        AxisDecision::Absent {
            reason: "unrecorded: the iced stacked statistics rail keeps one fixed readable \
                     height instead of hiding under a content-height floor; convergence candidate",
        },
    ),
    entry(
        LayoutAxis::RootChromeSingleRowWidth,
        AxisDecision::Own {
            value: 1320.0,
            reason: "routes and actions fit one iced desktop row earlier than the Wide page \
                     profile; folding this onto the Wide band would wrap every frame in \
                     [1320, 1600) back to two rows",
        },
    ),
    entry(
        LayoutAxis::PerformanceStatsMinWidth,
        AxisDecision::Own {
            value: 260.0,
            reason: "unrecorded: inherited from the pre-port iced rail geometry; convergence \
                     candidate",
        },
    ),
    entry(
        LayoutAxis::PerformanceStatsMaxWidth,
        AxisDecision::Own {
            value: 320.0,
            reason: "unrecorded: inherited from the pre-port iced rail geometry; convergence \
                     candidate",
        },
    ),
    entry(
        LayoutAxis::PerformanceSidebarMinWidth,
        AxisDecision::Own {
            value: 248.0,
            reason: "unrecorded: inherited from the pre-port iced device-rail card width; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::SidebarPreferenceWidth,
        AxisDecision::Own {
            value: 248.0,
            reason: "iced persists no device-rail width, so the coarse constructor reports the \
                     rail's own minimum instead of a preference",
        },
    ),
    entry(
        LayoutAxis::NavigationHorizontalInset,
        AxisDecision::Own {
            value: 0.0,
            reason: "the iced scaffold's own page padding already separates rail and body, so \
                     charging the GPUI inset would double-count it",
        },
    ),
    entry(
        LayoutAxis::MinimumWindowWidth,
        AxisDecision::Absent {
            reason: "unrecorded: the iced window builder keeps the toolkit default minimum; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::MinimumWindowHeight,
        AxisDecision::Absent {
            reason: "unrecorded: the iced window builder keeps the toolkit default minimum; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::VerticalChromeReserve,
        AxisDecision::Own {
            value: 128.0,
            reason: "iced tracks the whole window, so the rung ladder is measured after the \
                     scaffold padding, spacing, nav row and footer it always spends",
        },
    ),
    entry(
        LayoutAxis::HorizontalChromeReserve,
        AxisDecision::Own {
            value: 20.0,
            reason: "the Performance workspace is measured without the body column's own \
                     two-sided scaffold padding",
        },
    ),
];

const BEVY_ENTRIES: &[LayoutThresholdEntry] = &[
    entry(
        LayoutAxis::ConstrainedContentHeight,
        AxisDecision::Absent {
            reason: "unrecorded: the Bevy shape composes Performance slots through BSN flex with \
                     rem widths and does not consume the typed page-profile bands; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::GenerousContentHeight,
        AxisDecision::Absent {
            reason: "unrecorded: no vertical capacity axis in the BSN composition yet; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::ChartsRungContentHeight,
        AxisDecision::Absent {
            reason: "unrecorded: the chart inventory follows the sidebar's own visible-row count \
                     instead of a content-height rung; convergence candidate",
        },
    ),
    entry(
        LayoutAxis::StackedDetailsMinHeight,
        AxisDecision::Absent {
            reason: "unrecorded: the statistics rail has no stacked form in this shape; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::RootChromeSingleRowWidth,
        AxisDecision::Absent {
            reason: "unrecorded: the nav strip and page chrome compose with flex in one column \
                     at every width; convergence candidate",
        },
    ),
    entry(
        LayoutAxis::PerformanceStatsMinWidth,
        AxisDecision::Absent {
            reason: "unrecorded: the statistics rail width is a rem constant in the scene, not a \
                     slot floor; convergence candidate",
        },
    ),
    entry(
        LayoutAxis::PerformanceStatsMaxWidth,
        AxisDecision::Absent {
            reason: "unrecorded: no clamped rail width in this shape yet; convergence candidate",
        },
    ),
    entry(
        LayoutAxis::PerformanceSidebarMinWidth,
        AxisDecision::Absent {
            reason: "unrecorded: the device rail admits or drops by its own rem floor; \
                     convergence candidate",
        },
    ),
    entry(
        LayoutAxis::SidebarPreferenceWidth,
        AxisDecision::Absent {
            reason: "the Bevy shape persists no device-rail width preference",
        },
    ),
    entry(
        LayoutAxis::NavigationHorizontalInset,
        AxisDecision::Absent {
            reason: "unrecorded: the rail's inset is the scaffold's own padding; convergence \
                     candidate",
        },
    ),
    entry(LayoutAxis::MinimumWindowWidth, AxisDecision::Shared),
    entry(LayoutAxis::MinimumWindowHeight, AxisDecision::Shared),
    entry(
        LayoutAxis::VerticalChromeReserve,
        AxisDecision::Absent {
            reason: "unrecorded: the Bevy root reads live Node geometry per frame; convergence \
                     candidate",
        },
    ),
    entry(
        LayoutAxis::HorizontalChromeReserve,
        AxisDecision::Absent {
            reason: "unrecorded: the Bevy root reads live Node geometry per frame; convergence \
                     candidate",
        },
    ),
];

const TUI_ENTRIES: &[LayoutThresholdEntry] = &[
    entry(
        LayoutAxis::ConstrainedContentHeight,
        AxisDecision::Absent {
            reason: "the terminal shape budgets in character cells: ratatui reports the area in \
                     rows and columns, so a logical-pixel band has no meaning (ADR-054)",
        },
    ),
    entry(
        LayoutAxis::GenerousContentHeight,
        AxisDecision::Absent {
            reason: "vertical capacity is the terminal row count, folded by the cell-grid \
                     scaffold rather than a px band",
        },
    ),
    entry(
        LayoutAxis::ChartsRungContentHeight,
        AxisDecision::Absent {
            reason: "the chart inventory folds by available terminal rows, not a px height",
        },
    ),
    entry(
        LayoutAxis::StackedDetailsMinHeight,
        AxisDecision::Absent {
            reason: "the terminal shape has no stacked statistics rail",
        },
    ),
    entry(
        LayoutAxis::RootChromeSingleRowWidth,
        AxisDecision::Absent {
            reason: "the header line and footer always occupy one terminal row each; chrome \
                     wrapping is a cell-grid decision",
        },
    ),
    entry(
        LayoutAxis::PerformanceStatsMinWidth,
        AxisDecision::Absent {
            reason: "no pinned statistics column exists in the terminal shape",
        },
    ),
    entry(
        LayoutAxis::PerformanceStatsMaxWidth,
        AxisDecision::Absent {
            reason: "no pinned statistics column exists in the terminal shape",
        },
    ),
    entry(
        LayoutAxis::PerformanceSidebarMinWidth,
        AxisDecision::Absent {
            reason: "device navigation is a selector sheet, not a rail beside the detail",
        },
    ),
    entry(
        LayoutAxis::SidebarPreferenceWidth,
        AxisDecision::Absent {
            reason: "the terminal shape persists no device-rail width preference",
        },
    ),
    entry(
        LayoutAxis::NavigationHorizontalInset,
        AxisDecision::Absent {
            reason: "the header line occupies full-width terminal rows; there is no rail inset",
        },
    ),
    entry(
        LayoutAxis::MinimumWindowWidth,
        AxisDecision::Absent {
            reason: "the terminal emulator owns the viewport; TaskForest never sets a window \
                     minimum",
        },
    ),
    entry(
        LayoutAxis::MinimumWindowHeight,
        AxisDecision::Absent {
            reason: "the terminal emulator owns the viewport; TaskForest never sets a window \
                     minimum",
        },
    ),
    entry(
        LayoutAxis::VerticalChromeReserve,
        AxisDecision::Absent {
            reason: "header, tabs and footer are subtracted as terminal rows by the scaffold",
        },
    ),
    entry(
        LayoutAxis::HorizontalChromeReserve,
        AxisDecision::Absent {
            reason: "the scaffold splits the pane rectangle directly; no px reserve is measured",
        },
    ),
];

/// Every shape's declaration. This table IS the authority: a frontend reads its
/// bands here and never restates a number.
///
/// GPUI is the reference shape (GPUI-05), so its row defines most baselines.
/// Iced's differences are the measured ones recorded when the port was
/// audited; Bevy and TUI answer every axis explicitly even where they spend no
/// budget, because a silent omission is drift.
pub const DECLARATIONS: &[LayoutThresholdDeclaration<'static>] = &[
    LayoutThresholdDeclaration {
        frontend: FrontendShape::Gpui,
        entries: GPUI_ENTRIES,
    },
    LayoutThresholdDeclaration {
        frontend: FrontendShape::Iced,
        entries: ICED_ENTRIES,
    },
    LayoutThresholdDeclaration {
        frontend: FrontendShape::Bevy,
        entries: BEVY_ENTRIES,
    },
    LayoutThresholdDeclaration {
        frontend: FrontendShape::Tui,
        entries: TUI_ENTRIES,
    },
];

/// The declaration for one shape.
#[must_use]
pub fn declaration(
    frontend: FrontendShape,
) -> Option<&'static LayoutThresholdDeclaration<'static>> {
    DECLARATIONS
        .iter()
        .find(|candidate| candidate.frontend == frontend)
}

/// One shape's decision for one axis.
#[must_use]
pub fn decision(frontend: FrontendShape, axis: LayoutAxis) -> Option<AxisDecision> {
    declaration(frontend)?
        .entries
        .iter()
        .find(|candidate| candidate.axis == axis)
        .map(|candidate| candidate.decision)
}

/// Resolve one shape's classification bands. A required axis a shape does not
/// spend reports the shared baseline, so the resolver is total and never
/// fabricates a zero; [`layout_threshold_findings`] is what proves the
/// declaration is complete.
#[must_use]
pub fn bands(frontend: FrontendShape) -> LayoutBands {
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
#[must_use]
pub fn number(frontend: FrontendShape, axis: LayoutAxis) -> f32 {
    match decision(frontend, axis) {
        Some(AxisDecision::Own { value, .. }) => value,
        _ => axis.baseline().unwrap_or(0.0),
    }
}

/// The effective optional band: `None` when the shape does not spend the axis.
#[must_use]
pub fn optional(frontend: FrontendShape, axis: LayoutAxis) -> Option<f32> {
    match decision(frontend, axis) {
        Some(AxisDecision::Own { value, .. }) => Some(value),
        Some(AxisDecision::Absent { .. }) => None,
        Some(AxisDecision::Shared) | None => axis.baseline(),
    }
}

/// One shape's drift on the threshold declaration axis: it reports whether the
/// registry is complete and self-consistent, never runtime availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutThresholdFindingKind {
    /// The shape declares nothing for this axis: a silent omission.
    MissingAxis,
    /// The shape declares this axis twice.
    DuplicateAxis,
    /// The shape claims the shared baseline for an axis that has none.
    SharedWithoutBaseline,
    /// The shape declares its own value equal to the baseline: drift bait that
    /// must be written as `Shared`.
    RedundantOwn,
    /// `Own`/`Absent` without a driver: the reason text is the register.
    EmptyReason,
}

/// One drift finding against the layout threshold registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutThresholdFinding {
    pub frontend: FrontendShape,
    pub axis: LayoutAxis,
    pub kind: LayoutThresholdFindingKind,
}

/// Validate every shape's declaration: exhaustive, unique, reason-bearing.
#[must_use]
pub fn layout_threshold_findings() -> Vec<LayoutThresholdFinding> {
    declaration_findings(DECLARATIONS)
}

/// Validate one supplied declaration table. The public gate calls this with
/// [`DECLARATIONS`]; its tests call it with mutated tables so every kind of
/// drift is detected by real behavior rather than enum equality.
#[must_use]
pub fn declaration_findings(
    declarations: &[LayoutThresholdDeclaration<'_>],
) -> Vec<LayoutThresholdFinding> {
    let mut findings = Vec::new();
    for frontend in FrontendShape::ALL {
        for axis in LayoutAxis::ALL {
            let declared: Vec<AxisDecision> = declarations
                .iter()
                .find(|candidate| candidate.frontend == frontend)
                .map(|existing| {
                    existing
                        .entries
                        .iter()
                        .filter(|candidate| candidate.axis == axis)
                        .map(|candidate| candidate.decision)
                        .collect()
                })
                .unwrap_or_default();
            match declared.len() {
                0 => findings.push(LayoutThresholdFinding {
                    frontend,
                    axis,
                    kind: LayoutThresholdFindingKind::MissingAxis,
                }),
                1 => {
                    if let Some(kind) = decision_drift(axis, declared[0]) {
                        findings.push(LayoutThresholdFinding {
                            frontend,
                            axis,
                            kind,
                        });
                    }
                }
                _ => findings.push(LayoutThresholdFinding {
                    frontend,
                    axis,
                    kind: LayoutThresholdFindingKind::DuplicateAxis,
                }),
            }
        }
    }
    findings
}

fn decision_drift(axis: LayoutAxis, decision: AxisDecision) -> Option<LayoutThresholdFindingKind> {
    match decision {
        AxisDecision::Shared if axis.baseline().is_none() => {
            Some(LayoutThresholdFindingKind::SharedWithoutBaseline)
        }
        AxisDecision::Own { reason, .. } if reason.trim().is_empty() => {
            Some(LayoutThresholdFindingKind::EmptyReason)
        }
        AxisDecision::Own { value, .. } if axis.baseline() == Some(value) => {
            Some(LayoutThresholdFindingKind::RedundantOwn)
        }
        AxisDecision::Absent { reason } if reason.trim().is_empty() => {
            Some(LayoutThresholdFindingKind::EmptyReason)
        }
        _ => None,
    }
}

/// One unexplained difference: a shape that spends a band the reference shape
/// does not, with no recorded driver. Set equality against this census is the
/// ratchet — new debt fails, retired debt must shrink the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutDebtCell {
    pub frontend: FrontendShape,
    pub axis: LayoutAxis,
}

/// The live debt census over every declared shape and axis.
#[must_use]
pub fn layout_debt_census() -> Vec<LayoutDebtCell> {
    let mut cells = Vec::new();
    for frontend in FrontendShape::ALL {
        for axis in LayoutAxis::ALL {
            if decision(frontend, axis).is_some_and(AxisDecision::is_debt) {
                cells.push(LayoutDebtCell { frontend, axis });
            }
        }
    }
    cells
}

/// The axes on which two shapes spend a different effective band. This is the
/// only lawful explanation for a differing layout verdict: a field whose
/// governing axes are NOT in this set must classify identically.
#[must_use]
pub fn divergent_axes(left: FrontendShape, right: FrontendShape) -> Vec<LayoutAxis> {
    LayoutAxis::ALL
        .iter()
        .copied()
        .filter(|axis| effective_band(left, *axis) != effective_band(right, *axis))
        .collect()
}

/// One shape's resolved band for an axis, as the classifier actually sees it.
#[must_use]
pub fn effective_band(frontend: FrontendShape, axis: LayoutAxis) -> Option<f32> {
    if axis.is_optional() {
        optional(frontend, axis)
    } else {
        Some(number(frontend, axis))
    }
}

#[cfg(test)]
#[path = "../../tests/headless/ui_presentation_thresholds.rs"]
mod tests;
