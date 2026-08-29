use taskmanager_core::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::ProviderFailure;

use super::{inventory_failure, snapshot_failure};

fn status(provider: &'static str, outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome,
        item_count: 1,
    }
}

#[test]
fn inventory_available_or_empty_is_not_a_failure() {
    assert_eq!(
        inventory_failure(&[status(
            "linux.process.procfs.inventory",
            SourceOutcome::Available
        )]),
        None
    );
    assert_eq!(
        inventory_failure(&[status(
            "linux.process.procfs.inventory",
            SourceOutcome::Empty
        )]),
        None
    );
}

#[test]
fn inventory_failure_kind_is_preserved() {
    assert_eq!(
        inventory_failure(&[status(
            "linux.process.procfs.inventory",
            SourceOutcome::Unavailable(FailureKind::TimedOut)
        )]),
        Some(ProviderFailure::TimedOut)
    );
    assert_eq!(
        inventory_failure(&[status(
            "linux.process.procfs.inventory",
            SourceOutcome::Partial(FailureKind::PermissionDenied)
        )]),
        Some(ProviderFailure::PermissionDenied)
    );
}

#[test]
fn missing_inventory_source_is_a_provider_fault() {
    assert_eq!(
        inventory_failure(&[status(
            "linux.process.procfs.other",
            SourceOutcome::Available
        )]),
        Some(ProviderFailure::ProviderFault),
        "an inventory source that never reported must not pass the guard"
    );
    assert_eq!(inventory_failure(&[]), Some(ProviderFailure::ProviderFault));
}

#[test]
fn snapshot_failure_picks_the_highest_priority_failure() {
    let sources = [
        status(
            "linux.process.procfs.cpu",
            SourceOutcome::Partial(FailureKind::TemporarilyUnavailable),
        ),
        status(
            "linux.process.procfs.state",
            SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        ),
    ];
    assert_eq!(
        snapshot_failure(&sources),
        ProviderFailure::PermissionDenied,
        "permission denied (8) outranks temporarily unavailable (lower)"
    );
}

#[test]
fn escalation_denial_outranks_plain_denial() {
    let sources = [
        status(
            "linux.process.procfs.x",
            SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        ),
        status(
            "linux.process.procfs.pmu",
            SourceOutcome::Unavailable(FailureKind::RequiresEscalation),
        ),
    ];
    assert_eq!(
        snapshot_failure(&sources),
        ProviderFailure::RequiresEscalation,
        "escalatable denial outranks plain denial and remains actionable"
    );
}

#[test]
fn snapshot_failure_with_no_failed_sources_falls_back_to_provider_fault() {
    assert_eq!(
        snapshot_failure(&[status("a", SourceOutcome::Available)]),
        ProviderFailure::ProviderFault
    );
    assert_eq!(snapshot_failure(&[]), ProviderFailure::ProviderFault);
}
