use super::*;
use taskforest_core::core::{
    AlertInstanceId, AlertRuleId, AlertTargetIdentity, DeviceGeneration, DeviceId, DiskMetrics,
};

fn alert(metric: AlertMetric, target: &str) -> Alert {
    let target_identity = if matches!(
        metric,
        AlertMetric::DiskTemperatureC
            | AlertMetric::SmartPercentUsed
            | AlertMetric::SmartCriticalWarning
    ) {
        AlertTargetIdentity::Storage {
            device_id: DeviceId::new(target),
            generation: DeviceGeneration::INITIAL,
        }
    } else {
        AlertTargetIdentity::System
    };
    let rule_id = AlertRuleId::new("test-rule");
    Alert {
        instance_id: AlertInstanceId::new(rule_id.clone(), target_identity),
        rule_id,
        target: target.to_owned(),
        metric,
        severity: AlertSeverity::Warning,
        value: 72.0,
        threshold: 70.0,
        active_since_ms: 1,
    }
}

fn snapshot_with_disk() -> SystemSnapshot {
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.device_id = "disk:stable:nvme0n1".to_owned();
    SystemSnapshot {
        disks: vec![disk],
        ..SystemSnapshot::default()
    }
}

#[test]
fn alert_disk_target_resolves_by_identity_or_name() {
    let snapshot = snapshot_with_disk();
    assert_eq!(
        alert_target_device(
            &alert(AlertMetric::DiskTemperatureC, "disk:stable:nvme0n1"),
            &snapshot
        ),
        Some(SelectedDevice::Disk(0))
    );
    assert_eq!(
        alert_target_device(&alert(AlertMetric::SmartPercentUsed, "nvme0n1"), &snapshot),
        Some(SelectedDevice::Disk(0))
    );
}

#[test]
fn missing_alert_disk_target_does_not_redirect_to_the_first_disk() {
    let snapshot = snapshot_with_disk();
    assert_eq!(
        alert_target_device(
            &alert(AlertMetric::SmartCriticalWarning, "disk:gone"),
            &snapshot
        ),
        None,
        "a vanished alert target must not be replaced by index zero"
    );
}
