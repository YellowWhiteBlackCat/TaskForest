//! Feature coverage registry fold tests (P1): the anti-silence matrix over the
//! extensible `FeatureId` set, mirroring the capability registry fold.
//!
//! The gate set ([`FeatureId::ALL`]) is the FULL roadmap parity matrix: these
//! tests pin its size, prove every feature carries a precise semantic
//! specification, and keep every frontend fold honest.

use super::*;

fn entry(feature: FeatureId, support: CapabilitySupport) -> FeatureCoverageEntry {
    FeatureCoverageEntry { feature, support }
}

fn full_declaration(
    frontend: FrontendShape,
    support: impl Fn(FeatureId) -> CapabilitySupport,
) -> FeatureCoverageDeclaration {
    FeatureCoverageDeclaration {
        frontend,
        entries: FeatureId::ALL
            .iter()
            .map(|feature| entry(*feature, support(*feature)))
            .collect(),
    }
}

/// The gate set is total and duplicate-free: `ALL` is non-empty, every feature
/// id is unique, and the pinned count makes adding or dropping a gated feature
/// a conscious registry change.
#[test]
fn all_covers_features_exactly_once() {
    assert_eq!(FeatureId::ALL.len(), 45);
    let mut ids: Vec<_> = FeatureId::ALL.iter().map(|feature| feature.id()).collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "feature ids must be unique");
    assert!(
        FeatureId::ALL
            .iter()
            .all(|feature| !feature.id().is_empty()),
        "every feature names a stable machine id"
    );
}

/// Every feature carries one explicit, non-empty, toolkit-neutral semantic
/// specification; the spec names its own feature so a copy/paste slip cannot
/// pass silently.
#[test]
fn every_feature_has_explicit_toolkit_neutral_semantic_specification() {
    for feature in FeatureId::ALL {
        let spec = feature.semantic_spec();
        assert_eq!(spec.feature, *feature);
        assert!(
            !spec.delivery_definition.is_empty(),
            "feature {} must define what counts as delivered",
            feature.id()
        );
    }
}

/// Every feature files under a known blueprint area, every area has at least
/// one registered representative, and the areas themselves are total and
/// unique. (The 225-item completeness TODO grows the per-area feature count;
/// it never empties an area.)
#[test]
fn feature_areas_are_well_formed_and_total() {
    assert_eq!(FeatureArea::ALL.len(), 15);
    let mut area_ids: Vec<_> = FeatureArea::ALL.iter().map(|area| area.id()).collect();
    let area_count = area_ids.len();
    area_ids.sort_unstable();
    area_ids.dedup();
    assert_eq!(
        area_ids.len(),
        area_count,
        "feature area ids must be unique"
    );

    for feature in FeatureId::ALL {
        assert!(
            FeatureArea::ALL.contains(&feature.area()),
            "feature {} files under a known area",
            feature.id()
        );
    }
    for area in FeatureArea::ALL {
        assert!(
            FeatureId::ALL.iter().any(|feature| feature.area() == *area),
            "area {} needs at least one registered representative",
            area.id()
        );
    }
}

/// `area()` classifies representatives correctly across the blueprint, not
/// just by prefix accident: first, last, and a mid-set area are all pinned,
/// including the former roadmap items now part of the gate set.
#[test]
fn feature_area_classification_is_correct() {
    assert_eq!(
        FeatureId::ProcessSchedulerPolicy.area(),
        FeatureArea::ProcessLifecycle
    );
    assert_eq!(
        FeatureId::LinuxNamespaceAudit.area(),
        FeatureArea::SecurityIsolation
    );
    assert_eq!(
        FeatureId::SystemdDependencyDag.area(),
        FeatureArea::ServicesInit
    );
    assert_eq!(
        FeatureId::MultiResolutionRingBuffer.area(),
        FeatureArea::HistoryTimeTravel
    );
    assert_eq!(
        FeatureId::PipeDeadlockDiagnosis.area(),
        FeatureArea::IpcDbus
    );
    assert_eq!(
        FeatureId::PmuCounterAbstraction.area(),
        FeatureArea::DynamicTracing
    );
    assert_eq!(
        FeatureId::CpuCacheTopology.area(),
        FeatureArea::HardwareTopologyNuma
    );
    assert_eq!(
        FeatureId::TimeTravelScrubber.area(),
        FeatureArea::HistoryTimeTravel
    );
}

