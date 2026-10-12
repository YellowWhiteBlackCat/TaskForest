//! Pure disk detail header projections.

use taskmanager_core::core::metrics::DiskMetrics;

/// One disk's display identity (GPUI parity): the model when known, else the
/// `/dev/`-stripped name — no family prefix.
#[must_use]
pub(crate) fn disk_title(disk: &DiskMetrics) -> String {
    if disk.model.trim().is_empty() {
        disk.name.trim_start_matches("/dev/").to_string()
    } else {
        disk.model.trim().to_string()
    }
}

/// A disk's subtitle facts. The device name supplements a known model title;
/// when the name is already the title, keep only the independent type/filesystem
/// facts so it is not repeated in the header.
#[must_use]
pub(crate) fn disk_subtitle(disk: &DiskMetrics) -> String {
    let mut parts = Vec::new();
    if !disk.model.trim().is_empty() {
        parts.push(disk.name.trim_start_matches("/dev/").trim());
    }
    parts.extend([disk.disk_type.trim(), disk.fs_type.trim()]);
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}
