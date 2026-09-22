//! Feature-coverage declaration gate for the Bevy shape (P1): totality, the
//! explicit decision set, and the honest absence list.
//!
//! The test proves declaration behavior - every registered feature carries a
//! decision and the absence set is pinned so a coverage claim cannot change
//! silently - never source text.

use super::*;
use taskmanager_ui_contract::{
    CapabilitySupport, feature_coverage_drift, feature_coverage_findings, feature_coverage_report,
};

/// Contract gate: every feature has exactly one explicit decision and every
/// deliberate absence carries its driver. An empty findings list is the only
/// passing state.
#[test]
fn declaration_is_total_and_every_absence_is_registered() {
    let declaration = feature_coverage_declaration();
    assert_eq!(declaration.frontend, FrontendShape::Bevy);
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
/// a non-empty reason. The one deliberate reduction (thermal temperature
/// without the source distinction) is pinned too.
#[test]
fn the_unsupported_feature_set_is_pinned() {
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
    assert_eq!(divergent, ["power.thermal-zones"]);

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
            "handles.deleted-file-watch",
            "threads.context-switch-rates",
            "network.listening-port-topology",
            "hardware.cpu-cache-topology",
            "hardware.numa-memory-distribution",
            "power.rapl-draw",
            "pressure.use-attribution",
            "ipc.dbus-service-topology",
            "ipc.dbus-introspection",
            "ipc.pipe-deadlock",
            "tracing.pmu-counters",
            "tracing.syscall-distribution",
            "tracing.slow-syscall-trap",
            "history.multi-format-export",
            "history.time-travel-scrubber",
        ]
    );
}
