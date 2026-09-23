use super::{disk_stats, temperature_trend_stat_row, temperature_trend_value};
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::{
    DiskMetrics, DiskScalarObservations, ScalarObservation, SmartAvailability,
};
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_test_support::DiskMetricsFixtureBuilder;
use taskmanager_test_support::pin_english;

fn row_value(rows: &[StatRow], label: &str) -> Option<String> {
    rows.iter()
        .find(|row| row.label() == label)
        .and_then(|row| row.value().map(str::to_owned))
}

fn keyed_value(rows: &[StatRow], key: &'static str) -> Option<String> {
    row_value(rows, t(key))
}

/// An empty or all-gap window renders no trend row — absence stays
/// absence instead of becoming a fabricated "0 °C" summary.
#[test]
fn empty_or_non_finite_temperature_windows_render_no_trend() {
    assert_eq!(temperature_trend_value(&[]), None);
    assert_eq!(temperature_trend_value(&[f32::NAN, f32::NAN]), None);
}

/// A populated window summarizes latest/average/peak with the shared
/// locale labels, skipping any non-finite samples (the window is
/// NaN-free by construction; this guards the boundary).
#[test]
fn temperature_trend_summarizes_latest_average_and_peak() {
    let trend =
        temperature_trend_value(&[30.0, f32::NAN, 32.0, 40.0]).expect("finite samples exist");
    assert_eq!(
        trend,
        format!(
            "{} 40 °C · {} 34 °C · {} 40 °C",
            t("common.latest"),
            t("common.avg"),
            t("common.peak"),
        )
    );
}

/// SMART temperature trend produces a typed StatRow::Trend with distinct
/// latest, average, and peak components plus the full raw copy string.
#[test]
fn temperature_trend_stat_row_preserves_multiline_parts_and_raw_value() {
    let row =
        temperature_trend_stat_row(&[35.0, 37.0, 42.0]).expect("finite temperature samples exist");
    assert_eq!(row.label(), t("proc.trend"));
    let (latest, avg, peak) = row.trend_parts().expect("trend parts must be present");
    assert!(latest.contains("42"));
    assert!(avg.contains("38"));
    assert!(peak.contains("42"));
    let value = row.value().expect("full string must exist");
    assert!(value.contains("42 °C"));
    assert!(value.contains("38 °C"));
}

