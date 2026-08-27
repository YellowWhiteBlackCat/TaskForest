//! Private Iced ownership for coordinator state and persisted projections.

use taskmanager_application::{Config, ConfigClient, ConfigRevision};
use taskmanager_theme::Theme;

use super::PresentationPreferences;
use crate::i18n::Language;

pub(super) struct IcedConfiguration {
    client: Option<ConfigClient>,
    applied_revision: Option<ConfigRevision>,
    draft: Config,
    preferences: PresentationPreferences,
    language: Language,
    theme: Theme,
}

impl IcedConfiguration {
    pub(super) fn new(
        client: Option<ConfigClient>,
        font_availability: taskmanager_theme::FontAvailability,
    ) -> Self {
        Self {
            client,
            applied_revision: None,
            draft: Config::default(),
            preferences: PresentationPreferences::with_font_availability(font_availability),
            language: Language::En,
            theme: Theme::dark(),
        }
    }

    pub(super) fn client(&self) -> Option<&ConfigClient> {
        self.client.as_ref()
    }

    pub(super) fn client_mut(&mut self) -> Option<&mut ConfigClient> {
        self.client.as_mut()
    }

    pub(super) const fn applied_revision(&self) -> Option<ConfigRevision> {
        self.applied_revision
    }

    pub(super) fn set_applied_revision(&mut self, revision: ConfigRevision) {
        self.applied_revision = Some(revision);
    }

    pub(super) const fn draft(&self) -> &Config {
        &self.draft
    }

    pub(super) const fn preferences(&self) -> &PresentationPreferences {
        &self.preferences
    }

    pub(super) const fn language(&self) -> Language {
        self.language
    }

    pub(super) const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Atomically replace every renderer-visible value derived from one
    /// immutable coordinator snapshot. No caller can advance the draft,
    /// language, or presentation projection independently.
    pub(super) fn apply_snapshot(
        &mut self,
        draft: Config,
        preferences: PresentationPreferences,
        language: Language,
        theme: Theme,
    ) {
        self.draft = draft;
        self.preferences = preferences;
        self.language = language;
        self.theme = theme;
    }
}
