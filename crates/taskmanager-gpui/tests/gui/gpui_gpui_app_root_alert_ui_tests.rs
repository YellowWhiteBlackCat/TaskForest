use super::*;
use taskmanager_core::core::DiskMetrics;

fn alert(metric: AlertMetric, target: &str) -> Alert {
    Alert {
        instance_id: format!("test:{target}"),
        rule_id: "test-rule".to_owned(),
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