/// The removable-media row carries the locale label (never hardcoded
/// English "Removable"), and the power-on row formats through the
/// localized `{hours} h ({days} d)` catalog entry.
#[test]
fn removable_and_power_on_rows_use_locale_catalog_entries() {
    let disk = DiskMetricsFixtureBuilder::new()
        .media_removable(Some(true))
        .smart_power_on_hours(Some(72))
        .build();
    let rows = disk_stats(&disk, UnitPreferences::default(), &[]);
    let find = |key: &'static str| {
        rows.iter()
            .find(|row| row.label() == t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(
        find("disk.removable").value(),
        Some(t("common.yes")),
        "removable row must use the locale label, not hardcoded English"
    );
    assert_eq!(
        find("disk.power_on").value().map(str::to_owned),
        Some(
            t("disk.power_on_format")
                .replace("{hours}", "72")
                .replace("{days}", "3")
        )
    );
}

/// Rate rows keep `None` for first-sample gaps — the panel renders the
/// shared dash; capacity rows stay `None` until the provider reports.
#[test]
fn first_sample_rate_rows_are_none_not_fabricated_zeros() {
    let rows = disk_stats(&DiskMetrics::default(), UnitPreferences::default(), &[]);
    let find = |key: &'static str| {
        rows.iter()
            .find(|row| row.label() == t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(find("disk.read").value(), None);
    assert_eq!(find("disk.write").value(), None);
    assert_eq!(find("disk.iops").value(), None);
    assert_eq!(find("disk.active_time").value(), None);
}

/// The `storage.iops-queue-latency` definition clause by clause: an observed
/// IOPS, response latency, and average queue depth each render their own
/// production row with the real projected value (plus the separate
/// busy-time service-time estimate); a first-sample gap keeps the row's value
/// `None` — the panel's shared dash — never a fabricated `0`.
#[test]
fn disk_stats_render_the_observed_iops_latency_and_queue_depth() {
    pin_english();
    let disk = DiskMetricsFixtureBuilder::new()
        .scalar_observations(DiskScalarObservations {
            iops: ScalarObservation::available(137, 10),
            response_time_ms: ScalarObservation::available(1.54, 10),
            average_queue_depth: ScalarObservation::available(2.25, 10),
            service_time_ms: ScalarObservation::available(0.42, 10),
            ..Default::default()
        })
        .build();
    let rows = disk_stats(&disk, UnitPreferences::default(), &[]);
    assert_eq!(keyed_value(&rows, "disk.iops").as_deref(), Some("137"));
    assert_eq!(
        keyed_value(&rows, "disk.response").as_deref(),
        Some("1.54 ms")
    );
    assert_eq!(
        keyed_value(&rows, "disk.queue_depth").as_deref(),
        Some("2.25")
    );
    assert_eq!(
        keyed_value(&rows, "disk.service_time").as_deref(),
        Some("0.42 ms")
    );

    // The unobserved twin keeps its row (an applicable fact) with the dash,
    // so the panel can never fill the slot from a fabricated zero.
    let cold = disk_stats(&DiskMetrics::default(), UnitPreferences::default(), &[]);
    for key in [
        "disk.iops",
        "disk.response",
        "disk.queue_depth",
        "disk.service_time",
    ] {
        assert!(
            cold.iter().any(|row| row.label() == t(key)),
            "{key}: an applicable unobserved fact keeps its dash row"
        );
        assert_eq!(keyed_value(&cold, key), None, "{key}: never a fabricated 0");
    }
}

/// The `storage.smart-health` definition clause by clause, on the row set the
/// Performance disk panel paints (`disk_stats` feeds `stats_panel`): reported
/// availability, temperature (shared + per-sensor), percentage used, available
/// spare, power-on hours, and unsafe shutdowns each carry the real projected
/// value. The honesty half: a disk whose SMART telemetry is not available
/// grows none of those rows and reports the typed availability instead, so
/// none of the six clauses can surface as a fabricated `0%` / `0 h` / `0`.
#[test]
fn disk_stats_render_the_smart_health_evidence_families() {
    pin_english();
    let mut disk = DiskMetricsFixtureBuilder::new()
        .device_id("disk:wwid:smart".into())
        .name("nvme0n1".into())
        .smart_availability(SmartAvailability::Available)
        .smart_temperature_c(Some(41.0))
        .smart_temp_critical_c(Some(85.0))
        .smart_percent_used(Some(2.0))
        .smart_power_on_hours(Some(7200))
        .build();
    disk.smart_temperature_sensors_c = vec![41.0, 43.0];
    disk.smart_available_spare_pct = Some(4.0);
    disk.smart_available_spare_threshold_pct = Some(10.0);
    disk.smart_unsafe_shutdowns = Some(12);

    let rows = disk_stats(&disk, UnitPreferences::default(), &[]);
    assert_eq!(
        keyed_value(&rows, "common.temperature").as_deref(),
        Some("41 / 85 °C"),
        "the shared temperature readout must carry the value and its critical bound"
    );
    let sensor = |index: usize| format!("{} {index}", t("disk.temperature_sensor"));
    assert_eq!(
        row_value(&rows, &sensor(1)).as_deref(),
        Some("41 °C"),
        "each SMART temperature sensor keeps its own named row"
    );
    assert_eq!(row_value(&rows, &sensor(2)).as_deref(), Some("43 °C"));
    assert_eq!(
        keyed_value(&rows, "disk.endurance_used").as_deref(),
        Some("2%")
    );
    assert_eq!(
        keyed_value(&rows, "disk.available_spare").as_deref(),
        Some("4% ⚠ (≤10%)"),
        "spare at/below its warning threshold must carry the threshold"
    );
    assert_eq!(
        keyed_value(&rows, "disk.power_on").as_deref(),
        Some(
            t("disk.power_on_format")
                .replace("{hours}", "7200")
                .replace("{days}", "300")
                .as_str()
        )
    );
    assert_eq!(
        keyed_value(&rows, "disk.unsafe_shutdowns").as_deref(),
        Some("12")
    );

    // A disk whose provider can only report availability keeps that one typed
    // row; a disk with no SMART telemetry at all gets the section hidden.
    let availability_only = DiskMetricsFixtureBuilder::new()
        .smart_availability(SmartAvailability::Available)
        .build();
    let rows = disk_stats(&availability_only, UnitPreferences::default(), &[]);
    assert_eq!(
        keyed_value(&rows, "disk.smart_status").as_deref(),
        Some(t("device.healthy")),
        "a reported-available provider must surface its typed availability status"
    );
    assert_eq!(keyed_value(&rows, "disk.endurance_used"), None);

    let cold = disk_stats(&DiskMetrics::default(), UnitPreferences::default(), &[]);
    for key in [
        "disk.smart_status",
        "common.temperature",
        "disk.endurance_used",
        "disk.available_spare",
        "disk.power_on",
        "disk.unsafe_shutdowns",
    ] {
        assert!(
            keyed_value(&cold, key).is_none(),
            "{key}: an unsupported SMART provider must not fabricate a readout"
        );
    }
}
