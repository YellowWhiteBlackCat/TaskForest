//! Shared folds for native appearance and health facts consumed by renderers.

use super::*;
use taskmanager_application::CapabilityId;

pub(super) fn apply_desktop_appearance(
    events: Vec<taskmanager_application::CorrelatedDesktopAppearanceEvent>,
    fold: &mut FoldState,
) {
    if events.is_empty() {
        return;
    }
    // Appearance still routes through its correlated side output because the
    // shell store does not own a persisted presentation preference. Marking
    // the domain here keeps the fold activity/revision contract honest.
    fold.output.changes.desktop_appearance = true;
    fold.mark_updated();
    fold.output.desktop_appearance_events = events;
}

pub(super) fn apply_storage_health(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedStorageHealthEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        let StorageHealthEvent::Snapshot(snapshot) = correlated.event;
        store.storage_health = Some(snapshot.value);
        store.storage_health_source = Some(snapshot.sources);
        fold.output.changes.storage_health = true;
        fold.mark_updated();
    }
}

pub(super) fn apply_smart(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedSmartEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        let SmartEvent::Batch(batch) = correlated.event;
        if correlated.capability == CapabilityId::SMART_CONTROL
            && let Some(target) = batch.subject.clone()
        {
            fold.output
                .smart_self_test_results
                .push(SmartSelfTestResult {
                    request_id: correlated.request_id,
                    target,
                });
        }
        if store.smart_observations.apply(&batch) != SmartProjectionApplyResult::Applied {
            continue;
        }
        store.smart_subject = batch.subject;
        fold.output.changes.smart = true;
        fold.mark_updated();
    }
}
