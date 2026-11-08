//! The per-shape declaration tables: one row per axis per frontend, each
//! divergent or absent row carrying its architecture driver.
//!
//! The tables ARE the authority for the numbers; the resolvers in the parent
//! module read them, and [`super::layout_threshold_findings`] proves they are
//! exhaustive, unique and reason-bearing.

use crate::keybindings::FrontendShape;
use crate::presentation::thresholds::{
    AxisDecision, LayoutAxis, LayoutThresholdDeclaration, LayoutThresholdEntry,
};

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
    entry(LayoutAxis::PerformanceStatsMinWidth, AxisDecision::Shared),
    entry(LayoutAxis::PerformanceStatsMaxWidth, AxisDecision::Shared),
    entry(LayoutAxis::PerformanceSidebarMinWidth, AxisDecision::Shared),
    entry(LayoutAxis::SidebarPreferenceWidth, AxisDecision::Shared),
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
