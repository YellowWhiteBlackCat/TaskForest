//! Common persisted-series headings; identity stays owned by the core key.

use taskmanager_core::core::history::HistorySeriesKey;

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
