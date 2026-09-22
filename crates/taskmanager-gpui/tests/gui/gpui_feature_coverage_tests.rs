//! Feature-coverage declaration gate for the GPUI reference shape (P1):
//! totality, the reference role, and the honest-undelivered rule.
//!
//! The test proves declaration behavior - the registry is covered exactly
//! once, the shape owns the reference role for delivered features, and every
//! undelivered item is a reasoned `Unsupported` gap - never source text.

use super::*;
use taskmanager_ui_contract::{
    FeatureCoverageFindingKind, feature_coverage_drift, feature_coverage_findings,
    feature_coverage_report,
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
            "memory.vma-map",
            "memory.leak-trend",
            "threads.context-switch-rates",
            "network.listening-port-topology",
            "hardware.numa-memory-distribution",
            "pressure.use-attribution",
            "ipc.dbus-service-topology",
            "ipc.dbus-introspection",
            "ipc.pipe-deadlock",
            "tracing.pmu-counters",
            "tracing.syscall-distribution",
            "tracing.slow-syscall-trap",
            "history.time-travel-scrubber",
        ]
    );
}
