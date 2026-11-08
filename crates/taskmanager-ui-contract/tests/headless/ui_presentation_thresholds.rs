use crate::FrontendShape;
use crate::presentation::thresholds::{
    AxisDecision, DECLARATIONS, LayoutAxis, LayoutDebtCell, LayoutThresholdDeclaration,
    LayoutThresholdEntry, LayoutThresholdFinding, LayoutThresholdFindingKind,
    UNRECORDED_DEBT_PREFIX, bands, decision, declaration_findings, divergent_axes, effective_band,
    layout_debt_census, layout_threshold_findings, number, optional,
};

/// The debt census at the time this contract was introduced. A new debt cell
/// fails the gate; retiring one requires lowering this baseline in the SAME
/// change that removes it. The equality is a ratchet, not an allowlist.
const DEBT_BASELINE: &[LayoutDebtCell] = &[
    cell(FrontendShape::Iced, LayoutAxis::StackedDetailsMinHeight),
    cell(FrontendShape::Iced, LayoutAxis::PerformanceStatsMinWidth),
    cell(FrontendShape::Iced, LayoutAxis::PerformanceStatsMaxWidth),
    cell(FrontendShape::Iced, LayoutAxis::PerformanceSidebarMinWidth),
    cell(FrontendShape::Iced, LayoutAxis::MinimumWindowWidth),
    cell(FrontendShape::Iced, LayoutAxis::MinimumWindowHeight),
    cell(FrontendShape::Bevy, LayoutAxis::ConstrainedContentHeight),
    cell(FrontendShape::Bevy, LayoutAxis::GenerousContentHeight),
    cell(FrontendShape::Bevy, LayoutAxis::ChartsRungContentHeight),
    cell(FrontendShape::Bevy, LayoutAxis::StackedDetailsMinHeight),
    cell(FrontendShape::Bevy, LayoutAxis::RootChromeSingleRowWidth),
    cell(FrontendShape::Bevy, LayoutAxis::NavigationHorizontalInset),
    cell(FrontendShape::Bevy, LayoutAxis::VerticalChromeReserve),
    cell(FrontendShape::Bevy, LayoutAxis::HorizontalChromeReserve),
];

const fn cell(frontend: FrontendShape, axis: LayoutAxis) -> LayoutDebtCell {
    LayoutDebtCell { frontend, axis }
}

#[test]
fn every_shape_declares_every_axis_exactly_once() {
    let findings: Vec<LayoutThresholdFinding> = layout_threshold_findings();
    assert!(
        findings.is_empty(),
        "threshold registry findings: {findings:?}"
    );
    assert_eq!(DECLARATIONS.len(), FrontendShape::ALL.len());
    for shape in FrontendShape::ALL {
        let declaration = declaration_for(shape);
        assert_eq!(declaration.entries.len(), LayoutAxis::ALL.len());
        for axis in LayoutAxis::ALL {
            assert!(
                decision(shape, axis).is_some(),
                "{} has no answer for {}",
                shape.name(),
                axis.label()
            );
        }
    }
}

#[test]
fn every_divergence_carries_a_reason_and_the_reference_shape_spends_the_baseline() {
    for shape in FrontendShape::ALL {
        for axis in LayoutAxis::ALL {
            let answer = decision(shape, axis).expect("registry is exhaustive");
            match answer {
                AxisDecision::Shared => {
                    assert!(axis.baseline().is_some());
                    assert_eq!(
                        number(shape, axis),
                        axis.baseline().expect("shared band always has a baseline")
                    );
                }
                AxisDecision::Own { value, reason } => {
                    assert!(!reason.trim().is_empty());
                    assert!(
                        value.is_finite() && value >= 0.0,
                        "an inset of zero is a lawful measured band; a negative one is not"
                    );
                    assert_eq!(number(shape, axis), value);
                    assert_eq!(effective_band(shape, axis), Some(value));
                }
                AxisDecision::Absent { reason } => {
                    assert!(!reason.trim().is_empty());
                    assert!(optional(shape, axis).is_none() || axis.baseline().is_some());
                }
            }
        }
    }
    assert_eq!(
        bands(FrontendShape::Gpui).constrained_content_height,
        LayoutAxis::ConstrainedContentHeight
            .baseline()
            .expect("reference shape spends the shared band")
    );
}

