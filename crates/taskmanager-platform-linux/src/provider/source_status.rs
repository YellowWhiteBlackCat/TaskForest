//! Shared conversion from device health to typed provider-source outcomes.

use taskmanager_core::DeviceStatus;
use taskmanager_platform_contract::{FailureKind, ProviderId, SourceOutcome, SourceStatus};

pub(super) fn source_status_from_device_state(
    provider: &'static str,
    status: DeviceStatus,
    successful_items: usize,
    total_items: usize,
) -> SourceStatus {
    let failure = match status {
        DeviceStatus::PermissionDenied => FailureKind::PermissionDenied,
        DeviceStatus::MissingTool => FailureKind::MissingDependency,
        DeviceStatus::Unsupported => FailureKind::Unsupported,
        DeviceStatus::Stale | DeviceStatus::Healthy => FailureKind::TemporarilyUnavailable,
    };
    let outcome = match status {
        DeviceStatus::Healthy if total_items == 0 => SourceOutcome::Empty,
        DeviceStatus::Healthy if successful_items == total_items => SourceOutcome::Available,
        DeviceStatus::Unsupported if total_items == 0 => SourceOutcome::Empty,
        DeviceStatus::Unsupported if successful_items == 0 => {
            SourceOutcome::Unavailable(FailureKind::Unsupported)
        }
        _ if successful_items > 0 => SourceOutcome::Partial(failure),
        _ => SourceOutcome::Unavailable(failure),
    };
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome,
        item_count: successful_items,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_provider_source_status_tests.rs"]
mod tests;
