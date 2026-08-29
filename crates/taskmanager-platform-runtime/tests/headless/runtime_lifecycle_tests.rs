use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

use super::*;

fn source(outcome: SourceOutcome) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed("fixture.discovery"),
        outcome,
        item_count: 0,
    }
}

#[test]
fn source_truth_controls_absence_authority() {
    assert_eq!(
        discovery_refresh_outcome(&[source(SourceOutcome::Empty)]),
        DeviceRefreshOutcome::Complete
    );
    assert_eq!(
        discovery_refresh_outcome(&[source(SourceOutcome::Partial(
            FailureKind::PermissionDenied,
        ))]),
        DeviceRefreshOutcome::Unavailable(DeviceStatus::PermissionDenied)
    );
    assert_eq!(
        discovery_refresh_outcome(&[]),
        DeviceRefreshOutcome::Unavailable(DeviceStatus::Stale)
    );
}
