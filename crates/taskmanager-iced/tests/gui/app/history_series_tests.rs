use super::*;

#[test]
fn series_snapshot_reuses_until_the_data_epoch_changes() {
    let history = LiveGraphHistory::default();
    let mut cache = HistorySeriesCache::default();
    let first = cache.get(&history, 4, MetricSeries::CpuUsagePercent);
    let second = cache.get(&history, 4, MetricSeries::CpuUsagePercent);
    assert!(Rc::ptr_eq(&first, &second));

    let next_epoch = cache.get(&history, 5, MetricSeries::CpuUsagePercent);
    assert!(!Rc::ptr_eq(&first, &next_epoch));

    let other_series = cache.get(&history, 5, MetricSeries::MemoryUsagePercent);
    assert!(!Rc::ptr_eq(&next_epoch, &other_series));
}

#[test]
fn core_and_device_snapshots_reuse_until_the_revision_changes() {
    let history = LiveGraphHistory::default();
    let mut cache = HistorySeriesCache::default();

    let cores = cache.core(&history, 7);
    let same_cores = cache.core(&history, 7);
    assert!(Rc::ptr_eq(&cores, &same_cores));

    let disk = cache.cached_device(
        &history,
        7,
        DeviceSeriesKey::new(DeviceSeriesKind::DiskBytesPerSec, "nvme0n1", "model", ""),
        |_| vec![1.0, 2.0],
    );
    let same_disk = cache.cached_device(
        &history,
        7,
        DeviceSeriesKey::new(DeviceSeriesKind::DiskBytesPerSec, "nvme0n1", "model", ""),
        |_| vec![9.0],
    );
    assert!(Rc::ptr_eq(&disk, &same_disk));
    assert_eq!(&*same_disk, &[1.0, 2.0]);

    let next_disk = cache.cached_device(
        &history,
        8,
        DeviceSeriesKey::new(DeviceSeriesKind::DiskBytesPerSec, "nvme0n1", "model", ""),
        |_| vec![3.0],
    );
    assert!(!Rc::ptr_eq(&disk, &next_disk));
}
