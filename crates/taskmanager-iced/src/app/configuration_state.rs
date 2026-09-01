//! Private Iced ownership for coordinator state and persisted projections.

use taskmanager_application::{ConfigClient, ConfigRevision};
use taskmanager_core::core::config::Config;

use taskmanager_theme::Theme;
use taskmanager_theme::tokens::MotionPolicy;

use super::PresentationPreferences;
use crate::i18n::Language;

pub(super) struct IcedConfiguration {
    client: Option<ConfigClient>,
    applied_revision: Option<ConfigRevision>,
    draft: Config,
    preferences: PresentationPreferences,
    language: Language,
    theme: Theme,
    motion_policy: MotionPolicy,
    observed_color_scheme: Option<super::appearance::OsColorScheme>,
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
            motion_policy: MotionPolicy::Normal,
            observed_color_scheme: None,
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

    pub(super) const fn motion_policy(&self) -> MotionPolicy {
        self.motion_policy
    }

    pub(super) fn set_focus_visible(&mut self, focus_visible: bool) {
        self.theme = self.theme.with_focus_visible(focus_visible);
    }

    pub(super) const fn observed_color_scheme(&self) -> Option<super::appearance::OsColorScheme> {
        self.observed_color_scheme
    }

    pub(super) fn set_observed_color_scheme(
        &mut self,
        observed: Option<super::appearance::OsColorScheme>,
    ) {
        self.observed_color_scheme = observed;
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
        self.motion_policy = super::motion::motion_policy_from_token(draft.motion.as_str());
        self.draft = draft;
        self.preferences = preferences;
        self.language = language;
        self.theme = theme;
    }
}
