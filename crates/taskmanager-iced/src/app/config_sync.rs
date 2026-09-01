//! Iced projection of the shared configuration coordinator.
//!
//! This module is the only bridge between immutable config publications and
//! the renderer presentation projection. It never performs filesystem I/O.

use std::time::Duration;

use super::*;
use taskmanager_application::{
    ConfigBootstrap, ConfigBootstrapFallback, ConfigDrain, ConfigPublicationOutcome,
    ConfigRecoveryNotice, ConfigSubmissionStatus, DEFAULT_CONFIG_INITIAL_WAIT,
};

impl IcedApp {
    /// Apply the first immutable publication before the first production
    /// frame. A bounded timeout uses a typed default fallback; later ticks can
    /// still converge through the private `drain_config_publications` tick.
    pub fn load_config(&mut self) {
        let bootstrap = self
            .configuration
            .client_mut()
            .map(|client| client.wait_for_initial(DEFAULT_CONFIG_INITIAL_WAIT));
        match bootstrap {
            Some(ConfigBootstrap::Published(publication)) => {
                self.apply_config_snapshot(publication.snapshot(), true);
                self.configuration
                    .set_applied_revision(publication.revision());
                self.report_recovery(publication.outcome());
            }
            Some(ConfigBootstrap::Fallback { snapshot, source }) => {
                self.apply_config_snapshot(&snapshot, true);
                let detail = match source {
                    ConfigBootstrapFallback::TimedOutDefault => {
                        "Configuration load timed out; using defaults until recovery"
                    }
                    ConfigBootstrapFallback::WorkerStoppedDefault => {
                        "Configuration worker stopped; using defaults"
                    }
                };
                self.shell.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    detail,
                );
            }
            None => self.apply_config_snapshot(&Config::default(), true),
        }
    }

    /// Drain configuration publications from the in-memory replay log. This
    /// method is safe on the Iced tick because it never waits or touches disk.
    pub(super) fn drain_config_publications(&mut self) {
        let drain = self
            .configuration
            .client_mut()
            .map_or(ConfigDrain::Empty, ConfigClient::drain);
        match drain {
            ConfigDrain::Empty => {}
            ConfigDrain::Publications(publications) => {
                for publication in publications {
                    self.apply_config_publication(&publication);
                }
            }
            ConfigDrain::ResyncRequired {
                missed_publications,
                latest,
            } => {
                self.shell.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::UntilReplaced,
                    format!(
                        "Configuration stream resynchronised after {missed_publications} missed updates"
                    ),
                );
                self.apply_config_publication(&latest);
            }
        }
    }

    pub(super) fn commit_config_draft(&mut self, config: Config) {
        let submission = self
            .configuration
            .client()
            .map(|client| client.try_submit(config.clone()));
        match submission {
            None | Some(Ok(ConfigSubmissionStatus::Queued | ConfigSubmissionStatus::NoChange)) => {
                self.apply_config_snapshot(&config, false);
            }
            Some(Err(error)) => {
                if let Some(canonical) = self
                    .configuration
                    .client()
                    .and_then(ConfigClient::snapshot)
                    .cloned()
                {
                    self.apply_config_snapshot(&canonical, false);
                }
                self.shell.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Settings not queued: {error}"),
                );
            }
        }
    }

    #[must_use]
    pub(super) fn config_draft(&self) -> Config {
        self.configuration.draft().clone()
    }

    pub(crate) fn apply_config_publication(
        &mut self,
        publication: &taskmanager_application::ConfigPublication,
    ) {
        let failed = publication.outcome().is_failure();
        if failed || self.configuration.applied_revision() != Some(publication.revision()) {
            self.apply_config_snapshot(publication.snapshot(), false);
        }
        if !failed {
            self.configuration
                .set_applied_revision(publication.revision());
        }
        match publication.outcome() {
            ConfigPublicationOutcome::SaveFailed { error, .. } => self.shell.report_notice(
                FeedbackSource::Settings,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("Settings not saved: {error}"),
            ),
            ConfigPublicationOutcome::RefreshFailed(recovery) => self.shell.report_notice(
                FeedbackSource::Settings,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::UntilReplaced,
                recovery_message("Configuration refresh failed", *recovery),
            ),
            outcome => self.report_recovery(outcome),
        }
    }

    pub(super) fn apply_config_snapshot(&mut self, config: &Config, startup: bool) {
        let history_preference_changed =
            self.configuration.draft().history_persistence != config.history_persistence;
        let language = config
            .language
            .as_deref()
            .and_then(crate::i18n::Language::from_token)
            .unwrap_or_else(|| self.configuration.language());
        let (preferences, theme) = self.resolve_preferences_and_theme(config);
        self.configuration
            .apply_snapshot(config.clone(), preferences, language, theme);
        if !startup && history_preference_changed {
            self.request_history_frontend(config.history_persistence);
        }
        crate::i18n::sync_shared_language(language);
        self.configuration
            .set_focus_visible(self.input.modality.shows_focus_ring());
        self.shell
            .set_telemetry_interval(TelemetryInterval::clamped(Duration::from_millis(
                config.refresh_ms,
            )));
        self.shell
            .set_history_capacity(usize::try_from(config.graph_data_points).unwrap_or(60));
        self.shell.set_alert_policy(config.notification_policy());
        if startup {
            self.apply_startup_page(config);
            // Restore the persisted column-width token once at boot. Later
            // snapshots (echoes of this frontend's own commits, external
            // refresh publications) deliberately leave the override set
            // alone: the live session — including an open drag — keeps its
            // authority until the next launch.
            self.process_column_sizing.overrides =
                ColumnWidthOverrides::from_config(&config.process_col_widths);
        }
    }

    fn apply_startup_page(&mut self, config: &Config) {
        self.shell.application.active_page = match config.startup_page.as_str() {
            "performance" => AppPage::Performance,
            "apps" => AppPage::Applications,
            _ if config.last_page == "apps" => AppPage::Applications,
            _ => AppPage::Performance,
        };
    }

    fn report_recovery(&mut self, outcome: &ConfigPublicationOutcome) {
        let recovery = match outcome {
            ConfigPublicationOutcome::Loaded(recovery)
            | ConfigPublicationOutcome::Refreshed(recovery) => *recovery,
            _ => return,
        };
        let severity = match recovery.initial_notice() {
            ConfigRecoveryNotice::None => return,
            ConfigRecoveryNotice::Recovered => FeedbackSeverity::Warning,
            ConfigRecoveryNotice::Failed => FeedbackSeverity::Error,
        };
        self.shell.report_notice(
            FeedbackSource::Settings,
            severity,
            FeedbackLifecycle::UntilReplaced,
            recovery_message("Configuration recovered", recovery),
        );
    }
}

fn recovery_message(prefix: &str, recovery: taskmanager_application::ConfigRecovery) -> String {
    format!(
        "{prefix}: source={:?}, primary={}, backup={}",
        recovery.source(),
        recovery.primary_error().map_or(
            "none",
            taskmanager_application::ConfigStoreErrorKind::stable_code
        ),
        recovery.backup_error().map_or(
            "none",
            taskmanager_application::ConfigStoreErrorKind::stable_code
        ),
    )
}
