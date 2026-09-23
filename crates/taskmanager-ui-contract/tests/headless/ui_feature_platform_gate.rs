//! Hard three-axis gate tests (P5 M3.4).
//!
//! Three families:
//!
//! 1. the achieved shape is green - a complete static source surface with
//!    attached anchors passes the product policy, and today's empty layer-B
//!    surface passes it without inventing a `Ready` claim;
//! 2. one counterexample per rule G1-G6 - an inconsistent input must report
//!    red, so the gate cannot be a rubber stamp;
//! 3. the live baseline census - the accepted-debt list and the ceiling
//!    numbers are exact, so they only move when the gaps move.

use super::evidence::{FeatureEvidenceFinding, FeatureEvidenceTable};
use super::*;
use crate::capabilities::CapabilitySupport;
use crate::feature_coverage::{
    FeatureCoverageDeclaration, FeatureCoverageEntry, NO_EVIDENCE, PartialCause,
};
use taskmanager_platform_contract::RetryDisposition;

fn declaration(
    frontend: FrontendShape,
    support: impl Fn(FeatureId) -> CapabilitySupport,
) -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend,
        entries: FeatureId::ALL
            .iter()
            .map(|feature| FeatureCoverageEntry {
                feature: *feature,
                support: support(*feature),
            })
            .collect(),
    }
}

/// Every feature delivered by every shape: the static layer-B surface is the
/// only remaining variable.
fn total_declaration(frontend: FrontendShape) -> FeatureCoverageDeclaration {
    declaration(frontend, |_| {
        if frontend == FrontendShape::Gpui {
            CapabilitySupport::Reference
        } else {
            CapabilitySupport::Ported
        }
    })
}

fn all_declarations() -> Vec<FeatureCoverageDeclaration> {
    FrontendShape::ALL
        .iter()
        .copied()
        .map(total_declaration)
        .collect()
}

/// Every capability any feature requires is `Present` on all three platforms.
fn total_surface() -> PlatformCapabilitySurface {
    let mut surface = PlatformCapabilitySurface::new();
    for feature in FeatureId::ALL {
        for capability in feature.platform_binding().capabilities() {
            for platform in PlatformAxis::ALL {
                surface.declare(platform, capability.clone(), PlatformSource::Present);
            }
        }
    }
    surface
}

/// The achieved-shape ledger: anchors are attached exactly where a capability
/// source backs the cell.
fn evidenced_ledger(sources: &PlatformCapabilitySurface) -> FeaturePlatformLedger {
    FeaturePlatformLedger::from_declarations_with_evidence(
        &all_declarations(),
        sources,
        |feature, _, _| {
            if feature.platform_binding().capabilities().is_empty() {
                NO_EVIDENCE
            } else {
                "p4.manifest.anchor.fixture"
            }
        },
    )
}

fn count_rule(findings: &[PlatformGateFinding], rule: PlatformGateRule) -> usize {
    findings
        .iter()
        .filter(|finding| finding.rule() == rule)
        .count()
}

/// The gate is green on the achieved shape: a complete static surface with
/// evidence attached, the full expected identity set bound or accepted, and the
/// product baseline.
#[test]
fn the_product_policy_is_green_on_the_achieved_shape() {
    let sources = total_surface();
    let ledger = evidenced_ledger(&sources);
    assert_eq!(
        ledger.len(),
        4 * FeatureId::ALL.len() * PlatformAxis::ALL.len()
    );
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(findings.is_empty(), "{findings:?}");
}

/// The same gate is green on the surface that exists today: an empty layer-B
/// declaration means no cell may claim `Ready`, and the typed reasons, the
/// binding census, and the ceilings are already enforced.
#[test]
fn the_product_policy_is_green_on_an_empty_layer_b_surface() {
    let sources = PlatformCapabilitySurface::new();
    for frontend in FrontendShape::ALL {
        let ledger =
            FeaturePlatformLedger::from_declaration(&total_declaration(frontend), &sources);
        let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
        assert!(findings.is_empty(), "{}: {findings:?}", frontend.name());
        assert!(
            !ledger
                .iter()
                .any(|cell| matches!(cell.status, FeaturePlatformStatus::Ready)),
            "{}: an undeclared source must never fold to Ready",
            frontend.name()
        );
    }
}