/// All 10 Wave 3 / Wave 4 core deliverables are registered and tagged in the
/// single gate set, so the wave work cannot silently fall outside the coverage
/// inventory.
#[test]
fn all_wave_deliverables_are_registered_and_tagged() {
    let gated: Vec<_> = FeatureId::ALL
        .iter()
        .filter(|feature| feature.origin() == FeatureOrigin::Wave3Wave4)
        .copied()
        .collect();
    assert_eq!(gated.len(), 10);
    for expected in [
        FeatureId::LinuxNamespaceAudit,
        FeatureId::PosixCapabilitiesAudit,
        FeatureId::SystemdDependencyDag,
        FeatureId::ServiceLogStream,
        FeatureId::PsiMultiWindowTelemetry,
        FeatureId::MemoryThrashingHealthScore,
        FeatureId::DbusServiceTopology,
        FeatureId::PipeDeadlockDiagnosis,
        FeatureId::PmuCounterAbstraction,
        FeatureId::MultiResolutionRingBuffer,
    ] {
        assert!(
            gated.contains(&expected),
            "wave deliverable {} must be registered",
            expected.id()
        );
    }
}

/// A total explicit declaration folds with no drift and no findings - for the
/// reference shape (all `Reference`) and for a porting shape (mixed explicit
/// decisions with non-empty reasons).
#[test]
fn total_declarations_produce_no_drift_or_findings() {
    let reference = full_declaration(FrontendShape::Gpui, |_| CapabilitySupport::Reference);
    assert!(feature_coverage_drift(&feature_coverage_report(&reference)).is_empty());
    assert!(feature_coverage_findings(&reference).is_empty());

    let porting = full_declaration(FrontendShape::Iced, |feature| match feature {
        FeatureId::ServiceLogStream => CapabilitySupport::Native {
            via: "iced async subscription",
        },
        FeatureId::NpuTelemetry => CapabilitySupport::Unsupported {
            reason: "no NPU surface on this shape yet",
        },
        FeatureId::SocketInventory => CapabilitySupport::Divergent {
            reason: "socket inventory renders through the process-detail rail",
        },
        _ => CapabilitySupport::Ported,
    });
    assert!(feature_coverage_drift(&feature_coverage_report(&porting)).is_empty());
    assert!(feature_coverage_findings(&porting).is_empty());
}

/// A silent omission is drift: dropping one entry yields exactly one `Missing`
/// cell and one finding for that feature.
#[test]
fn silent_omission_is_drift() {
    let mut declaration = full_declaration(FrontendShape::Tui, |_| CapabilitySupport::Ported);
    declaration.entries.truncate(declaration.entries.len() - 1);
    let last = *FeatureId::ALL.last().expect("non-empty ALL");
    assert_eq!(
        feature_coverage_drift(&feature_coverage_report(&declaration)),
        vec![(last, FeatureCoverageStatus::Missing)]
    );
    assert_eq!(
        feature_coverage_findings(&declaration),
        vec![FeatureCoverageFinding {
            frontend: FrontendShape::Tui,
            feature: last,
            kind: FeatureCoverageFindingKind::Missing,
        }]
    );
}

