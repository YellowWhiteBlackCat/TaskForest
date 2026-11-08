//! The process-properties section tabs: one product vocabulary for every
//! frontend.
//!
//! The Properties dialog (and the TUI properties modal, the Iced details
//! overlay, the Bevy properties scene) all split the same process record
//! into the same four sections. Each frontend used to carry its own copy of
//! that enum, its own section order and its own catalog-key map, so the tab
//! strip could drift into four different sets and four different labels.
//!
//! This module owns the fact: which sections exist, in which order, which
//! stable token names one, and which catalog key carries its label. The
//! localized copy itself stays with the i18n owner
//! (`crate::i18n::process_details_section_label`).

/// The sections of the process-properties surface, in the canonical order
/// every frontend walks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ProcessDetailsSection {
    /// The identity and current-observation summary.
    #[default]
    Overview,
    /// The performance history and its charts.
    Performance,
    /// The command line and the executable facts.
    Command,
    /// The insights facets derived from the observation.
    Insights,
}

impl ProcessDetailsSection {
    /// Every section in the canonical tab order.
    ///
    /// Enumerated once here (not re-listed at a call site) so the quality
    /// gate's "no duplicated variant list" rule holds for the tab strips.
    pub const ALL: [Self; 4] = [
        Self::Overview,
        Self::Performance,
        Self::Command,
        Self::Insights,
    ];

    /// The stable, non-localized token naming one section.
    ///
    /// Frontends compose their own id namespaces around this token (an
    /// element id, a focus-operation id); the token itself is one product
    /// fact.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Performance => "performance",
            Self::Command => "command",
            Self::Insights => "insights",
        }
    }

    /// The catalog key carrying this section's label.
    ///
    /// The key belongs to the vocabulary; the copy behind it belongs to the
    /// i18n owner, which resolves it through
    /// [`crate::i18n::process_details_section_label`].
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Overview => "prop.overview",
            Self::Performance => "prop.performance",
            Self::Command => "prop.command",
            Self::Insights => "prop.insights",
        }
    }

    /// The next section in the canonical cycle.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Overview => Self::Performance,
            Self::Performance => Self::Command,
            Self::Command => Self::Insights,
            Self::Insights => Self::Overview,
        }
    }

    /// The previous section in the canonical cycle.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Overview => Self::Insights,
            Self::Performance => Self::Overview,
            Self::Command => Self::Performance,
            Self::Insights => Self::Command,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_process_details_section_tests.rs"]
mod tests;
