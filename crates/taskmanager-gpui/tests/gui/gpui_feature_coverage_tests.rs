//! Feature-coverage declaration gate for the GPUI reference shape (P1):
//! totality, the reference role, and the honest-undelivered rule.
//!
//! The test proves declaration behavior - the registry is covered exactly
//! once, the shape owns the reference role for delivered features, and every
//! undelivered item is a reasoned `Unsupported` gap - never source text.

use super::*;
use taskmanager_platform_contract::{PlatformAxis, PlatformCapabilitySurface};
use taskmanager_ui_contract::{
    FeatureCoverageFindingKind, FeaturePlatformLedger, FeaturePlatformStatus,
    PLATFORM_GATE_BASELINE, PLATFORM_GATE_POLICY, PlatformGateRule, feature_coverage_drift,
    feature_coverage_findings, feature_coverage_report, feature_platform_gate_findings,
};

/// Contract gate: every registered feature has exactly one explicit decision,
/// the fold reports no drift, and every cell is either the reference role or an
/// explained undelivered gap.
#[test]
fn declaration_covers_every_feature_once_with_a_reference_or_gap() {
    let declaration = feature_coverage_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Gpui);
    assert_eq!(declaration.entries.len(), FeatureId::ALL.len());
    let report = feature_coverage_report(&declaration);
    assert_eq!(report.len(), FeatureId::ALL.len());
    assert!(
        feature_coverage_drift(&report).is_empty(),
        "a silent feature omission is drift: {report:?}"
    );
    assert!(
        declaration.entries.iter().all(|entry| match entry.support {
            CapabilitySupport::Reference => true,
            CapabilitySupport::Unsupported { reason } => !reason.is_empty(),
            _ => false,
        }),
        "the reference shape declares Reference or a reasoned Unsupported gap"
    );
}

/// The reference shape never ports, diverges from, or delegates a feature: no
/// `ReferenceShapeCannotPort` (or any other) finding may appear.
#[test]
fn reference_shape_never_ports_or_diverges_a_feature() {
    let declaration = feature_coverage_declaration();
    let findings = feature_coverage_findings(&declaration);
    assert!(findings.is_empty(), "{findings:?}");
    assert!(
        !findings
            .iter()
            .any(|finding| finding.kind == FeatureCoverageFindingKind::ReferenceShapeCannotPort),
        "the reference shape must not port, diverge from, or delegate a feature"
    );
}

/// The undelivered set is pinned: exactly the roadmap features the reference
/// surface does not render yet, each an `Unsupported` gap. A new absence
/// cannot appear silently and the reference role cannot be fabricated for an
/// undelivered item.
#[test]
fn the_reference_undelivered_set_is_pinned() {
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
        PLATFORM_GATE_BASELINE.missing_ceiling(FrontendShape::Gpui),
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