/// A duplicated declaration marks the feature `Duplicated` - two entries for
/// one feature cannot both be the shape's decision.
#[test]
fn duplicated_declaration_is_drift() {
    let mut declaration = full_declaration(FrontendShape::Iced, |_| CapabilitySupport::Ported);
    let first = declaration.entries[0];
    declaration.entries.push(first);
    let drift = feature_coverage_drift(&feature_coverage_report(&declaration));
    assert_eq!(drift.len(), 1);
    assert_eq!(drift[0].0, first.feature);
    assert_eq!(drift[0].1, FeatureCoverageStatus::Duplicated);
    assert_eq!(
        feature_coverage_findings(&declaration),
        vec![FeatureCoverageFinding {
            frontend: FrontendShape::Iced,
            feature: first.feature,
            kind: FeatureCoverageFindingKind::Duplicated,
        }]
    );
}

/// Deliberate differences must say why: empty `via`/`reason` text is a
/// finding, replacing prose parity claims with explained decisions.
#[test]
fn deliberate_differences_carry_non_empty_explanations() {
    for empty in [
        CapabilitySupport::Native { via: "" },
        CapabilitySupport::Divergent { reason: "" },
        CapabilitySupport::Unsupported { reason: "" },
    ] {
        let mut declaration = full_declaration(FrontendShape::Bevy, |_| CapabilitySupport::Ported);
        declaration.entries[0].support = empty;
        assert_eq!(
            feature_coverage_findings(&declaration),
            vec![FeatureCoverageFinding {
                frontend: FrontendShape::Bevy,
                feature: declaration.entries[0].feature,
                kind: FeatureCoverageFindingKind::EmptyExplanation,
            }],
            "support {empty:?} must carry its explanation"
        );
    }
}

/// Only the GPUI reference shape may own `Reference` semantics, and that shape
/// cannot port, diverge from, or delegate a feature (reusing the GPUI-05 rule).
#[test]
fn reference_role_belongs_to_the_reference_shape() {
    assert!(FrontendShape::Gpui.is_capability_reference_shape());
    assert!(!FrontendShape::Tui.is_capability_reference_shape());

    let stolen = full_declaration(FrontendShape::Tui, |_| CapabilitySupport::Reference);
    let findings = feature_coverage_findings(&stolen);
    assert_eq!(findings.len(), FeatureId::ALL.len());
    assert!(
        findings
            .iter()
            .all(|finding| finding.kind
                == FeatureCoverageFindingKind::ReferenceOutsideReferenceShape)
    );

    for forbidden in [
        CapabilitySupport::Ported,
        CapabilitySupport::Divergent { reason: "why" },
        CapabilitySupport::Native { via: "why" },
    ] {
        let mut declaration =
            full_declaration(FrontendShape::Gpui, |_| CapabilitySupport::Reference);
        declaration.entries[0].support = forbidden;
        assert_eq!(
            feature_coverage_findings(&declaration),
            vec![FeatureCoverageFinding {
                frontend: FrontendShape::Gpui,
                feature: declaration.entries[0].feature,
                kind: FeatureCoverageFindingKind::ReferenceShapeCannotPort,
            }],
            "support {forbidden:?} is not a reference-shape decision"
        );
    }
}

/// The reference shape MAY declare `Unsupported { reason }` - the coverage
/// matrix keeps an undelivered reference item visible instead of forcing a
/// fabricated `Reference` claim - but the reason must be non-empty.
#[test]
fn reference_shape_may_declare_an_unsupported_gap() {
    let declaration = full_declaration(FrontendShape::Gpui, |_| CapabilitySupport::Unsupported {
        reason: "reference surface not yet delivered",
    });
    assert!(feature_coverage_findings(&declaration).is_empty());

    let mut empty = declaration.clone();
    empty.entries[0].support = CapabilitySupport::Unsupported { reason: "" };
    assert_eq!(
        feature_coverage_findings(&empty),
        vec![FeatureCoverageFinding {
            frontend: FrontendShape::Gpui,
            feature: empty.entries[0].feature,
            kind: FeatureCoverageFindingKind::EmptyExplanation,
        }]
    );
}

