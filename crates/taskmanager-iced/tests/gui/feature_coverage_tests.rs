//! Feature-coverage declaration gate for the Iced shape (P1): totality, the
//! explicit decision set, and the honest absence list.
//!
//! The test proves declaration behavior - every registered feature carries a
//! decision and the absence set is pinned so a coverage claim cannot change
//! silently - never source text.

use super::*;
use taskmanager_platform_contract::{PlatformAxis, PlatformCapabilitySurface};
use taskmanager_ui_contract::{
    CapabilitySupport, FeaturePlatformLedger, FeaturePlatformStatus, PLATFORM_GATE_BASELINE,
    PLATFORM_GATE_POLICY, PlatformGateRule, feature_coverage_drift, feature_coverage_findings,
    feature_coverage_report, feature_platform_gate_findings,
};

/// Contract gate: every feature has exactly one explicit decision and every
/// deliberate absence carries its driver. An empty findings list is the only
/// passing state.
#[test]
fn declaration_is_total_and_every_absence_is_registered() {
    let declaration = feature_coverage_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Iced);
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

/// The unsupported set is exactly the features this shape does not render; a
/// new absence cannot appear silently and an unsupported cell always carries
/// a non-empty reason.
#[test]
fn the_unsupported_feature_set_is_pinned() {
    let declaration = feature_coverage_declaration();
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
            "handles.type-classification",
            "handles.deleted-file-watch",
            "handles.fd-limit-saturation",
            "handles.reverse-path-search",
            "threads.context-switch-rates",
            "network.listening-port-topology",
            "network.socket-queue-backlog",
            "hardware.numa-memory-distribution",
            "hardware.core-frequency",
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

/// P5 M3.4 hard gate: this shape's real declaration folds over
/// `FeatureId::ALL x PlatformAxis::ALL` and must satisfy G1-G6 against the
/// product gate policy.
///
/// Layer-B per-platform source declarations have not landed yet, so the fold
/// runs against an empty `PlatformCapabilitySurface`: no cell may claim
/// `Ready`, which is the honest shape today. The gate still proves the
/// exhaustive grid, the typed reasons, zero false success, the bidirectional
/// capability binding, and this shape's `Missing` ceiling against the live
/// baseline.
#[test]
fn the_three_axis_gate_is_green_for_this_shape() {
    let declaration = feature_coverage_declaration();
    let sources = PlatformCapabilitySurface::new();
    let ledger = FeaturePlatformLedger::from_declaration(&declaration, &sources);
    assert_eq!(ledger.len(), FeatureId::ALL.len() * PlatformAxis::ALL.len());
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(findings.is_empty(), "{findings:?}");

    let missing = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Missing(_)))
        .count();
    assert_eq!(
        missing,
        PLATFORM_GATE_BASELINE.missing_ceiling(FrontendShape::Iced),
        "a shape that changes its gap set must move its baseline in the same change"
    );
    assert!(
        !ledger
            .iter()
            .any(|cell| matches!(cell.status, FeaturePlatformStatus::Ready)),
        "an undeclared platform source must never fold to Ready"
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

    let sources = PlatformCapabilitySurface::new();
    let ledger = FeaturePlatformLedger::from_declaration(&sabotaged, &sources);
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule() == PlatformGateRule::G3),
        "{findings:?}"
    );
}
