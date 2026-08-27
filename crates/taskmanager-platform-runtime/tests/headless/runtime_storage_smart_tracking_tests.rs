use crate::CapabilityHealth;
use taskmanager_application::{
    DeviceGeneration, DeviceId, FailureKind, SmartControlRequest, SmartObservationBatch,
    SmartObservationIssue, SmartSelfTestIntent, StorageDeviceKey, StorageDeviceTarget,
};
use taskmanager_core::{SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport};

use super::{batch_health, control};
use crate::storage::smart_state::SharedSmartRuntimeState;

fn issue(device: &str, failure: FailureKind) -> SmartObservationIssue {
    SmartObservationIssue {
        target: StorageDeviceTarget {
            device_id: DeviceId::new(format!("disk:{device}")),
            device_generation: DeviceGeneration::INITIAL,
            locator: StorageDeviceKey::new(device),
        },
        failure,
    }
}

#[test]
fn multi_target_health_is_independent_of_device_or_issue_order() {
    let permission = issue("z", FailureKind::PermissionDenied);
    let timeout = issue("a", FailureKind::TimedOut);
    let left = SmartObservationBatch {
        issues: vec![timeout.clone(), permission.clone()],
        ..SmartObservationBatch::default()
    };
    let right = SmartObservationBatch {
        issues: vec![permission, timeout],
        ..SmartObservationBatch::default()
    };

    assert_eq!(
        batch_health(&left, true),
        CapabilityHealth::Degraded(FailureKind::PermissionDenied)
    );
    assert_eq!(batch_health(&left, true), batch_health(&right, true));
    assert_eq!(
        batch_health(&left, false),
        CapabilityHealth::Unavailable(taskmanager_application::ProviderFailure::PermissionDenied)
    );
}

fn intent(device: &str) -> SmartSelfTestIntent {
    SmartSelfTestIntent {
        device_id: DeviceId::new(format!("disk:{device}")),
        device_generation: DeviceGeneration::INITIAL,
        device_key: StorageDeviceKey::new(device),
        display_name: device.to_string(),
        kind: SmartSelfTestKind::Short,
    }
}

#[test]
fn capacity_rejection_happens_before_the_drive_side_effect() {
    let state = SharedSmartRuntimeState::with_job_limit(100, 1);
    let running = SmartSelfTestReport {
        phase: SmartSelfTestPhase::Running,
        kind: Some(SmartSelfTestKind::Short),
        ..SmartSelfTestReport::default()
    };
    state
        .install_started(intent("a").into_observation(running.clone()), 0)
        .expect("fill bounded state");
    let provider_calls = Arc::new(AtomicU8::new(0));
    let calls = Arc::clone(&provider_calls);
    let mut provider = move |_intent: &SmartSelfTestIntent, _now_ms: u64| {
        calls.fetch_add(1, Ordering::Relaxed);
        Ok(running.clone())
    };

    assert!(matches!(
        control(
            SmartControlRequest::StartSelfTest(intent("b")),
            &state,
            &mut provider,
            1,
        ),
        Err(taskmanager_application::ProviderFailure::Rejected)
    ));
    assert_eq!(
        provider_calls.load(Ordering::Relaxed),
        0,
        "a rejected job must not touch the drive"
    );
    assert_eq!(state.snapshot().expect("bounded state").jobs.len(), 1);
}
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
