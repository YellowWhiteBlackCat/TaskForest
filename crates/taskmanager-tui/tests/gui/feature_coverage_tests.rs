//! Feature-coverage declaration gate for the TUI shape (P1): totality, the
//! explicit decision set, and the honest absence/divergence list.
//!
//! The test proves declaration behavior - every registered feature carries a
//! decision and the difference set is pinned so a coverage claim cannot
//! change silently - never source text.

use super::*;
use taskmanager_app_host::native_capability_surface;
use taskmanager_platform_contract::PlatformAxis;
use taskmanager_ui_contract::{
    CapabilitySupport, FeaturePlatformLedger, FeaturePlatformStatus, NO_EVIDENCE,
    PLATFORM_GATE_BASELINE, PLATFORM_GATE_POLICY, PlatformGateRule, feature_coverage_drift,
    feature_coverage_findings, feature_coverage_report, feature_platform_gate_findings,
};

/// Contract gate: every feature has exactly one explicit decision and every
/// divergence/absence carries its terminal driver. An empty findings list is
/// the only passing state.
#[test]
fn declaration_is_total_and_every_difference_is_registered() {
    let declaration = feature_coverage_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Tui);
    assert_eq!(declaration.entries.len(), FeatureId::ALL.len());
    let report = feature_coverage_report(&declaration);
    assert_eq!(report.len(), FeatureId::ALL.len());
    assert!(
        feature_coverage_drift(&report).is_empty(),
        "a silent feature omission is drift: {report:?}"
    );
    let findings = feature_coverage_findings(&declaration);
    assert!(findings.is_empty(), "{findings:?}");
}

/// The terminal-limit set is pinned: no deliberate divergences, and exactly
/// the features this shape does not render as typed absences. A new difference
/// cannot appear silently, and every non-`Ported` cell carries a non-empty
/// reason.
#[test]
fn the_terminal_difference_set_is_pinned() {
    let declaration = feature_coverage_declaration();
    let divergent: Vec<&str> = declaration
        .entries
        .iter()
        .filter_map(|entry| match entry.support {
            CapabilitySupport::Divergent { reason } if !reason.is_empty() => {
                Some(entry.feature.id())
            }
            _ => None,
        })
        .collect();
    assert!(divergent.is_empty(), "{divergent:?}");

    let unsupported: Vec<&str> = declaration
        .entries
        .iter()
        .filter_map(|entry| match entry.support {
            CapabilitySupport::Unsupported { reason } if !reason.is_empty() => {
                Some(entry.feature.id())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        unsupported,
        [
            "process.ancestor-lineage",
            "memory.vma-map",
            "memory.leak-trend",
            "handles.fd-limit-saturation",
            "handles.reverse-path-search",
            "threads.context-switch-rates",
            "network.listening-port-topology",
            "network.socket-queue-backlog",
            "hardware.numa-memory-distribution",
            "gpu.process-attribution",
            "power.thermal-throttle-events",
            "pressure.use-attribution",
            "pressure.unresponsive-apps",
            "ipc.dbus-service-topology",
            "ipc.dbus-introspection",
            "ipc.pipe-deadlock",
            "ipc.shared-memory-segments",
            "ipc.uds-peer-topology",
            "tracing.pmu-counters",
            "tracing.syscall-distribution",
            "tracing.slow-syscall-trap",
            "tracing.process-event-trace",
            "tracing.cpu-flame-graph",
            "history.time-travel-scrubber",
            "history.percentile-aggregation",
            "history.chart-image-export",
        ]
    );
}

/// P5 M3.4 hard gate against the REAL layer-B declaration of the selected
/// native adapter (reached through the app-host composition edge).
///
/// The real declaration makes source-complete cells appear on the host
/// platform. No P4 behaviour anchor has landed yet, so G2 refuses every one of
/// them: the gate's only open rule is the evidence closure, and no cell is a
/// proven delivery claim. Every other rule - the G1 grid, the G3 typed reasons,
/// G4 zero false success, the G5 binding census, and this shape's G6 ceilings -
/// is green, so the open set is exactly the source-complete set: neither a new
/// platform source nor a lost one can hide behind the evidence gap.
#[test]
fn the_three_axis_gate_reports_only_the_evidence_gap_for_this_shape() {
    let declaration = feature_coverage_declaration();
    let sources = native_capability_surface();
    // Evidence seam (P4): the cross-frontend manifest carries no `feature` row
    // yet, so every cell answers `NO_EVIDENCE`. When anchors land, this closure
    // returns them and the census assertions below must move in the same change.
    let ledger = FeaturePlatformLedger::from_declarations_with_evidence(
        std::slice::from_ref(&declaration),
        &sources,
        |_, _, _| NO_EVIDENCE,
    );
    assert_eq!(ledger.len(), FeatureId::ALL.len() * PlatformAxis::ALL.len());

    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    let source_complete = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Ready))
        .count();
    assert!(
        source_complete > 0,
        "the selected adapter's declaration must make some cells source-complete"
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.rule() == PlatformGateRule::G2),
        "the P4 evidence closure is the only rule that may be open: {findings:?}"
    );
    assert_eq!(
        findings.len(),
        source_complete,
        "G2 must refuse exactly the source-complete cells and nothing else"
    );
    assert!(
        ledger.iter().all(|cell| !cell.has_evidence()),
        "a P4 behaviour anchor appeared: move the evidence-census assertions in the same change"
    );

    let missing = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Missing(_)))
        .count();
    assert_eq!(
        missing,
        PLATFORM_GATE_BASELINE.missing_ceiling(FrontendShape::Tui),
        "a shape that changes its gap set must move its baseline in the same change"
    );
    assert_eq!(
        ledger
            .iter()
            .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Unregistered(_)))
            .count(),
        0,
        "every remaining vocabulary-gap feature is declared a frontend gap, so no cell \
         may claim delivery of a feature with no capability identity"
    );
}

/// Counterexample self-check: a delivered entry narrowed to an unexplained
/// divergence is reported red by G3, so the gate above is not a rubber stamp.
#[test]
fn the_three_axis_gate_rejects_an_untyped_declaration() {
    let mut sabotaged = feature_coverage_declaration();
    let victim = sabotaged
        .entries
        .iter_mut()
        .find(|entry| {
            matches!(
                entry.support,
                CapabilitySupport::Reference
                    | CapabilitySupport::Ported
                    | CapabilitySupport::Native { .. }
            ) && !entry.feature.platform_binding().capabilities().is_empty()
        })
        .expect("a capability-backed delivered entry");
    victim.support = CapabilitySupport::Divergent { reason: "" };

    let sources = native_capability_surface();
    let ledger = FeaturePlatformLedger::from_declaration(&sabotaged, &sources);
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule() == PlatformGateRule::G3),
        "{findings:?}"
    );
}
