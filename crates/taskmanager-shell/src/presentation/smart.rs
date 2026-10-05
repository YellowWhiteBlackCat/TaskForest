//! Visibility of observed SMART facts and actionable source failures.

use super::has_smart_fields;
use taskmanager_core::core::metrics::{DiskMetrics, SmartAvailability};

/// Keep actionable SMART failures visible even before a reading exists.
/// Unsupported or unobserved sources stay absent unless they retain real fields.
#[must_use]
pub fn smart_section_visible(disk: &DiskMetrics) -> bool {
    has_smart_fields(disk)
        || [
            SmartAvailability::Available,
            SmartAvailability::MissingTool,
            SmartAvailability::PermissionDenied,
        ]
        .contains(&disk.smart_availability)
}
