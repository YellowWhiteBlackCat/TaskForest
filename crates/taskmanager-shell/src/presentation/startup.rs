//! Single shared presentation projections for startup applications and systemd services.

use taskforest_core::core::startup::{StartupEntry, StartupImpactEvidence};
use taskmanager_application::i18n;

/// The source column with its scope suffix (GPUI/TUI/Bevy/Iced parity: the row reads
/// `Desktop Entry · User` instead of the bare provider label).
#[must_use]
pub fn startup_source_text(entry: &StartupEntry) -> String {
    format!(
        "{} · {}",
        entry.source.as_str(),
        i18n::t(entry.scope.i18n_key())
    )
}

/// The impact column with its evidence (GPUI/TUI/Bevy/Iced parity: `Low · 42 ms` for a
/// measured boot impact, `Low · unmeasured` when the provider could not
/// instrument it — never a fabricated duration).
#[must_use]
pub fn startup_impact_text(entry: &StartupEntry) -> String {
    match entry.impact_evidence {
        StartupImpactEvidence::Measured { duration_ms } => {
            format!("{} · {duration_ms} ms", i18n::t(entry.impact.i18n_key()))
        }
        StartupImpactEvidence::Unknown { .. } => {
            format!(
                "{} · {}",
                i18n::t(entry.impact.i18n_key()),
                i18n::t("startup.impact_unmeasured")
            )
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/startup_tests.rs"]
mod tests;
