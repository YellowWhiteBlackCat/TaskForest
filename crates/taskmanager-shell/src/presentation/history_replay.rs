//! Common persisted-series headings; identity stays owned by the core key.

use taskforest_core::core::history::HistorySeriesKey;

#[must_use]
pub fn row_heading(key: &HistorySeriesKey) -> String {
    let mut heading = key.metric().slug().to_owned();
    if let Some(device) = key.device() {
        heading.push_str(" · ");
        heading.push_str(device.as_str());
    }
    if let Some(core) = key.core_index() {
        heading.push_str(&format!(" · core {core}"));
    }
    heading
}

/// Format a peak value according to standard scale rules.
#[must_use]
pub fn format_peak(peak: f64) -> String {
    if peak.abs() >= 100.0 {
        format!("{peak:.0}")
    } else if peak.abs() >= 1.0 {
        format!("{peak:.1}")
    } else {
        format!("{peak:.3}")
    }
}
