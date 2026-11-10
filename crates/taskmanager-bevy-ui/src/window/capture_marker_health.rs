//! Capture readiness for the System health-review surfaces.

use super::capture_marker::CaptureAccess;
use crate::app::Page;
use taskmanager_application::i18n::t;
use taskmanager_application::system_timeline::SystemPageSection;
use taskmanager_shell::presentation::health_review::HealthReviewSection;

/// Prove that the requested health section and its measured labels are painted.
pub(super) fn health_review_ready(
    access: &CaptureAccess<'_, '_>,
    route: Page,
    scenario: &str,
) -> bool {
    let expected = if scenario == "sensor-center" {
        HealthReviewSection::Sensors
    } else {
        HealthReviewSection::Storage
    };
    if route != Page::System
        || access.system_state.section != SystemPageSection::Health
        || access.system_state.health_section != expected
    {
        return false;
    }
    let projection = access.track.shell.projection();
    if !projection
        .storage_health_projection()
        .is_some_and(|(snapshot, _)| snapshot.filesystems.len() == 3)
        || !projection
            .sensors
            .as_ref()
            .is_some_and(|snapshot| snapshot.readings.len() == 4)
    {
        return false;
    }
    let labels: &[&str] = if scenario == "sensor-center" {
        &["CPU package", "Chassis fan", "Package power"]
    } else {
        &[t("health.read_only"), t("health.errors_reported")]
    };
    labels.iter().all(|label| {
        access.text.iter().any(|(text, node)| {
            node.size().x > 0.0 && node.size().y > 0.0 && text.0.contains(label)
        })
    })
}