/// G1: a duplicated frontend shape duplicates its cells; the gate reports the
/// duplicate instead of merging it away.
#[test]
fn g1_reports_a_duplicated_frontend_shape() {
    let sources = PlatformCapabilitySurface::new();
    let declarations = [
        total_declaration(FrontendShape::Gpui),
        total_declaration(FrontendShape::Gpui),
    ];
    let ledger = FeaturePlatformLedger::from_declarations(&declarations, &sources);
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(
        count_rule(&findings, PlatformGateRule::G1) > 0,
        "{findings:?}"
    );
}

/// G1: a ledger with no frontend is not a grid.
#[test]
fn g1_reports_an_empty_grid() {
    let sources = PlatformCapabilitySurface::new();
    let ledger = FeaturePlatformLedger::default();
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(
        findings.contains(&PlatformGateFinding::GridEmpty),
        "{findings:?}"
    );
}

/// G2: a `Ready` cell without a usable anchor is red - both for a missing
/// anchor and for a placeholder pretending to be one.
#[test]
fn g2_reports_ready_without_usable_evidence() {
    let sources = total_surface();
    for bogus in [NO_EVIDENCE, "0", "pending", "  "] {
        let declarations = all_declarations();
        let ledger = FeaturePlatformLedger::from_declarations_with_evidence(
            &declarations,
            &sources,
            |_, _, _| bogus,
        );
        let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
        assert!(
            count_rule(&findings, PlatformGateRule::G2) > 0,
            "anchor {bogus:?} must not pass as evidence: {findings:?}"
        );
    }
}

/// G3: every non-`Ready` shape carries its typed reason; a runtime status, a
/// mismatched retry, an empty note, and a causeless `Partial` are all red.
#[test]
fn g3_reports_untyped_reasons() {
    let sources = PlatformCapabilitySurface::new();

    // A runtime status smuggled into a static absence.
    let runtime_absence = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Unsupported(PlatformUnavailability {
            capability: Some(CapabilityId::ACCELERATOR_NPU),
            status: CapabilityStatus::Available,
            retry: RetryDisposition::Never,
        }),
    );
    let findings = feature_platform_cell_findings(&runtime_absence, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G3) > 0,
        "{findings:?}"
    );

    // A retry disposition that contradicts the absence projection.
    let mismatched_retry = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Unregistered(PlatformUnavailability {
            capability: None,
            status: CapabilityStatus::PermissionRequired,
            retry: RetryDisposition::Never,
        }),
    );
    let findings = feature_platform_cell_findings(&mismatched_retry, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G3) > 0,
        "{findings:?}"
    );

    // An `Unsupported` cell that does not name its capability.
    let anonymous_gap = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Unsupported(PlatformUnavailability {
            capability: None,
            status: CapabilityStatus::Unsupported,
            retry: RetryDisposition::Never,
        }),
    );
    let findings = feature_platform_cell_findings(&anonymous_gap, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G3) > 0,
        "{findings:?}"
    );

    // A `Gated` cell with a status that gates nothing.
    let fake_gate = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Gated(CapabilityStatus::Available),
    );
    let findings = feature_platform_cell_findings(&fake_gate, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G3) > 0,
        "{findings:?}"
    );

    // An empty note on a frontend gap, and a `Partial` with no cause at all.
    let empty_note = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Missing("  "),
    );
    let findings = feature_platform_cell_findings(&empty_note, &sources);
    assert!(
        findings.contains(&PlatformGateFinding::EmptyReason {
            feature: FeatureId::NpuTelemetry,
            frontend: FrontendShape::Gpui,
            platform: PlatformAxis::Linux,
        }),
        "{findings:?}"
    );

    let causeless = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Partial(PartialCause {
            platform: None,
            frontend: None,
        }),
    );
    let findings = feature_platform_cell_findings(&causeless, &sources);
    assert!(
        findings.contains(&PlatformGateFinding::PartialWithoutCause {
            feature: FeatureId::NpuTelemetry,
            frontend: FrontendShape::Gpui,
            platform: PlatformAxis::Linux,
        }),
        "{findings:?}"
    );
}

