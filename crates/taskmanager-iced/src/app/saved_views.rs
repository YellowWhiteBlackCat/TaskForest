//! Save/restore bridge between the shared view model and immutable config publications.

use super::IcedApp;
use taskmanager_core::core::config::Config;
use taskmanager_shell::saved_views::{preset_to_config, restore_saved_views};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

impl IcedApp {
    pub(super) fn persist_saved_views(&mut self) {
        let mut config = self.config_draft();
        config.saved_process_views = self
            .saved_views
            .iter()
            .filter_map(preset_to_config)
            .collect();
        self.commit_config_draft(config);
    }

    pub(super) fn restore_saved_views_snapshot(&mut self, config: &Config) {
        if let Err(error) = restore_saved_views(
            &mut self.saved_views,
            &mut self.next_saved_view_id,
            &config.saved_process_views,
        ) {
            self.shell.report_notice(
                FeedbackSource::Settings,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("Saved views not restored: {error}"),
            );
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/saved_views_tests.rs"]
mod tests;