#[test]
fn the_two_recorded_legacy_differences_are_registered_not_erased() {
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::RootChromeSingleRowWidth),
        1320.0,
        "1320/1600 is a registered decision, not a coincidental literal"
    );
    assert_eq!(
        optional(FrontendShape::Gpui, LayoutAxis::RootChromeSingleRowWidth),
        None,
        "GPUI composes its chrome from measured regions instead of spending this band"
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::ConstrainedContentHeight),
        640.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::ConstrainedContentHeight),
        700.0,
        "640/700 is registered with the scaffold driver"
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::GenerousContentHeight),
        900.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::GenerousContentHeight),
        960.0
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::ChartsRungContentHeight),
        520.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::ChartsRungContentHeight),
        650.0,
        "the per-core matrix rung keeps its measured Iced band"
    );
    // The bands the frontends bind into their own `const` lengths. Each value
    // used to be restated as a frontend mirror constant, so the registry row
    // is now the only address — and this is where the number is pinned.
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::PerformanceStatsMinWidth),
        236.0
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::PerformanceStatsMaxWidth),
        280.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::PerformanceStatsMinWidth),
        260.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::PerformanceStatsMaxWidth),
        320.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::PerformanceSidebarMinWidth),
        248.0
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::NavigationHorizontalInset),
        12.0
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::PerformanceSidebarMinWidth),
        220.0
    );
    assert_eq!(
        number(FrontendShape::Gpui, LayoutAxis::StackedDetailsMinHeight),
        520.0,
        "the reference rail floor the GPUI mirror used to restate"
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::VerticalChromeReserve),
        128.0
    );
    assert_eq!(
        number(FrontendShape::Iced, LayoutAxis::HorizontalChromeReserve),
        20.0
    );
    let registered = divergent_axes(FrontendShape::Gpui, FrontendShape::Iced);
    for axis in [
        LayoutAxis::ConstrainedContentHeight,
        LayoutAxis::GenerousContentHeight,
        LayoutAxis::ChartsRungContentHeight,
        LayoutAxis::RootChromeSingleRowWidth,
        LayoutAxis::PerformanceStatsMinWidth,
        LayoutAxis::PerformanceStatsMaxWidth,
        LayoutAxis::PerformanceSidebarMinWidth,
        LayoutAxis::SidebarPreferenceWidth,
        LayoutAxis::NavigationHorizontalInset,
        LayoutAxis::MinimumWindowWidth,
        LayoutAxis::MinimumWindowHeight,
        LayoutAxis::VerticalChromeReserve,
        LayoutAxis::HorizontalChromeReserve,
        LayoutAxis::StackedDetailsMinHeight,
    ] {
        assert!(
            registered.contains(&axis),
            "{} must be a registered explanation for a differing verdict",
            axis.label()
        );
    }
}

#[test]
fn the_debt_census_is_exact_and_never_grows() {
    let live = layout_debt_census();
    assert_eq!(
        live, DEBT_BASELINE,
        "new unexplained divergence or retired debt"
    );
    for cell in &live {
        let answer = decision(cell.frontend, cell.axis).expect("debt cell is declared");
        let reason = answer
            .reason()
            .expect("a debt cell always carries a reason");
        assert!(
            reason.starts_with(UNRECORDED_DEBT_PREFIX),
            "{}:{} is listed as debt but has a recorded driver: {reason}",
            cell.frontend.name(),
            cell.axis.label()
        );
    }
}

