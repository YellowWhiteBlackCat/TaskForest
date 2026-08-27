use super::{AlertEngine, AlertMetric, AlertRule, DiskMetrics};

fn disk(device_id: &str, name: &str) -> DiskMetrics {
    let mut disk = DiskMetrics::new(name);
    disk.device_id = device_id.to_owned();
    disk
}

fn rule(target: Option<&str>) -> AlertRule {
    let rule = AlertRule::new(
        "disk-full",
        AlertMetric::SmartPercentUsed,
        super::AlertSeverity::Warning,
        80.0,
        std::time::Duration::from_millis(0),
        0.0,
    );
    match target {
        Some(target) => rule.for_target(target),
        None => rule,
    }
}

#[test]
fn empty_device_id_falls_back_to_the_disk_name() {
    // device_id empty → name becomes the stable target (a `delete !` of
    // the guard would treat "" as a real id and produce a blank target).
    let signals = super::disk_signals(&rule(None), &[disk("", "nvme0n1")], |_| Some(90.0));
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].key_target, "nvme0n1");
}

#[test]
fn target_matches_any_one_of_stable_name_or_id() {
    // The target predicate is an OR over stable/name/device_id: matching
    // ONLY the stable key must still fire (a `||`→`&&` mutation of any
    // arm would require all three to agree).
    // Only the NAME matches (stable and device_id differ): the OR over
    // stable/name/device_id must still fire — this input is exactly what
    // a `||`→`&&` mutation of either arm breaks.
    let signals = super::disk_signals(
        &rule(Some("System Disk")),
        &[disk("sda", "System Disk")],
        |_| Some(88.0),
    );
    assert_eq!(signals.len(), 1, "target matching the name fires");
    assert_eq!(signals[0].key_target, "sda");
    assert_eq!(signals[0].display_target, "System Disk");

    // A non-matching target yields nothing.
    let none = super::disk_signals(&rule(Some("sdb")), &[disk("sda", "System Disk")], |_| {
        Some(88.0)
    });
    assert!(none.is_empty());
}

#[test]
fn clear_resets_pending_alert_state() {
    // Build a REAL pending duration first, then clear: after clear the
    // next evaluation starts a fresh window instead of firing on the old
    // one (a `clear → ()` mutation leaves the old window live).
    let mut engine = AlertEngine::new([AlertRule::new(
        "cpu-hot",
        AlertMetric::CpuUsagePercent,
        super::AlertSeverity::Warning,
        80.0,
        std::time::Duration::from_millis(5_000),
        0.0,
    )]);
    assert!(
        engine
            .evaluate(&super::super::SystemSnapshot {
                timestamp_ms: 1_000,
                cpu: crate::core::metrics::CpuMetrics::from_observations(
                    crate::core::metrics::CpuScalarObservations {
                        global_usage_pct: crate::core::metrics::ScalarObservation::available(
                            95.0, 1_000,
                        ),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            })
            .is_empty()
    );

    engine.clear();

    let after = engine.evaluate(&super::super::SystemSnapshot {
        timestamp_ms: 8_000,
        cpu: crate::core::metrics::CpuMetrics::from_observations(
            crate::core::metrics::CpuScalarObservations {
                global_usage_pct: crate::core::metrics::ScalarObservation::available(95.0, 8_000),
                ..Default::default()
            },
        ),
        ..Default::default()
    });
    assert!(
        after.is_empty(),
        "clear must reset the pending duration so the alert restarts"
    );
}