/// G4: no false success. A `Ready` cell whose required source is missing, a
/// non-delivery cell carrying a behaviour anchor, and a delivery state on a
/// non-delivery binding are all red.
#[test]
fn g4_reports_false_success() {
    let sources = PlatformCapabilitySurface::new();

    let mut ready_without_source = cell(FeatureId::NpuTelemetry, FeaturePlatformStatus::Ready);
    ready_without_source.evidence = "p4.manifest.anchor.fixture";
    let findings = feature_platform_cell_findings(&ready_without_source, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G4) > 0,
        "{findings:?}"
    );

    let mut missing_with_evidence = cell(
        FeatureId::NpuTelemetry,
        FeaturePlatformStatus::Missing("no entry point"),
    );
    missing_with_evidence.evidence = "p4.manifest.anchor.fixture";
    let findings = feature_platform_cell_findings(&missing_with_evidence, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G4) > 0,
        "{findings:?}"
    );

    let mut shared_derivation_with_evidence = cell(
        FeatureId::MultiFormatExport,
        FeaturePlatformStatus::NotApplicable("shared history projection"),
    );
    shared_derivation_with_evidence.evidence = "p4.manifest.anchor.fixture";
    let findings = feature_platform_cell_findings(&shared_derivation_with_evidence, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G4) > 0,
        "{findings:?}"
    );

    let mut ready_on_shared_derivation =
        cell(FeatureId::MultiFormatExport, FeaturePlatformStatus::Ready);
    ready_on_shared_derivation.evidence = "p4.manifest.anchor.fixture";
    let findings = feature_platform_cell_findings(&ready_on_shared_derivation, &sources);
    assert!(
        count_rule(&findings, PlatformGateRule::G4) > 0,
        "{findings:?}"
    );
}

