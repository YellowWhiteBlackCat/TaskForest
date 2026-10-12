//! Visibility of observed SMART facts and actionable source failures.

use super::has_smart_fields;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskmanager_core::core::smart::{
    SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestSupport,
};
use taskmanager_core::core::system_health::SmartSelfTestIntent;

/// Whether the device has explicit support evidence for the requested test kind.
#[must_use]
pub fn self_test_supported(disk: &DiskMetrics, kind: SmartSelfTestKind) -> bool {
    disk.smart_self_test_capabilities.support_for(kind) == SmartSelfTestSupport::Supported
}

/// Freeze an explicitly supported test target before a later activation can
/// reorder inventory. Health availability never substitutes for per-kind
/// device support evidence.
#[must_use]
pub fn self_test_intent(
    disk: &DiskMetrics,
    kind: SmartSelfTestKind,
) -> Option<SmartSelfTestIntent> {
    (self_test_supported(disk, kind)
        && !disk.device_id.is_empty()
        && disk.device_generation.is_valid())
    .then(|| SmartSelfTestIntent {
        device_id: DeviceId::new(disk.device_id.clone()),
        device_generation: disk.device_generation,
        device_key: StorageDeviceKey::new(disk.name.clone()),
        display_name: if disk.model.is_empty() {
            disk.name.clone()
        } else {
            disk.model.clone()
        },
        kind,
    })
}

/// Keep actionable SMART failures visible even before a reading exists.
/// Unsupported or unobserved sources stay absent unless they retain real fields.
#[must_use]
pub fn smart_section_visible(disk: &DiskMetrics) -> bool {
    has_smart_fields(disk)
        || [
            disk.smart_self_test_capabilities.short,
            disk.smart_self_test_capabilities.extended,
            disk.smart_self_test_capabilities.conveyance,
        ]
        .into_iter()
        .any(|support| support != SmartSelfTestSupport::Unknown)
        || [
            SmartAvailability::Available,
            SmartAvailability::MissingTool,
            SmartAvailability::PermissionDenied,
        ]
        .contains(&disk.smart_availability)
}

/// Shared localization key for a terminal SMART self-test failure.
#[must_use]
pub const fn smart_self_test_failure_i18n_key(failure: SmartSelfTestFailure) -> &'static str {
    match failure {
        SmartSelfTestFailure::InvalidDevice => "health.failure_invalid_device",
        SmartSelfTestFailure::SupportUnknown => "health.failure_support_unknown",
        SmartSelfTestFailure::Unsupported => "health.failure_unsupported",
        SmartSelfTestFailure::MissingTool => "device.missing_tool",
        SmartSelfTestFailure::RequiresEscalation => "device.requires_escalation",
        SmartSelfTestFailure::PermissionDenied => "device.permission_denied",
        SmartSelfTestFailure::AuthorizationUnavailable => {
            "health.failure_authorization_unavailable"
        }
        SmartSelfTestFailure::HelperUnavailable => "health.failure_helper_unavailable",
        SmartSelfTestFailure::HelperProtocolViolation => "health.failure_helper_protocol",
        SmartSelfTestFailure::TimedOut | SmartSelfTestFailure::ProviderUnavailable => {
            "health.failure_provider_unavailable"
        }
        SmartSelfTestFailure::Rejected => "health.failure_rejected",
    }
}
