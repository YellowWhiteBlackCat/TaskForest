//! Correlated storage-health, SMART, and directory-usage events appended to
//! the bounded `PlatformEventBatch`.

use super::super::{DirectoryUsageEvent, SmartEvent, StorageHealthEvent};
use super::{CorrelatedEvent, PlatformEventBatch, PlatformEventContext};

pub type CorrelatedStorageHealthEvent = CorrelatedEvent<StorageHealthEvent>;
pub type CorrelatedSmartEvent = CorrelatedEvent<SmartEvent>;
pub type CorrelatedDirectoryUsageEvent = CorrelatedEvent<DirectoryUsageEvent>;

pub(super) fn push_storage_health(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: StorageHealthEvent,
) {
    batch
        .storage_health_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_smart(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: SmartEvent,
) {
    batch
        .smart_events
        .push(CorrelatedEvent::new(context, event));
}

pub(super) fn push_directory_usage(
    batch: &mut PlatformEventBatch,
    context: PlatformEventContext,
    event: DirectoryUsageEvent,
) {
    batch
        .directory_usage_events
        .push(CorrelatedEvent::new(context, event));
}

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_event_batch_storage_tests.rs"]
mod tests;