#[test]
fn registry_mutations_are_detected_by_the_findings_gate() {
    let gpui = declaration_for(FrontendShape::Gpui);
    let iced = declaration_for(FrontendShape::Iced);
    let bevy = declaration_for(FrontendShape::Bevy);
    let tui = declaration_for(FrontendShape::Tui);

    let without_one_axis: Vec<LayoutThresholdEntry> = iced
        .entries
        .iter()
        .copied()
        .filter(|entry| entry.axis != LayoutAxis::ConstrainedContentHeight)
        .collect();
    let duplicated_axis: Vec<LayoutThresholdEntry> = gpui
        .entries
        .iter()
        .copied()
        .chain(gpui.entries.iter().copied().take(1))
        .collect();
    let empty_reason: Vec<LayoutThresholdEntry> = tui
        .entries
        .iter()
        .map(|entry| LayoutThresholdEntry {
            axis: entry.axis,
            decision: AxisDecision::Absent { reason: "" },
        })
        .collect();
    let redundant_own: Vec<LayoutThresholdEntry> = bevy
        .entries
        .iter()
        .map(|entry| {
            if entry.axis == LayoutAxis::MinimumWindowWidth {
                LayoutThresholdEntry {
                    axis: entry.axis,
                    decision: AxisDecision::Own {
                        value: 720.0,
                        reason: "equal to the baseline, so this is drift bait",
                    },
                }
            } else {
                *entry
            }
        })
        .collect();
    let shared_without_baseline: Vec<LayoutThresholdEntry> = iced
        .entries
        .iter()
        .map(|entry| {
            if entry.axis == LayoutAxis::RootChromeSingleRowWidth {
                LayoutThresholdEntry {
                    axis: entry.axis,
                    decision: AxisDecision::Shared,
                }
            } else {
                *entry
            }
        })
        .collect();

    let mutated = [
        LayoutThresholdDeclaration {
            frontend: FrontendShape::Gpui,
            entries: &duplicated_axis,
        },
        LayoutThresholdDeclaration {
            frontend: FrontendShape::Iced,
            entries: &without_one_axis,
        },
        LayoutThresholdDeclaration {
            frontend: FrontendShape::Bevy,
            entries: &redundant_own,
        },
        LayoutThresholdDeclaration {
            frontend: FrontendShape::Tui,
            entries: &empty_reason,
        },
    ];
    let findings = declaration_findings(&mutated);
    assert_eq!(
        findings.len(),
        LayoutAxis::ALL.len() + 3,
        "every TUI axis loses its reason, one Iced axis goes missing, one GPUI axis is \
         duplicated, one Bevy axis restates the baseline"
    );
    assert!(findings.iter().any(|finding| {
        finding.frontend == FrontendShape::Iced
            && finding.axis == LayoutAxis::ConstrainedContentHeight
            && finding.kind == LayoutThresholdFindingKind::MissingAxis
    }));
    assert!(findings.iter().any(|finding| {
        finding.frontend == FrontendShape::Gpui
            && finding.kind == LayoutThresholdFindingKind::DuplicateAxis
    }));
    assert!(findings.iter().any(|finding| {
        finding.frontend == FrontendShape::Bevy
            && finding.kind == LayoutThresholdFindingKind::RedundantOwn
    }));
    assert!(
        findings
            .iter()
            .filter(|finding| finding.kind == LayoutThresholdFindingKind::EmptyReason)
            .count()
            == LayoutAxis::ALL.len()
    );

    let shared_only = [LayoutThresholdDeclaration {
        frontend: FrontendShape::Iced,
        entries: &shared_without_baseline,
    }];
    let shared_findings = declaration_findings(&shared_only);
    assert!(shared_findings.iter().any(|finding| {
        finding.axis == LayoutAxis::RootChromeSingleRowWidth
            && finding.kind == LayoutThresholdFindingKind::SharedWithoutBaseline
    }));
    assert!(layout_threshold_findings().is_empty());
}

fn declaration_for(frontend: FrontendShape) -> &'static LayoutThresholdDeclaration<'static> {
    DECLARATIONS
        .iter()
        .find(|declaration| declaration.frontend == frontend)
        .expect("every shape has a declaration")
}
