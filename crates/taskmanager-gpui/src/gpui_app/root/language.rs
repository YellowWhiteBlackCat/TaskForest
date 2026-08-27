//! Per-window language preference and application i18n activation.

use gpui::Context;

use super::RootView;
use crate::i18n::{self, Language};

impl RootView {
    /// Apply a user-selected language immediately and retain the preference
    /// for the GPUI config writer. Native locale detection never overwrites a
    /// value chosen here.
    pub(crate) fn set_language_preference(&mut self, language: Language, cx: &mut Context<Self>) {
        let mut appearance = self.presentation.appearance();
        appearance.language = Some(language);
        self.presentation.set_appearance(appearance);
        i18n::set_language(language);
        cx.notify();
    }
}
