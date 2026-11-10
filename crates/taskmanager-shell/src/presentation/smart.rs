//! Visibility of observed SMART facts and actionable source failures.
//!
//! One rule binds the four frontends' SMART readouts: a reading that crossed
//! its typed level wears its marker on EVERY surface that prints it. The
//! endurance percentage carries the critical marker whenever the device
//! reached its rated life; the spare percentage carries its marker with the
//! observed threshold whenever the pool fell to its warning level. The
//! judgment belongs to core (`is_nvme_wear_critical`,
//! `DiskMetrics::smart_spare_pool_needs_warning`), the words belong to the
//! catalog (`disk.exceeded_rated_life`), and the marker's FORM belongs to the
//! shape: a text glyph where the surface paints text, an icon plate where the
//! surface may not carry a decoration codepoint. The SMART dialog and overlay
//! are the surfaces with room for a sentence, so they spell the reason out; a
//! compact row stays a marker.

use super::constants::ALERT_MARKER;
use super::has_smart_fields;
use super::percent::observed_percentage;
use taskforest_core::core::StorageDeviceKey;
use taskforest_core::core::identity::DeviceId;
use taskforest_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskforest_core::core::smart::{SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestSupport};
use taskforest_core::core::storage_health::is_nvme_wear_critical;
use taskforest_core::core::system_health::SmartSelfTestIntent;

/// The endurance-used readout for a surface that paints text: the level
/// percent, plus the alert marker when the device reached its rated life.
///
/// One composition for every text surface. The judgment is core's
/// ([`is_nvme_wear_critical`]), the percent spelling is the shared
/// absence-aware one ([`observed_percentage`]), and the marker is the shared
/// text form ([`ALERT_MARKER`]) — a surface that may not carry a decoration
/// codepoint paints an icon plate instead, so the FORM differs by shape while
/// the reading does not.
///
/// `rated_life_note` is the sentence a surface with room for one appends to
/// the marker, from the copy catalog (`disk.exceeded_rated_life`). A compact
/// row passes `None` and stays a marker.
///
/// Five surfaces compose this readout. The Iced compact row spelled the
/// percent through the bare `percent_text` while the GPUI and TUI compact rows
/// used the absence-aware owner readout, so a non-finite wear reading printed
/// `inf%` on one surface and the shared dash on another; the two annotated
/// surfaces agreed on the sentence only by coincidence, since one read it from
/// the catalog and the other wrote the English words into source. Before R131
/// the three compact rows wore no marker at all while the dialog and the
/// overlay did — the paint divergence R131 closed by growing one private helper
/// per frontend, which made the rule uniform and the composition five copies.
/// This is the one address.
#[must_use]
pub fn endurance_used_readout(percent_used: Option<f32>, rated_life_note: Option<&str>) -> String {
    let value = observed_percentage(percent_used);
    // A reading that is not a number is absent, and absence wears no marker:
    // `is_nvme_wear_critical` answers `true` for infinity, so the finiteness
    // gate has to come first or an unmeasured device would be reported as past
    // its rated life next to a dash.
    let Some(percent) = percent_used.filter(|value| value.is_finite()) else {
        return value;
    };
    if !is_nvme_wear_critical(percent) {
        return value;
    }
    match rated_life_note {
        Some(note) => format!("{value} {ALERT_MARKER} {note}"),
        None => format!("{value} {ALERT_MARKER}"),
    }
}

/// The available-spare readout for a surface that paints text: the observed
/// pool percent, wearing the alert marker plus the threshold it fell to when
/// the pool crossed its warning level.
///
/// The judgment and the threshold are core's
/// ([`taskforest_core::core::metrics::DiskMetrics::smart_spare_pool_needs_warning`],
/// [`taskforest_core::core::metrics::DiskMetrics::smart_spare_warning_threshold_pct`]);
/// this owns the ONE composition, because five surfaces used to write it and
/// each of them wrote the marker codepoint for itself.
///
/// `warning_clause` is the localized clause a surface with room for a sentence
/// prints beside the marker — the catalog's `disk.spare_warning_clause`, whose
/// `{threshold}` slot this fills. A compact row passes `None` and keeps the
/// bare symbol clause, which carries no words and therefore needs no catalog.
/// The clause is copy, so it belongs to the catalogs: two surfaces used to
/// spell the English word "warning" inside their source, printing English to
/// every user regardless of language.
#[must_use]
pub fn spare_pool_readout(disk: &DiskMetrics, warning_clause: Option<&str>) -> String {
    let spare = disk.smart_available_spare_pct;
    let threshold = disk.smart_spare_warning_threshold_pct();
    let value = observed_percentage(spare);
    if !disk.smart_spare_pool_needs_warning() {
        return value;
    }
    let threshold = observed_percentage(Some(threshold));
    match warning_clause {
        Some(clause) => format!(
            "{value} {ALERT_MARKER} {}",
            clause.replace("{threshold}", &threshold)
        ),
        None => format!("{value} {ALERT_MARKER} (≤{threshold})"),
    }
}

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

/// The SMART temperature label a text surface prints: the localized
/// temperature name, wearing the alert marker when the device reported its
/// critical-warning bit.
///
/// The bit is the most actionable fact the hwmon layer carries, so a surface
/// that names the temperature also says which fact is flagged. Four surfaces
/// composed this themselves — two compact rows, one dialog and one overlay —
/// and one of them wrote the English word for the label into source instead of
/// reading the catalog, so that label ignored the language for every user.
#[must_use]
pub fn smart_temperature_label(temperature_label: &str, critical_warning: bool) -> String {
    if critical_warning {
        format!("{temperature_label} {ALERT_MARKER}")
    } else {
        temperature_label.to_owned()
    }
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

#[cfg(test)]
#[path = "../../tests/headless/presentation/smart_tests.rs"]
mod tests;
