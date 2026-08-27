use std::time::Duration;

use super::*;

#[test]
fn counter_reset_and_generation_prune_never_bridge_rate_baselines() {
    let device_id = "gpu:pci:0000:00:02.0";
    let started_at = Instant::now();
    let mut tracker = IntelRc6Tracker::default();
    assert_eq!(
        tracker
            .observe(device_id, GpuFieldRead::available(100), started_at)
            .failure,
        Some(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        tracker
            .observe(
                device_id,
                GpuFieldRead::available(200),
                started_at + Duration::from_secs(1),
            )
            .utilization_pct,
        Some(90.0)
    );
    assert_eq!(
        tracker
            .observe(
                device_id,
                GpuFieldRead::available(50),
                started_at + Duration::from_secs(2),
            )
            .failure,
        Some(FailureKind::IdentityChanged)
    );

    tracker.prune(&[DeviceId::new(device_id)]);
    let readded = tracker.observe(
        device_id,
        GpuFieldRead::available(100),
        started_at + Duration::from_secs(3),
    );
    assert_eq!(readded.utilization_pct, None);
    assert_eq!(readded.failure, Some(FailureKind::TemporarilyUnavailable));
}
