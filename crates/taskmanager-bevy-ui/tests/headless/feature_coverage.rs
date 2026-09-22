//! Feature-coverage declaration gate for the Bevy shape (P1): totality, the
//! explicit decision set, and the honest absence list.
//!
//! The test proves declaration behavior - every registered feature carries a
//! decision and the absence set is pinned so a coverage claim cannot change
//! silently - never source text.

use super::*;
use taskmanager_app_host::native_capability_surface;
use taskmanager_platform_contract::PlatformAxis;
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
/// a non-empty reason. The deliberate reductions (thermal temperature without
/// the source distinction, battery charge/power without the voltage/health/
/// cycle detail) are pinned too.
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
    assert_eq!(
        divergent,
        ["power.thermal-zones", "power.battery-inventory"]
    );

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
            "handles.deleted-file-watch",
            "handles.fd-limit-saturation",
            "handles.reverse-path-search",
            "threads.context-switch-rates",
            "network.listening-port-topology",
            "network.socket-queue-backlog",
            "hardware.cpu-cache-topology",
            "hardware.numa-memory-distribution",
            "hardware.heterogeneous-cores",
            "hardware.core-frequency",
            "gpu.process-attribution",
            "power.rapl-draw",
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
            "history.multi-format-export",
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
/// near-misses (log stream, namespace audit) as explicit `pending` gaps in the
/// same table. The fifth batch (W16-C) anchored those two cells together with
/// this shape's remaining observation gaps: the swap-in/out throughput rates,
/// the IOPS/response/queue/service rows, the six-family SMART evidence, the
/// projected partition rows, the per-adapter GPU blocks with their
/// per-engine rows, and the page-fault / anonymous huge-page counters. The
/// W23-A batch anchored `memory.breakdown-rss-pss` for this shape
/// after the details overview gained the derived-shared (`RSS - USS`) row, so
/// the narrowed resident/proportional/private/derived-shared definition is
/// proven by one details-overview fold.
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
        17,
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
        PLATFORM_GATE_BASELINE.missing_ceiling(FrontendShape::Bevy),
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
