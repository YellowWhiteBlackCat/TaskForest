//! Feature-coverage declaration gate for the TUI shape (P1): totality, the
//! explicit decision set, and the honest absence/divergence list.
//!
//! The test proves declaration behavior - every registered feature carries a
//! decision and the difference set is pinned so a coverage claim cannot
//! change silently - never source text.

use super::*;
use taskmanager_ui_contract::{
    CapabilitySupport, feature_coverage_drift, feature_coverage_findings, feature_coverage_report,
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
