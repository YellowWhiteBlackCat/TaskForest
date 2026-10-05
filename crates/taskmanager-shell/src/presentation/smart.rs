//! Visibility of observed SMART facts and actionable source failures.

use super::has_smart_fields;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;

/// Freeze a control's physical identity before a later activation can reorder inventory.
pub fn self_test_intent(disk: &DiskMetrics, kind: SmartSelfTestKind) -> SmartSelfTestIntent {
    SmartSelfTestIntent {
        device_id: DeviceId::new(disk.device_id.clone()),
        device_generation: disk.device_generation,
        device_key: StorageDeviceKey::new(disk.name.clone()),
        display_name: if disk.model.is_empty() {
            disk.name.clone()
        } else {
            disk.model.clone()
        },
        kind,
    }
}

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
