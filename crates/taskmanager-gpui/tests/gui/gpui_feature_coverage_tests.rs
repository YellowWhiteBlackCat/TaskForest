//! Feature-coverage declaration gate for the GPUI reference shape (P1):
//! totality, the reference role, and the honest-undelivered rule.
//!
//! The test proves declaration behavior - the registry is covered exactly
//! once, the shape owns the reference role for delivered features, and every
//! undelivered item is a reasoned `Unsupported` gap - never source text.

use super::*;
use taskmanager_app_host::native_capability_surface;
use taskmanager_platform_contract::PlatformAxis;
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

/// P5 M3.4 hard gate against the REAL layer-B declaration of the selected
/// native adapter, with the committed feature-level evidence anchors attached.
///
/// The first anchor batch (`handles.enumeration`, `threads.topology`,
/// `services.lifecycle-control`, `services.dependency-dag`) plus the second
/// batch (W14-A: `gpu.engine-utilization`, `memory.breakdown-rss-pss`,
/// `storage.device-topology`, `services.inventory`,
/// `security.posix-capabilities`, `security.sandbox-detection`,
/// `hardware.heterogeneous-cores`, `hardware.core-frequency` where this shape
/// proves the delivered surface) land in `scripts/parity/feature_evidence.tsv`:
/// every anchored `(feature, frontend)` cell folds to `Ready` on the platform
/// axis whose static source surface is complete and carries the hand-declared
/// nextest anchor. G2 is not relaxed: a source-complete delivery cell without a
/// committed anchor is still refused, so the only open rule stays the evidence
/// closure over the un-anchored remainder. The committed anchored census for
/// this shape is pinned below; `Ready` itself stays host-derived. The third
/// survey (W15-A) added no anchor for this shape and recorded its remaining
/// near-misses (SMART, swap throughput, IOPS/queue/latency, battery, thermal
/// zones, log stream) as explicit `pending` gaps in the same table.
#[test]
fn the_committed_feature_anchors_produce_the_first_ready_batch_for_this_shape() {
    let declaration = feature_coverage_declaration();
    let sources = native_capability_surface();
    let anchors = PLATFORM_GATE_POLICY.feature_evidence();
    assert!(
        anchors.findings().is_empty(),
        "the committed evidence table must be structurally clean: {:?}",
        anchors.findings()
    );

    let ledger = FeaturePlatformLedger::from_declarations_with_evidence(
        std::slice::from_ref(&declaration),
        &sources,
        PLATFORM_GATE_POLICY.evidence_closure(&sources),
    );
    assert_eq!(ledger.len(), FeatureId::ALL.len() * PlatformAxis::ALL.len());

    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(
        findings
            .iter()
            .all(|finding| finding.rule() == PlatformGateRule::G2),
        "the P4 evidence closure is the only rule that may be open: {findings:?}"
    );

    // The first real `Ready` batch: every committed anchor for this shape
    // materializes exactly once, on the axis whose source commitment is
    // complete, and the folded cell carries the committed test id. An anchor
    // may only land on a declared delivery.
    let mut materialized = 0usize;
    for row in anchors
        .rows_for(declaration.frontend)
        .filter(|row| row.is_anchored())
    {
        let delivered = declaration
            .entries
            .iter()
            .find(|entry| entry.feature == row.feature)
            .is_some_and(|entry| {
                matches!(
                    entry.support,
                    CapabilitySupport::Reference
                        | CapabilitySupport::Ported
                        | CapabilitySupport::Native { .. }
                )
            });
        assert!(
            delivered,
            "{}: an anchored row must target a declared delivery",
            row.feature.id()
        );

        let ready: Vec<_> = PlatformAxis::ALL
            .iter()
            .filter_map(|platform| ledger.cell(row.feature, declaration.frontend, *platform))
            .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Ready))
            .collect();
        if ready.is_empty() {
            assert!(
                ledger
                    .iter()
                    .filter(|cell| cell.feature == row.feature)
                    .all(|cell| !cell.has_evidence()),
                "{}: an anchor must not attach where the source commitment is incomplete",
                row.feature.id()
            );
            continue;
        }
        assert_eq!(
            ready.len(),
            1,
            "{}: exactly one platform axis owns the complete source surface",
            row.feature.id()
        );
        assert_eq!(
            ready[0].evidence,
            row.anchor,
            "{}: the Ready cell must carry the committed anchor",
            row.feature.id()
        );
        materialized += 1;
    }
    assert!(
        materialized > 0,
        "the first real Ready batch must be non-empty on the supported host"
    );
    // The committed anchored census for this shape is pinned: a batch change
    // must move this number in the same change. `Ready` itself stays
    // host-derived (an anchor materializes only on the axis whose source
    // commitment is complete), so the count above is never hardcoded.
    assert_eq!(
        anchors
            .rows_for(declaration.frontend)
            .filter(|row| row.is_anchored())
            .count(),
        7,
        "the committed anchored census for this shape moved"
    );
    let admitted = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Ready) && cell.has_evidence())
        .count();
    assert_eq!(
        admitted, materialized,
        "Ready == the committed anchor batch"
    );

    // G2 is not relaxed: the refused remainder is exactly the source-complete
    // cells without a usable committed anchor.
    let refused = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Ready) && !cell.has_evidence())
        .count();
    assert_eq!(
        findings.len(),
        refused,
        "G2 must refuse exactly the source-complete cells without a usable anchor"
    );

    // G6 ceilings and the grid are unchanged by the evidence batch.
    let missing = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Missing(_)))
        .count();
    assert_eq!(
        missing,
        PLATFORM_GATE_BASELINE.missing_ceiling(FrontendShape::Gpui),
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
