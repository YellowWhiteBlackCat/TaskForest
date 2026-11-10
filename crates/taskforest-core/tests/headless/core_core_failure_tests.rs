use super::*;

const ALL_KINDS: [FailureKind; 9] = [
    FailureKind::RequiresEscalation,
    FailureKind::PermissionDenied,
    FailureKind::MissingDependency,
    FailureKind::TimedOut,
    FailureKind::ProviderFault,
    FailureKind::TemporarilyUnavailable,
    FailureKind::Unsupported,
    FailureKind::IdentityChanged,
    FailureKind::Rejected,
];

/// The rank is total over the vocabulary: every variant is ranked, ranks are
/// unique, and `stronger` therefore always returns one of its two inputs.
#[test]
fn severity_rank_is_total_and_strictly_ordered_over_every_variant() {
    let mut ranks: Vec<u8> = Vec::with_capacity(ALL_KINDS.len());
    for kind in ALL_KINDS {
        let rank = kind.severity_rank();
        assert!(rank > 0, "{kind:?} must carry a non-zero rank");
        ranks.push(rank);
    }
    ranks.sort_unstable();
    let mut unique = ranks.clone();
    unique.dedup();
    assert_eq!(
        ranks, unique,
        "two failure kinds must not share a rank: {ranks:?}"
    );
    // The documented precedence, highest first. A reordering is a product
    // decision about which cause a partial source reports, never a refactor.
    assert_eq!(
        ALL_KINDS
            .iter()
            .map(|kind| kind.severity_rank())
            .collect::<Vec<_>>(),
        [9, 8, 7, 6, 5, 4, 3, 2, 1],
        "the severity table must match the documented order"
    );
}

/// Pairwise: the fold is exactly the ranking, so no call site can obtain a
/// different representative failure for the same pair.
#[test]
fn stronger_returns_the_higher_ranked_kind_for_every_pair() {
    for left in ALL_KINDS {
        for right in ALL_KINDS {
            let wanted = if right.severity_rank() > left.severity_rank() {
                right
            } else {
                left
            };
            assert_eq!(
                left.stronger(right),
                wanted,
                "{left:?} vs {right:?} must fold to {wanted:?}"
            );
            assert_eq!(
                right.stronger(left),
                wanted,
                "the fold must be symmetric in its result for {left:?} and {right:?}"
            );
        }
    }
}

/// Ties keep the first observed cause: an equal rank never replaces a cause
/// that was already reported.
#[test]
fn equal_ranks_keep_the_first_observed_cause() {
    for kind in ALL_KINDS {
        assert_eq!(kind.stronger(kind), kind);
    }
    assert_eq!(
        FailureKind::Rejected.stronger(FailureKind::Unsupported),
        FailureKind::Unsupported
    );
    assert_eq!(
        FailureKind::Unsupported.stronger(FailureKind::RequiresEscalation),
        FailureKind::RequiresEscalation
    );
    // The two pairs the retired per-crate tables used to answer differently.
    assert_eq!(
        FailureKind::ProviderFault.stronger(FailureKind::TimedOut),
        FailureKind::TimedOut,
        "a provider that did not answer outranks one that answered badly"
    );
    assert_eq!(
        FailureKind::Rejected.stronger(FailureKind::Unsupported),
        FailureKind::Unsupported,
        "a permanent platform boundary outranks one declined request"
    );
}

/// `None` means nothing failed yet, so the candidate is taken verbatim.
#[test]
fn strongest_folds_an_optional_current_cause() {
    assert_eq!(
        FailureKind::strongest(None, FailureKind::Rejected),
        FailureKind::Rejected
    );
    assert_eq!(
        FailureKind::strongest(Some(FailureKind::Rejected), FailureKind::PermissionDenied),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        FailureKind::strongest(Some(FailureKind::RequiresEscalation), FailureKind::Rejected),
        FailureKind::RequiresEscalation
    );
}
