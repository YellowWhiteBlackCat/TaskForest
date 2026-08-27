use super::*;

/// The complete escalation-column set, matching Boundary 3 of
/// `docs/PERMISSION_MODEL.md`. Delegates to the single source of truth
/// (`EscalationFeature::ALL`) so this list and the enum cannot drift.
const ALL_FEATURES: [EscalationFeature; 7] = EscalationFeature::ALL;

#[test]
fn unprivileged_gate_requires_escalation_for_every_feature() {
    let gate = UnprivilegedGate;
    for feature in ALL_FEATURES {
        assert_eq!(
            gate.probe(feature),
            EscalationAvailability::RequiresEscalation(feature),
            "UnprivilegedGate must report RequiresEscalation for {feature:?} — \
                 the app starts unprivileged and nothing is escalated until the \
                 user actively uses a feature",
        );
    }
}

#[test]
fn unprivileged_gate_never_silently_grants_or_hard_denies() {
    // The honest default must not collapse to Available (fabricated access)
    // or to Denied (hides a real escalation opportunity behind a refusal).
    let gate = UnprivilegedGate;
    for feature in ALL_FEATURES {
        match gate.probe(feature) {
            EscalationAvailability::RequiresEscalation(probed) => {
                assert_eq!(
                    probed, feature,
                    "RequiresEscalation must echo the probed feature exactly",
                );
            }
            other => panic!(
                "unprivileged default must report RequiresEscalation for \
                     {feature:?}, not {other:?}",
            ),
        }
    }
}

#[test]
fn escalation_seam_types_are_debug_clone_eq() {
    // The seam types participate in the typed-degradation contract and must
    // be Debug + Clone + Eq so callers can compare, route, and log them.
    fn assert_traits<T: core::fmt::Debug + Clone + PartialEq + Eq>() {}
    assert_traits::<EscalationFeature>();
    assert_traits::<EscalationDenialReason>();
    assert_traits::<EscalationAvailability>();
    assert_traits::<UnprivilegedGate>();

    // Eq is sound: structurally equal availabilities compare equal.
    assert_eq!(
        EscalationAvailability::Available,
        EscalationAvailability::Available,
    );
    assert_eq!(
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu),
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu),
    );
    assert_ne!(
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu),
        EscalationAvailability::RequiresEscalation(EscalationFeature::AtaSmart),
    );
    assert_ne!(
        EscalationAvailability::RequiresEscalation(EscalationFeature::IntelPmu),
        EscalationAvailability::Available,
    );
    assert_eq!(
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::PermissionDenied,
        },
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::PermissionDenied,
        },
    );
    assert_ne!(
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::Unsupported,
        },
        EscalationAvailability::Denied {
            reason: EscalationDenialReason::PermissionDenied,
        },
    );

    // UnprivilegedGate is a trivial value type that compares equal to itself.
    assert_eq!(UnprivilegedGate, UnprivilegedGate);
}

#[test]
fn every_escalation_feature_variant_is_distinct() {
    // The UI offers exactly one prompt per feature, so variants must be
    // distinct and exhaustive against the documented escalation column.
    for (i, lhs) in ALL_FEATURES.iter().enumerate() {
        for rhs in ALL_FEATURES.iter().skip(i + 1) {
            assert_ne!(
                lhs, rhs,
                "EscalationFeature variants collided — each must map to exactly one prompt",
            );
        }
    }
}
