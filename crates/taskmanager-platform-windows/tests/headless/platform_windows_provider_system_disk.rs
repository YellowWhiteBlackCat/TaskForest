use super::*;

#[test]
fn disk_kind_labels_are_stable() {
    assert_eq!(disk_kind_label(sysinfo::DiskKind::HDD), "HDD");
    assert_eq!(disk_kind_label(sysinfo::DiskKind::SSD), "SSD");
}

#[test]
fn disk_io_requires_an_interval_but_preserves_measured_idle_zero() {
    assert_eq!(
        rate_observation(1_024, None, true, 100),
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        rate_observation(0, Some(2.0), true, 100),
        ScalarObservation::available(0, 100)
    );
    assert_eq!(
        rate_observation(2_048, Some(2.0), true, 100),
        ScalarObservation::available(1_024, 100)
    );
    assert_eq!(
        rate_observation(0, Some(2.0), false, 100),
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
    );
}
