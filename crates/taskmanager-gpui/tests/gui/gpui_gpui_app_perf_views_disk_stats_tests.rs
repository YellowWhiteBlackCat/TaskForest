use super::{disk_stats, temperature_trend_stat_row, temperature_trend_value};
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_core::core::units::UnitPreferences;

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
            taskmanager_application::i18n::t("common.latest"),
            taskmanager_application::i18n::t("common.avg"),
            taskmanager_application::i18n::t("common.peak"),
        )
    );
}

/// SMART temperature trend produces a typed StatRow::Trend with distinct
/// latest, average, and peak components plus the full raw copy string.
#[test]
fn temperature_trend_stat_row_preserves_multiline_parts_and_raw_value() {
    let row =
        temperature_trend_stat_row(&[35.0, 37.0, 42.0]).expect("finite temperature samples exist");
    assert_eq!(row.label(), taskmanager_application::i18n::t("proc.trend"));
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
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .media_removable(Some(true))
        .smart_power_on_hours(Some(72))
        .build();
    let rows = disk_stats(&disk, UnitPreferences::default(), &[]);
    let find = |key: &'static str| {
        rows.iter()
            .find(|row| row.label() == taskmanager_application::i18n::t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(
        find("disk.removable").value(),
        Some(taskmanager_application::i18n::t("common.yes")),
        "removable row must use the locale label, not hardcoded English"
    );
    assert_eq!(
        find("disk.power_on").value().map(str::to_owned),
        Some(
            taskmanager_application::i18n::t("disk.power_on_format")
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
            .find(|row| row.label() == taskmanager_application::i18n::t(key))
            .unwrap_or_else(|| panic!("{key} row must exist"))
    };
    assert_eq!(find("disk.read").value(), None);
    assert_eq!(find("disk.write").value(), None);
    assert_eq!(find("disk.iops").value(), None);
    assert_eq!(find("disk.active_time").value(), None);
}
