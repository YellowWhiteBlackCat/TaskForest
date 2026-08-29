//! Capability registry fold tests (CORE-08): the anti-silence matrix over
//! the component/surface capability set, mirroring the keybindings fold.

use super::*;

fn entry(capability: ComponentCapability, support: CapabilitySupport) -> CapabilityEntry {
    CapabilityEntry {
        capability,
        support,
    }
}

fn full_declaration(
    frontend: FrontendShape,
    support: impl Fn(ComponentCapability) -> CapabilitySupport,
) -> FrontendCapabilityDeclaration {
    FrontendCapabilityDeclaration {
        frontend,
        entries: ComponentCapability::ALL
            .iter()
            .map(|capability| entry(*capability, support(*capability)))
            .collect(),
    }
}

/// The registry is total and duplicate-free: every variant appears exactly
/// once in `ALL`, ids are unique, and every entry names a reference path.
/// The count is pinned so adding a capability is a conscious registry
/// change, never an accident.
#[test]
fn all_covers_every_capability_exactly_once() {
    assert_eq!(ComponentCapability::ALL.len(), 19);
    let mut ids: Vec<_> = ComponentCapability::ALL.iter().map(|c| c.id()).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "capability ids must be unique");
    assert!(
        ComponentCapability::ALL
            .iter()
            .all(|capability| !capability.reference_path().is_empty()),
        "every capability names its taskmanager-ui reference path (GPUI-05)"
    );
}

/// A total explicit declaration folds with no drift and no findings — for
/// the reference shape (all `Reference`) and for a porting shape (mixed
/// explicit decisions with reasons).
#[test]
fn total_declarations_produce_no_drift_or_findings() {
    let reference = full_declaration(FrontendShape::Gpui, |_| CapabilitySupport::Reference);
    assert!(capability_drift(&capability_report(&reference)).is_empty());
    assert!(capability_findings(&reference).is_empty());

    let porting = full_declaration(FrontendShape::Iced, |capability| match capability {
        ComponentCapability::Toast => CapabilitySupport::Divergent {
            reason: "feedback rides the footer activity line",
        },
        ComponentCapability::Scrollbar => CapabilitySupport::Native {
            via: "iced scrollable",
        },
        ComponentCapability::Tooltip => CapabilitySupport::Unsupported {
            reason: "no hover surface",
        },
        _ => CapabilitySupport::Ported,
    });
    assert!(capability_drift(&capability_report(&porting)).is_empty());
    assert!(capability_findings(&porting).is_empty());
}

/// A silent omission is drift: dropping one entry yields exactly one
/// `Missing` cell and one finding for that capability.
#[test]
fn silent_omission_is_drift() {
    let mut declaration = full_declaration(FrontendShape::Tui, |_| CapabilitySupport::Ported);
    declaration.entries.truncate(declaration.entries.len() - 1);
    let report = capability_report(&declaration);
    assert_eq!(
        capability_drift(&report),
        vec![(
            *ComponentCapability::ALL.last().expect("non-empty ALL"),
            CapabilityStatus::Missing
        )]
    );
    assert_eq!(
        capability_findings(&declaration),
        vec![CapabilityFinding {
            frontend: FrontendShape::Tui,
            capability: *ComponentCapability::ALL.last().expect("non-empty ALL"),
            kind: CapabilityFindingKind::Missing,
        }]
    );
}

/// A duplicated declaration marks the capability `Duplicated` — two entries
/// for one capability cannot both be the shape's decision.
#[test]
fn duplicated_declaration_is_drift() {
    let mut declaration = full_declaration(FrontendShape::Iced, |_| CapabilitySupport::Ported);
    let first = declaration.entries[0];
    declaration.entries.push(first);
    let drift = capability_drift(&capability_report(&declaration));
    assert_eq!(drift.len(), 1);
    assert_eq!(drift[0].0, first.capability);
    assert_eq!(drift[0].1, CapabilityStatus::Duplicated);
    assert_eq!(
        capability_findings(&declaration),
        vec![CapabilityFinding {
            frontend: FrontendShape::Iced,
            capability: first.capability,
            kind: CapabilityFindingKind::Duplicated,
        }]
    );
}

/// Only the GPUI shape may own `Reference` semantics (GPUI-05); a porting
/// shape claiming the reference role is a finding even though the cell is
/// explicit.
#[test]
fn reference_role_belongs_to_the_reference_shape() {
    assert!(FrontendShape::Gpui.is_capability_reference_shape());
    assert!(!FrontendShape::Iced.is_capability_reference_shape());
    assert!(!FrontendShape::Tui.is_capability_reference_shape());

    let stolen = full_declaration(FrontendShape::Tui, |_| CapabilitySupport::Reference);
    assert!(capability_drift(&capability_report(&stolen)).is_empty());
    let findings = capability_findings(&stolen);
    assert_eq!(findings.len(), ComponentCapability::ALL.len());
    assert!(
        findings
            .iter()
            .all(|finding| finding.kind == CapabilityFindingKind::ReferenceOutsideReferenceShape)
    );
}

/// The reference shape cannot defer itself: `Ported`/`Divergent`/
/// `Unsupported` in the GPUI declaration means the vocabulary outran the
/// reference layer — the capability must not exist until taskmanager-ui
/// grows it (GPUI-05).
#[test]
fn the_reference_shape_cannot_port_diverge_or_defer() {
    for deferred in [
        CapabilitySupport::Ported,
        CapabilitySupport::Divergent { reason: "why" },
        CapabilitySupport::Unsupported { reason: "why" },
    ] {
        let mut declaration =
            full_declaration(FrontendShape::Gpui, |_| CapabilitySupport::Reference);
        declaration.entries[0].support = deferred;
        assert_eq!(
            capability_findings(&declaration),
            vec![CapabilityFinding {
                frontend: FrontendShape::Gpui,
                capability: declaration.entries[0].capability,
                kind: CapabilityFindingKind::ReferenceShapeCannotDefer,
            }],
            "support {deferred:?} is not a reference-shape decision"
        );
    }
}

/// Deliberate differences must say why: empty `via`/`reason` text is a
/// finding — the registry replaces prose parity claims with explained
/// decisions.
#[test]
fn deliberate_differences_carry_non_empty_explanations() {
    for empty in [
        CapabilitySupport::Native { via: "" },
        CapabilitySupport::Divergent { reason: "" },
        CapabilitySupport::Unsupported { reason: "" },
    ] {
        let mut declaration = full_declaration(FrontendShape::Iced, |_| CapabilitySupport::Ported);
        declaration.entries[0].support = empty;
        assert_eq!(
            capability_findings(&declaration),
            vec![CapabilityFinding {
                frontend: FrontendShape::Iced,
                capability: declaration.entries[0].capability,
                kind: CapabilityFindingKind::EmptyExplanation,
            }],
            "support {empty:?} must carry its explanation"
        );
    }
}