/// A feature declared outside the known set is reported, never silently
/// dropped: the restricted-known-set seam keeps `Unknown` real.
#[test]
fn declared_feature_outside_known_set_is_unknown() {
    let declaration = FeatureCoverageDeclaration {
        frontend: FrontendShape::Tui,
        entries: vec![entry(
            FeatureId::HandleEnumeration,
            CapabilitySupport::Ported,
        )],
    };
    let known = [FeatureId::ProcessSchedulerPolicy];
    assert_eq!(
        feature_coverage_report_over(&declaration, &known),
        vec![
            (
                FeatureId::ProcessSchedulerPolicy,
                FeatureCoverageStatus::Missing
            ),
            (FeatureId::HandleEnumeration, FeatureCoverageStatus::Unknown),
        ]
    );
}

/// The two registries are deliberately asymmetric. Capability coverage is the
/// DELIVERED-SURFACE vocabulary, so a reference-shape `Unsupported` is a
/// finding; feature coverage is the ROADMAP matrix, so a reference-shape
/// `Unsupported { reason }` is an honest, accepted gap. Pinning both sides here
/// stops a future "cleanup" from flattening one contract into the other.
#[test]
fn capability_and_feature_registries_are_deliberately_asymmetric() {
    use crate::{
        CapabilityEntry, CapabilityFindingKind, ComponentCapability, FrontendCapabilityDeclaration,
        capability_findings,
    };

    // Feature axis (roadmap matrix): a reasoned reference gap is accepted.
    let feature_gap = full_declaration(FrontendShape::Gpui, |_| CapabilitySupport::Unsupported {
        reason: "reference surface not yet delivered",
    });
    assert!(
        feature_coverage_findings(&feature_gap).is_empty(),
        "the roadmap coverage matrix must admit a reasoned reference gap"
    );

    // Capability axis (delivered-surface vocabulary): the same declaration is
    // rejected for every capability the reference shape does not own.
    let capability_gap = FrontendCapabilityDeclaration {
        frontend: FrontendShape::Gpui,
        entries: ComponentCapability::ALL
            .iter()
            .map(|capability| CapabilityEntry {
                capability: *capability,
                support: CapabilitySupport::Unsupported {
                    reason: "reference surface not yet delivered",
                },
            })
            .collect(),
    };
    let findings = capability_findings(&capability_gap);
    assert_eq!(findings.len(), ComponentCapability::ALL.len());
    assert!(
        findings
            .iter()
            .all(|finding| finding.kind == CapabilityFindingKind::ReferenceShapeCannotDefer),
        "a reference `Unsupported` is a delivered-surface violation: {findings:?}"
    );
}

/// The three delivery definitions most easily blurred are pinned at their
/// boundary: CPU cache stays capacity-only, thrashing health keeps the
/// full-stall deduction requirement, and NUMA keeps the per-node requirement.
/// Weakening one must fail here and be a conscious specification change, never
/// an accidental overclaim.
#[test]
fn authoritative_delivery_definitions_pin_their_boundaries() {
    let cache = FeatureId::CpuCacheTopology
        .semantic_spec()
        .delivery_definition;
    assert!(cache.contains("l1d_cache_kb"), "{cache}");
    assert!(cache.contains("l3_cache_kb"), "{cache}");
    assert!(cache.contains("OUTSIDE"), "{cache}");

    let thrashing = FeatureId::MemoryThrashingHealthScore
        .semantic_spec()
        .delivery_definition;
    assert!(thrashing.contains("SystemHealthScore"), "{thrashing}");
    assert!(thrashing.contains("MemoryFullStall"), "{thrashing}");

    let numa = FeatureId::NumaMemoryDistribution
        .semantic_spec()
        .delivery_definition;
    assert!(numa.contains("per-NUMA-node"), "{numa}");
    assert!(numa.contains("does NOT satisfy"), "{numa}");
}