/// G5 forward: a binding that references an identity outside the product
/// surface is red.
#[test]
fn g5_reports_a_binding_outside_the_product_surface() {
    let expected = [CapabilityId::TELEMETRY_HOST];
    let baseline = PlatformGateBaseline::ZERO;
    let policy = PlatformGatePolicy {
        expected_surface: &expected,
        feature_independent: &[],
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let sources = PlatformCapabilitySurface::new();
    let ledger =
        FeaturePlatformLedger::from_declaration(&total_declaration(FrontendShape::Gpui), &sources);
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(
        findings.iter().any(|finding| matches!(
            finding,
            PlatformGateFinding::CapabilityOutsideProductSurface { .. }
        )),
        "{findings:?}"
    );
}

/// G5 reverse: a new unbound expected identity is red unless it is accepted
/// baseline debt or explicitly declared feature-independent; a stale debt entry
/// and an empty independence reason are red.
#[test]
fn g5_reports_unbound_orphans_and_stale_baselines() {
    let sources = PlatformCapabilitySurface::new();
    let ledger =
        FeaturePlatformLedger::from_declaration(&total_declaration(FrontendShape::Gpui), &sources);

    let mut expected: Vec<CapabilityId> = CapabilityId::EXPECTED_SURFACE.to_vec();
    expected.push(CapabilityId::owned("vendor.orphan"));
    let baseline = PlatformGateBaseline::ZERO;
    let policy = PlatformGatePolicy {
        expected_surface: &expected,
        feature_independent: &[],
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(
        findings.iter().any(|finding| matches!(
            finding,
            PlatformGateFinding::UnboundExpectedCapability { capability }
                if capability.as_str() == "vendor.orphan"
        )),
        "{findings:?}"
    );

    let bound_debt = [CapabilityId::PROCESS_LIST];
    let baseline = PlatformGateBaseline {
        missing_by_frontend: &[],
        unregistered_per_frontend: 0,
        unbound_expected_capabilities: &bound_debt,
    };
    let policy = PlatformGatePolicy {
        expected_surface: &CapabilityId::EXPECTED_SURFACE,
        feature_independent: &[],
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(
        findings.contains(&PlatformGateFinding::StaleCapabilityBaseline {
            capability: CapabilityId::PROCESS_LIST,
            detail: "the capability is now bound; shrink the baseline in the same change",
        }),
        "{findings:?}"
    );

    let independent = [(CapabilityId::TELEMETRY_HOST, "  ")];
    let baseline = PlatformGateBaseline::ZERO;
    let policy = PlatformGatePolicy {
        expected_surface: &CapabilityId::EXPECTED_SURFACE,
        feature_independent: &independent,
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(
        findings.contains(&PlatformGateFinding::EmptyIndependentReason {
            capability: CapabilityId::TELEMETRY_HOST,
        }),
        "{findings:?}"
    );
}

/// G6: the `Missing` ceiling is a real ceiling - one cell over the baseline is
/// red, a count at the ceiling is green.
#[test]
fn g6_reports_a_missing_count_above_its_ceiling() {
    let sources = PlatformCapabilitySurface::new();
    let mut omitted = total_declaration(FrontendShape::Gpui);
    omitted
        .entries
        .retain(|entry| entry.feature != FeatureId::NpuTelemetry);
    let ledger = FeaturePlatformLedger::from_declaration(&omitted, &sources);

    let missing = [(FrontendShape::Gpui, 3usize)];
    let baseline = PlatformGateBaseline {
        missing_by_frontend: &missing,
        unregistered_per_frontend: PLATFORM_GATE_BASELINE.unregistered_per_frontend,
        unbound_expected_capabilities: PLATFORM_GATE_BASELINE.unbound_expected_capabilities,
    };
    let policy = PlatformGatePolicy {
        expected_surface: &CapabilityId::EXPECTED_SURFACE,
        feature_independent: FEATURE_INDEPENDENT_CAPABILITIES,
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(findings.is_empty(), "{findings:?}");

    let missing = [(FrontendShape::Gpui, 2usize)];
    let baseline = PlatformGateBaseline {
        missing_by_frontend: &missing,
        unregistered_per_frontend: PLATFORM_GATE_BASELINE.unregistered_per_frontend,
        unbound_expected_capabilities: PLATFORM_GATE_BASELINE.unbound_expected_capabilities,
    };
    let policy = PlatformGatePolicy {
        expected_surface: &CapabilityId::EXPECTED_SURFACE,
        feature_independent: FEATURE_INDEPENDENT_CAPABILITIES,
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(
        findings.contains(&PlatformGateFinding::MissingCeilingExceeded {
            frontend: FrontendShape::Gpui,
            counted: 3,
            ceiling: 2,
        }),
        "{findings:?}"
    );
}

/// G6: the `Unregistered` ceiling counts per frontend; raising it above the
/// measured debt is red.
#[test]
fn g6_reports_an_unregistered_count_above_its_ceiling() {
    let sources = PlatformCapabilitySurface::new();
    let ledger =
        FeaturePlatformLedger::from_declaration(&total_declaration(FrontendShape::Tui), &sources);

    let baseline = PlatformGateBaseline {
        missing_by_frontend: PLATFORM_GATE_BASELINE.missing_by_frontend,
        unregistered_per_frontend: 2,
        unbound_expected_capabilities: PLATFORM_GATE_BASELINE.unbound_expected_capabilities,
    };
    let policy = PlatformGatePolicy {
        expected_surface: &CapabilityId::EXPECTED_SURFACE,
        feature_independent: FEATURE_INDEPENDENT_CAPABILITIES,
        baseline: &baseline,
        evidence: PLATFORM_GATE_POLICY.evidence,
    };
    let findings = feature_platform_gate_findings(&ledger, &sources, &policy);
    assert!(
        findings.contains(&PlatformGateFinding::UnregisteredCeilingExceeded {
            frontend: FrontendShape::Tui,
            counted: 3,
            ceiling: 2,
        }),
        "{findings:?}"
    );
}

/// The accepted-debt census is exact: the baseline list is the complete set of
/// product-expected identities no feature binds. Binding one, or declaring it
/// feature-independent, must shrink this list in the same change.
#[test]
fn the_unbound_capability_baseline_is_an_exact_live_census() {
    let bound: BTreeSet<CapabilityId> = FeatureId::ALL
        .iter()
        .flat_map(|feature| feature.platform_binding().capabilities().iter().cloned())
        .collect();
    let mut computed: Vec<String> = CapabilityId::EXPECTED_SURFACE
        .iter()
        .filter(|capability| !bound.contains(capability))
        .filter(|capability| {
            !FEATURE_INDEPENDENT_CAPABILITIES
                .iter()
                .any(|(candidate, _)| candidate == *capability)
        })
        .map(|capability| capability.as_str().to_owned())
        .collect();
    computed.sort_unstable();
    let mut expected: Vec<String> = PLATFORM_GATE_BASELINE
        .unbound_expected_capabilities
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect();
    expected.sort_unstable();
    assert_eq!(
        computed, expected,
        "the accepted-debt census drifted; update the baseline in the same change"
    );
}

/// The `Unregistered` ceiling is the exact count of `NotInVocabulary` features
/// on three platforms.
#[test]
fn the_unregistered_baseline_is_an_exact_live_census() {
    let counted = FeatureId::ALL
        .iter()
        .filter(|feature| {
            matches!(
                feature.platform_binding(),
                PlatformBinding::NotInVocabulary(_)
            )
        })
        .count()
        * PlatformAxis::ALL.len();
    assert_eq!(counted, PLATFORM_GATE_BASELINE.unregistered_per_frontend);
}

/// The per-frontend `Missing` ceilings are the exact counts the four frontend
/// declarations produce against today's empty layer-B surface. Each frontend's
/// own gate test re-asserts its own number; this audit keeps them enumerable in
/// one place.
#[test]
fn the_missing_ceilings_are_a_live_census_of_the_declared_gaps() {
    for frontend in FrontendShape::ALL {
        let ceiling = PLATFORM_GATE_BASELINE.missing_ceiling(frontend);
        assert!(
            ceiling > 0,
            "{}: every shape declares gaps today",
            frontend.name()
        );
        assert_eq!(
            ceiling % PlatformAxis::ALL.len(),
            0,
            "{}: a frontend gap repeats once per platform",
            frontend.name()
        );
    }
}

/// The rule ids are the stable machine names gate output cites.
#[test]
fn rule_ids_are_stable_machine_names() {
    assert_eq!(PlatformGateRule::G1.id(), "G1");
    assert_eq!(PlatformGateRule::G2.id(), "G2");
    assert_eq!(PlatformGateRule::G3.id(), "G3");
    assert_eq!(PlatformGateRule::G4.id(), "G4");
    assert_eq!(PlatformGateRule::G5.id(), "G5");
    assert_eq!(PlatformGateRule::G6.id(), "G6");
}

/// The committed feature-evidence table is structurally clean: an empty
/// findings list is the only passing state, every row is unique, and the
/// anchored batch is non-empty.
#[test]
fn the_committed_feature_evidence_table_is_structurally_clean() {
    let table = PLATFORM_GATE_POLICY.feature_evidence();
    assert!(table.findings().is_empty(), "{:?}", table.findings());
    assert!(
        table.anchored_count() > 0,
        "the first anchored batch is non-empty"
    );
    let mut cells: Vec<(String, String)> = table
        .rows()
        .iter()
        .map(|row| (row.feature.id().to_owned(), row.frontend.name().to_owned()))
        .collect();
    let count = cells.len();
    cells.sort_unstable();
    cells.dedup();
    assert_eq!(cells.len(), count, "one row per (feature, frontend) cell");
}

/// The anchored batches are a conscious census: the first batch anchors four
/// features on every frontend; the second batch (W14-A) anchors eight further
/// features on the frontends whose delivered surface a real test proves
/// (`gpu.engine-utilization`, `memory.breakdown-rss-pss`,
/// `storage.device-topology`, `services.inventory`,
/// `security.posix-capabilities`, `security.sandbox-detection`,
/// `hardware.heterogeneous-cores`, `hardware.core-frequency`); the third batch
/// (W15-A) anchors `storage.smart-health` (iced/tui),
/// `storage.iops-queue-latency` (tui), `power.battery-inventory` (iced/tui),
/// and `services.log-stream` (iced), and records the surveyed near-misses of
/// the same survey as explicit `pending` gaps; the fourth batch (W16-A)
/// converts the two GPUI `pending` cells that rested on test-tree-only
/// fixtures into anchors on the shape's real delivery surfaces
/// (`power.battery-inventory` on the Performance battery panel,
/// `power.thermal-zones` on the System Health sensor center) after deleting
/// the unshipped fixture. The fifth batch (W16-B/W16-C) closes the TUI
/// observation gaps (page faults, anonymous huge pages, the painted log
/// stream with its level filter, and the disk queue/service row that makes
/// the TUI `storage.iops-queue-latency` anchor prove all three clauses) and
/// the Iced/Bevy observations (swap rates, IOPS/queue/latency, the
/// page-fault and huge-page counters, per-adapter GPU enumeration with
/// Bevy's per-engine rows, Bevy partition rows and six-family SMART
/// evidence, the Bevy painted log stream, and the Bevy namespace audit),
/// adding eighteen anchored rows and converting thirteen surveyed `pending`
/// gaps. The same batch gained a nineteenth anchor in W17-A, when the Iced
/// health line's real thermal-zone surface landed and the Iced
/// `power.thermal-zones` cell moved from `pending` to the traversal test (one
/// row per shared temperature reading, each named by its own source label, a
/// failed read kept as the shared dash). The sixth batch (W18-A) closes the
/// six GPUI `pending` gaps: the painted services inventory rows with their
/// typed active state under the typed status filter, the disk page's painted
/// partition rows with their observed usage fraction, the SMART health
/// evidence families, the swap-in/out throughput rates, the IOPS/queue/latency
/// fold, and the painted service-log stream under its level filter. A
/// follow-up in the same batch converted the TUI `storage.swap-throughput`
/// cell after the TUI line landed its painted swap-rate test. The closing
/// follow-up (W20 tail) converted the table's last surveyed gap - the TUI
/// `power.thermal-zones` cell - after the TUI Fan page landed its SYSTEM
/// thermal-zone traversal (one row per shared temperature reading, each named
/// by its own source label, an unread channel kept as the shared dash). The
/// per-frontend anchored counts and the surveyed pending gaps are pinned, so
/// growing either batch - or recording a new near-miss as `pending` again -
/// must move this pin in the same change. The W23-A batch narrowed
/// `memory.breakdown-rss-pss` to its real facets (resident/proportional/
/// private/derived-shared), re-pointed the GPUI cell to the in-crate details
/// dialog fold that proves the complete narrowed definition, and anchored the
/// previously unanchored Bevy cell on the same details-overview fold. The
/// W25-A batch promoted `power.thermal-throttle-events` from its surveyed
/// `Unsupported` gap to the four shapes' real CPU-counter surfaces (GPUI
/// reference; Iced/TUI/Bevy ported) with one fresh anchor per shape.
/// The W28-A batch registered `memory.process-swap-charge` as the single
/// authority for the per-process swap charge the four shapes already render,
/// anchored the Iced and TUI Apps-table swap cells on their real tests, and
/// recorded the GPUI and Bevy cells as explicit `pending` gaps (no
/// discoverable test proves the observed charge there yet).
#[test]
fn the_first_anchor_batch_is_a_conscious_census() {
    let table = PLATFORM_GATE_POLICY.feature_evidence();
    for feature in [
        FeatureId::HandleEnumeration,
        FeatureId::ThreadTopologyEnumeration,
        FeatureId::ServiceLifecycleControl,
        FeatureId::SystemdDependencyDag,
    ] {
        for frontend in FrontendShape::ALL {
            assert!(
                table.anchor(feature, frontend).is_some(),
                "{}: {} must carry a committed anchor",
                frontend.name(),
                feature.id()
            );
        }
    }
    assert_eq!(
        table.anchored_count(),
        75,
        "the anchored batch census moved"
    );
    for (frontend, anchored) in [
        (FrontendShape::Gpui, 17usize),
        (FrontendShape::Iced, 19),
        (FrontendShape::Tui, 21),
        (FrontendShape::Bevy, 18),
    ] {
        assert_eq!(
            table
                .rows_for(frontend)
                .filter(|row| row.is_anchored())
                .count(),
            anchored,
            "{}: the anchored census moved",
            frontend.name()
        );
    }
    // The W28-A per-process swap addition (`memory.process-swap-charge`) is
    // anchored on every shape that renders the Apps-table swap cell: Iced and
    // TUI on their existing swap-cell tests, and (W28-P) GPUI and Bevy on new
    // tests that feed an observed `swap_bytes` and assert the rendered charge
    // plus the honest dash for an unobserved one. A new surveyed near-miss
    // must again be committed as an explicit `pending` row, and the pending
    // shape stays covered by the parser counterexamples and the synthetic
    // evidence-closure witness below.
    assert_eq!(
        table.pending_count(),
        0,
        "the surveyed pending-gap census moved"
    );
}

/// The evidence closure attaches the committed anchor only where the static
/// source commitment is complete: an undeclared/absent lane keeps
/// [`NO_EVIDENCE`], a pending row never becomes an anchor, and a complete lane
/// returns the committed test id. The pending counterexample is built on a
/// synthetic table because the committed table is fully anchored today; the
/// rule under test belongs to the fold, not to the current census.
#[test]
fn the_evidence_closure_requires_a_complete_source_commitment() {
    let table = PLATFORM_GATE_POLICY.feature_evidence();
    let feature = FeatureId::HandleEnumeration;
    let frontend = FrontendShape::Gpui;
    let committed = table.anchor(feature, frontend).expect("committed anchor");

    let undeclared = PlatformCapabilitySurface::new();
    let closure = PLATFORM_GATE_POLICY.evidence_closure(&undeclared);
    for platform in PlatformAxis::ALL {
        assert_eq!(
            closure(feature, frontend, platform),
            NO_EVIDENCE,
            "an undeclared source must not attach an anchor"
        );
    }

    let mut complete = PlatformCapabilitySurface::new();
    for capability in feature.platform_binding().capabilities() {
        complete.declare(
            PlatformAxis::Linux,
            capability.clone(),
            PlatformSource::Present,
        );
    }
    let closure = PLATFORM_GATE_POLICY.evidence_closure(&complete);
    assert_eq!(closure(feature, frontend, PlatformAxis::Linux), committed);
    for platform in [PlatformAxis::Windows, PlatformAxis::Macos] {
        assert_eq!(
            closure(feature, frontend, platform),
            NO_EVIDENCE,
            "every other platform axis is still undeclared"
        );
    }

    // A `pending` row is an explicit gap, never an anchor - even when the
    // feature's whole `Requires` set is `Present`. The committed table has no
    // pending row left (the survey is closed), so the witness is a synthetic
    // table with the same shape: without it, deleting the rule would leave the
    // committed table green.
    let pending_table = FeatureEvidenceTable::parse(concat!(
        "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
        "storage.smart-health\tbevy\t-\tpending\tno Bevy SMART evidence test\t-\n",
    ));
    assert!(
        pending_table.findings().is_empty(),
        "{:?}",
        pending_table.findings()
    );
    let pending_feature = FeatureId::DiskSmartHealth;
    let pending_frontend = FrontendShape::Bevy;
    assert!(
        pending_table
            .pending_note(pending_feature, pending_frontend)
            .is_some(),
        "the synthetic witness is a surveyed gap"
    );
    let mut pending_sources = PlatformCapabilitySurface::new();
    for capability in pending_feature.platform_binding().capabilities() {
        pending_sources.declare(
            PlatformAxis::Linux,
            capability.clone(),
            PlatformSource::Present,
        );
    }
    assert_eq!(
        pending_table.ledger_anchor(
            &pending_sources,
            pending_feature,
            pending_frontend,
            PlatformAxis::Linux,
        ),
        NO_EVIDENCE,
        "a pending row is an explicit gap, never an anchor"
    );
}

/// G2 is not relaxed by the committed table: on a complete source surface the
/// anchored rows materialize once per platform, and every un-anchored
/// `Ready` cell is still refused.
#[test]
fn g2_refuses_the_un_anchored_remainder_even_with_the_committed_table() {
    let sources = total_surface();
    let ledger = FeaturePlatformLedger::from_declarations_with_evidence(
        &all_declarations(),
        &sources,
        PLATFORM_GATE_POLICY.evidence_closure(&sources),
    );
    let findings = feature_platform_gate_findings(&ledger, &sources, &PLATFORM_GATE_POLICY);
    assert!(
        findings
            .iter()
            .all(|finding| finding.rule() == PlatformGateRule::G2),
        "{findings:?}"
    );
    let admitted = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Ready) && cell.has_evidence())
        .count();
    assert_eq!(
        admitted,
        PLATFORM_GATE_POLICY.feature_evidence().anchored_count() * PlatformAxis::ALL.len(),
        "every committed anchor materializes on every complete platform axis"
    );
    let refused = ledger
        .iter()
        .filter(|cell| matches!(cell.status, FeaturePlatformStatus::Ready) && !cell.has_evidence())
        .count();
    assert!(
        refused > 0,
        "the un-anchored remainder must still be refused"
    );
    assert_eq!(count_rule(&findings, PlatformGateRule::G2), refused);
}

/// The parser is not a rubber stamp: an unknown feature or frontend id, a
/// placeholder anchor, a pending row with a test id or without a note, a
/// duplicated cell, and an anchor on a non-delivery binding are all findings.
#[test]
fn the_evidence_table_parser_rejects_malformed_rows() {
    let header = "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n";
    let legal = FeatureEvidenceTable::parse(concat!(
        "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
        "handles.enumeration\tgpui\tgpui_app::tests::anchor\tanchored\t-\tgpui_app::tests::co_anchor\n",
        "storage.smart-health\tbevy\t-\tpending\tno Bevy SMART evidence test\t-\n",
    ));
    assert!(legal.findings().is_empty(), "{:?}", legal.findings());
    assert_eq!(legal.anchored_count(), 1);
    assert_eq!(legal.pending_count(), 1);
    assert_eq!(
        legal.co_anchor(FeatureId::HandleEnumeration, FrontendShape::Gpui),
        Some("gpui_app::tests::co_anchor"),
        "the sixth column parses into the row's co-anchor"
    );
    assert_eq!(
        legal.co_anchor(FeatureId::DiskSmartHealth, FrontendShape::Bevy),
        None,
        "a row without a co-anchor exposes none"
    );

    type EvidenceFindingCase = (&'static str, fn(&FeatureEvidenceFinding) -> bool);
    let cases: [EvidenceFindingCase; 7] = [
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "handles.nope\tgpui\tgpui_app::tests::anchor\tanchored\t-\t-\n",
            ),
            |finding| matches!(finding, FeatureEvidenceFinding::UnknownFeature { .. }),
        ),
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "handles.enumeration\twatch\tgpui_app::tests::anchor\tanchored\t-\t-\n",
            ),
            |finding| matches!(finding, FeatureEvidenceFinding::UnknownFrontend { .. }),
        ),
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "handles.enumeration\tgpui\tpending\tanchored\t-\t-\n",
            ),
            |finding| matches!(finding, FeatureEvidenceFinding::UnusableAnchor { .. }),
        ),
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "handles.enumeration\tgpui\tgpui_app::tests::anchor\tpending\t-\t-\n",
            ),
            |finding| matches!(finding, FeatureEvidenceFinding::PendingWithAnchor { .. }),
        ),
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "handles.enumeration\tgpui\tgpui_app::tests::anchor\tanchored\t-\t-\n",
                "handles.enumeration\tgpui\tgpui_app::tests::other\tanchored\t-\t-\n",
            ),
            |finding| matches!(finding, FeatureEvidenceFinding::DuplicateCell { .. }),
        ),
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "memory.leak-trend\tgpui\tgpui_app::tests::anchor\tanchored\t-\t-\n",
            ),
            |finding| {
                matches!(
                    finding,
                    FeatureEvidenceFinding::AnchorOnNonDeliveryBinding { .. }
                )
            },
        ),
        (
            concat!(
                "feature_id\tfrontend\ttest_id\tstatus\tnote\tco_test_id\n",
                "handles.enumeration\tgpui\tgpui_app::tests::anchor\tanchored\t-\n",
            ),
            |finding| matches!(finding, FeatureEvidenceFinding::MalformedRow { .. }),
        ),
    ];
    for (table, predicate) in cases {
        let parsed = FeatureEvidenceTable::parse(table);
        assert!(
            parsed.findings().iter().any(predicate),
            "expected a finding for:\n{table}got {:?}",
            parsed.findings()
        );
    }
    assert!(
        FeatureEvidenceTable::parse(header)
            .findings()
            .contains(&FeatureEvidenceFinding::EmptyTable)
    );
}

/// Test-only helper: a cell with the fixture shape and an empty anchor.
fn cell(feature: FeatureId, status: FeaturePlatformStatus) -> FeaturePlatformCell {
    FeaturePlatformCell {
        feature,
        frontend: FrontendShape::Gpui,
        platform: PlatformAxis::Linux,
        status,
        evidence: NO_EVIDENCE,
    }
}
