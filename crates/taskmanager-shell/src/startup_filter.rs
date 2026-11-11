//! Shared Startup-page enabled-state filter.

use taskmanager_application::i18n::t;

/// The enabled-state buckets exposed by the Startup filter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartupStatusFilter {
    /// Keep every startup entry regardless of its enabled state.
    #[default]
    All,
    Enabled,
    Disabled,
}

impl StartupStatusFilter {
    /// The complete segmented-control order used by the frontends.
    pub const ALL: [Self; 3] = [Self::All, Self::Enabled, Self::Disabled];

    /// Localized control label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => t("common.all"),
            Self::Enabled => t("common.enabled"),
            Self::Disabled => t("common.disabled"),
        }
    }

    /// Stable non-localized control identity.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// Whether a startup entry's enabled state belongs to this filter.
///
/// The function takes the row fact before the selected filter so callers can
/// compose it with [`crate::matches_startup_text`] without redefining the
/// enabled-state rule.
#[must_use]
pub fn matches_startup_status(enabled: bool, filter: StartupStatusFilter) -> bool {
    match filter {
        StartupStatusFilter::All => true,
        StartupStatusFilter::Enabled => enabled,
        StartupStatusFilter::Disabled => !enabled,
    }
}

#[cfg(test)]
#[path = "../tests/headless/startup_filter.rs"]
mod tests;
