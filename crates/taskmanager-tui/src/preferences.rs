//! The TUI's applied-preferences surface and the config lifecycle: the
//! renderer mirrors (`AppliedPrefs`), the composition-edge restore
//! (`load_config`), the remember-last page token, and the settings
//! apply/cancel flows. Extracted from [`super`] so the state module stays
//! under the repository's source-size budget. Persisted tokens arrive through
//! immutable config publications; the renderer never performs I/O.

use std::time::Duration;
use taskmanager_shell::saved_views::{sort_from_legacy_token, sort_token};

use crate::TuiApp;
use crate::theme::ThemeParams;
use crate::ui::settings::SettingsForm;
use taskforest_core::core::config::{Config, DeviceVisibilityPreferences};
use taskmanager_application::i18n::t;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::process_category_projection::default_category_expansions;
use taskmanager_application::{AppPage, TelemetryInterval};
use taskmanager_application::{
    ConfigBootstrap, ConfigClient, ConfigDrain, ConfigPublication, ConfigPublicationOutcome,
    ConfigRecoveryNotice, ConfigRevision, ConfigSubmissionStatus, DEFAULT_CONFIG_INITIAL_WAIT,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, SortCol, SortDir};
use taskmanager_theme::ModePreference;
use taskmanager_ui_contract::presentation::settings::{CAPACITY_CHOICES, REFRESH_CHOICES_MS};

/// The applied renderer preferences (mirrors of the persisted `Config` fields
/// the TUI renders). The `Config` type stays opaque to this crate; the
/// projection happens here where the inferred type is in scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedPrefs {
    /// Device-family visibility (order = the form's `show` array).
    pub show: [bool; 10],
    /// Unit matrix (order = the form's `units` array).
    pub units: [bool; 6],
    /// Gray-out zero values on the process table.
    pub gray_zero: bool,
    /// The Performance sparkline window (persisted graph-data-points
    /// preference; the device trends plot their newest N samples).
    pub graph_points: usize,
}

impl Default for AppliedPrefs {
    fn default() -> Self {
        Self {
            show: [true; 10],
            units: [true, true, true, true, false, false],
            gray_zero: false,
            graph_points: 60,
        }
    }
}

/// Lifecycle of the long-lived Settings form draft. A publication arriving
/// while the form is dirty becomes an explicit conflict; it never overwrites
/// the form or lets stale unedited fields flow back into the coordinator.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SettingsDraftLifecycle {
    Clean {
        base_revision: Option<ConfigRevision>,
    },
    Dirty {
        base_revision: Option<ConfigRevision>,
        base: Box<Config>,
    },
    Conflict {
        base_revision: Option<ConfigRevision>,
        base: Box<Config>,
        latest_revision: ConfigRevision,
        latest: Box<Config>,
    },
}

impl Default for SettingsDraftLifecycle {
    fn default() -> Self {
        Self::Clean {
            base_revision: None,
        }
    }
}

impl TuiApp {
    pub(crate) fn history_persistence_enabled(&self) -> bool {
        self.config_draft.history_persistence
    }
    /// Load the persisted configuration: seed the theme parameters, the
    /// settings form (including the refresh-interval choice), and apply the
    /// persisted cadence to the shared telemetry policy. Runs once at the
    /// composition edge; tests inject an app-host-equivalent client backed by
    /// an isolated fixture store.
    pub fn load_config(&mut self) {
        let bootstrap = self
            .config_client
            .as_mut()
            .map(|client| client.wait_for_initial(DEFAULT_CONFIG_INITIAL_WAIT));
        match bootstrap {
            Some(ConfigBootstrap::Published(publication)) => {
                self.apply_config_snapshot(publication.snapshot(), true, true);
                self.applied_config_revision = Some(publication.revision());
                self.settings_draft = SettingsDraftLifecycle::Clean {
                    base_revision: Some(publication.revision()),
                };
                self.report_config_recovery(publication.outcome());
            }
            Some(ConfigBootstrap::Fallback { snapshot, source }) => {
                self.apply_config_snapshot(&snapshot, true, true);
                self.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Configuration fallback: {source:?}"),
                );
            }
            None => self.apply_config_snapshot(&Config::default(), true, true),
        }
    }

    fn apply_config_snapshot(&mut self, config: &Config, startup: bool, update_form: bool) {
        self.restore_saved_views_config(config);
        self.theme_params = ThemeParams::from_config_tokens_with_appearance(
            &config.skin,
            &config.mode,
            config.hc,
            self.observed_appearance,
        );
        if update_form {
            self.settings_form = SettingsForm::from_config_tokens(
                &config.skin,
                &config.mode,
                config.hc,
                &config.ui_font,
                &config.mono_font,
                &config.density,
                (config.notify_enabled, config.notify_quiet_hours),
            );
            self.settings_form.refresh = REFRESH_CHOICES_MS
                .iter()
                .position(|ms| *ms == config.refresh_ms)
                .unwrap_or(1);
            self.settings_form.show = DeviceVisibilityPreferences::from_config(&config).to_array();
            self.settings_form.units = [
                config.memory_use_bytes,
                config.memory_use_base2,
                config.drive_use_bytes,
                config.drive_use_base2,
                config.network_use_bytes,
                config.network_use_base2,
            ];
            self.settings_form.gray_zero = config.gray_zero_values;
            self.settings_form.graph_points = CAPACITY_CHOICES
                .iter()
                .position(|points| *points == config.graph_data_points as usize)
                .unwrap_or(1);
            // Language (G-22): a recorded token seeds the form AND re-applies to
            // the process-global i18n bundle at the composition edge; no recorded
            // preference keeps the host-detected locale (the Config contract).
            self.settings_form.language =
                crate::ui::settings::SettingsForm::language_index_for(config.language.as_deref());
            self.settings_form.history_persistence = config.history_persistence;
            if let Some(language) =
                crate::ui::settings::SettingsForm::language_for_token(config.language.as_deref())
            {
                set_language(language);
            }
        }
        self.prefs = AppliedPrefs {
            show: DeviceVisibilityPreferences::from_config(&config).to_array(),
            units: [
                config.memory_use_bytes,
                config.memory_use_base2,
                config.drive_use_bytes,
                config.drive_use_base2,
                config.network_use_bytes,
                config.network_use_base2,
            ],
            gray_zero: config.gray_zero_values,
            graph_points: usize::try_from(config.graph_data_points).unwrap_or(60),
        };
        self.shell
            .set_telemetry_interval(TelemetryInterval::clamped(Duration::from_millis(
                config.refresh_ms,
            )));
        // The shared rolling history store follows the persisted graph window
        // (G-02): every headline/trend series the TUI renders reads the shell
        // `LiveGraphHistory`, so its capacity — not a frontend-local ring — is
        // what the preference must reach.
        self.shell.set_history_capacity(self.prefs.graph_points);
        // The persisted startup-page preference (GPUI parity): the
        // "performance" / "apps" tokens open that page at launch, overriding
        // the recorded last page; the empty remember-last sentinel restores
        // the persisted last_page token (falling back to Performance).
        if startup {
            let page = match config.startup_page.as_str() {
                "performance" => AppPage::Performance,
                "apps" => AppPage::Applications,
                _ => match config.last_page.as_str() {
                    "apps" => AppPage::Applications,
                    _ => AppPage::Performance,
                },
            };
            self.shell.application.active_page = page;
        }
        // The category tree is the only runtime projection. Config
        // deserialization has already normalized recognized historical mode
        // tokens at the application boundary; no mode value enters TUI state.
        if startup {
            self.expanded_groups = default_category_expansions();
        }
        let hidden: std::collections::HashSet<SortCol> = config
            .process_hidden_columns
            .iter()
            .filter_map(|token| sort_from_legacy_token(token))
            // PID/Name are always-visible identity columns; a hand-edited
            // config listing them is normalized away.
            .filter(|column| !matches!(column, SortCol::Pid | SortCol::Name))
            .collect();
        if config.process_hidden_columns_configured || !hidden.is_empty() {
            self.hidden_columns = hidden;
        }
        if let Some(column) = sort_from_legacy_token(&config.process_sort_col) {
            self.process_sort = (
                column,
                if config.process_sort_asc {
                    SortDir::Asc
                } else {
                    SortDir::Desc
                },
            );
        }
        let history_preference_changed =
            self.config_draft.history_persistence != config.history_persistence;
        self.config_draft = config.clone();
        if !startup && history_preference_changed {
            self.request_history_frontend(config.history_persistence);
        }
    }

    /// Drain in-memory configuration publications. Returns whether renderer
    /// state or typed feedback changed this cycle.
    pub(crate) fn drain_config_publications(&mut self) -> bool {
        let drain = self
            .config_client
            .as_mut()
            .map_or(ConfigDrain::Empty, |client| client.drain());
        match drain {
            ConfigDrain::Empty => false,
            ConfigDrain::Publications(publications) => {
                for publication in publications {
                    self.apply_config_publication(&publication);
                }
                true
            }
            ConfigDrain::ResyncRequired {
                missed_publications,
                latest,
            } => {
                self.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::UntilReplaced,
                    format!(
                        "Configuration stream resynchronised after {missed_publications} missed updates"
                    ),
                );
                self.apply_config_publication(&latest);
                true
            }
        }
    }

    pub(crate) fn apply_config_publication(&mut self, publication: &ConfigPublication) {
        let failed = publication.outcome().is_failure();
        let revision_changed = self.applied_config_revision != Some(publication.revision());
        if revision_changed {
            let previous = std::mem::replace(
                &mut self.settings_draft,
                SettingsDraftLifecycle::Clean {
                    base_revision: Some(publication.revision()),
                },
            );
            self.settings_draft = match previous {
                SettingsDraftLifecycle::Clean { .. } => SettingsDraftLifecycle::Clean {
                    base_revision: Some(publication.revision()),
                },
                SettingsDraftLifecycle::Dirty {
                    base_revision,
                    base,
                }
                | SettingsDraftLifecycle::Conflict {
                    base_revision,
                    base,
                    ..
                } => SettingsDraftLifecycle::Conflict {
                    base_revision,
                    base,
                    latest_revision: publication.revision(),
                    latest: Box::new(publication.snapshot().as_ref().clone()),
                },
            };
        }
        let update_form = matches!(self.settings_draft, SettingsDraftLifecycle::Clean { .. });
        if failed || revision_changed {
            self.apply_config_snapshot(publication.snapshot(), false, update_form);
        }
        self.config_draft = publication.snapshot().as_ref().clone();
        if !failed {
            self.applied_config_revision = Some(publication.revision());
        }
        match publication.outcome() {
            ConfigPublicationOutcome::SaveFailed { error, .. } => {
                let detail =
                    t("tui.settings_save_failed").replacen("{}", error.kind().stable_code(), 1);
                self.settings_form.save_error = Some(detail.clone());
                self.open_local_surface(crate::TuiSurface::Settings);
                self.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    detail,
                );
            }
            ConfigPublicationOutcome::RefreshFailed(recovery) => {
                self.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::UntilReplaced,
                    recovery.message_with_prefix("Configuration refresh failed"),
                );
            }
            outcome => self.report_config_recovery(outcome),
        }
    }

    pub(crate) fn enable_history_for_capture(&mut self) {
        let mut config = self.config_draft.clone();
        config.history_persistence = true;
        let _ = self.commit_config_draft(config);
    }

    pub(crate) fn commit_config_draft(&mut self, config: Config) -> bool {
        let submission = self
            .config_client
            .as_ref()
            .map(|client| client.try_submit(config.clone()));
        match submission {
            None | Some(Ok(ConfigSubmissionStatus::Queued | ConfigSubmissionStatus::NoChange)) => {
                self.apply_config_snapshot(&config, false, true);
                self.settings_draft = SettingsDraftLifecycle::Clean {
                    base_revision: self.applied_config_revision,
                };
                true
            }
            Some(Err(error)) => {
                if let Some(canonical) = self
                    .config_client
                    .as_ref()
                    .and_then(ConfigClient::snapshot)
                    .cloned()
                {
                    self.apply_config_snapshot(&canonical, false, true);
                }
                self.report_notice(
                    FeedbackSource::Settings,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Settings not queued: {error}"),
                );
                false
            }
        }
    }

    fn report_config_recovery(&mut self, outcome: &ConfigPublicationOutcome) {
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
        self.report_notice(
            FeedbackSource::Settings,
            severity,
            FeedbackLifecycle::UntilReplaced,
            recovery.message_with_prefix("Configuration recovered"),
        );
    }

    /// Persist process-list presentation prefs through the bounded shared
    /// configuration coordinator.
    /// Called after every keyboard mutation of those three axes.
    pub(crate) fn persist_process_prefs(&mut self) {
        let mut config = self.config_draft.clone();
        let mut hidden: Vec<String> = self
            .hidden_columns
            .iter()
            .map(|column| sort_token(*column).to_string())
            .collect();
        hidden.sort();
        config.process_hidden_columns = hidden;
        config.process_hidden_columns_configured = true;
        config.process_sort_col = sort_token(self.process_sort.0).to_string();
        config.process_sort_asc = self.process_sort.1 == SortDir::Asc;
        self.commit_config_draft(config);
    }

    /// Persist the selected top-level page as the remember-last token (GPUI
    /// parity). Only the pages the TUI renders are recorded; other tokens
    /// keep their previous value. Best-effort: a save failure only surfaces
    /// in the status line, never blocks navigation.
    pub(super) fn persist_last_page(&mut self) {
        let token = match self.page() {
            AppPage::Performance => "performance",
            AppPage::Applications => "apps",
            _ => return,
        };
        let mut config = self.config_draft.clone();
        config.last_page = token.to_string();
        self.commit_config_draft(config);
    }

    /// Apply the settings form to a client-local draft and queue its patch.
    /// Returns false only when the bounded command lane rejects it; filesystem
    /// failures arrive later as typed publications.
    #[must_use]
    pub fn apply_settings_form(&mut self) -> bool {
        if matches!(self.settings_draft, SettingsDraftLifecycle::Conflict { .. }) {
            let error = "configuration changed externally; cancel and reopen Settings".to_owned();
            self.settings_form.save_error = Some(error.clone());
            self.report_notice(
                FeedbackSource::Settings,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::UntilReplaced,
                error,
            );
            return false;
        }
        let mut config = match &self.settings_draft {
            SettingsDraftLifecycle::Dirty { base, .. } => base.as_ref().clone(),
            SettingsDraftLifecycle::Clean { .. } => self.config_draft.clone(),
            SettingsDraftLifecycle::Conflict { .. } => return false,
        };
        crate::ui::settings::apply_settings_to_config(
            &self.settings_form,
            &mut config,
            &mut self.theme_params,
        );
        // Only a follow-native preference re-resolves; the mode token is the
        // theme owner's codec, never a string this frontend compares (ADR-063).
        if ModePreference::from_config_token(&config.mode).follows_native() {
            self.theme_params = ThemeParams::from_config_tokens_with_appearance(
                &config.skin,
                &config.mode,
                config.hc,
                self.observed_appearance,
            );
        }
        if self.commit_config_draft(config) {
            self.settings_form.save_error = None;
            // The notification policy (opt-in + quiet hours) applies
            // immediately to the shared alert center.
            self.shell
                .set_alert_policy(self.settings_form.notification_policy());
            // The refresh interval drives the shared telemetry policy (the
            // same cadence authority the graphical frontends set).
            self.shell
                .set_telemetry_interval(TelemetryInterval::clamped(Duration::from_millis(
                    self.settings_form.refresh_ms(),
                )));
            // Language write-through (G-22): the token is already saved
            // by `apply_settings`; apply the choice to the process-global
            // i18n bundle so the very next frame renders localized.
            set_language(
                crate::ui::settings::SettingsForm::language_for_token(Some(
                    self.settings_form.language_token(),
                ))
                .unwrap_or(Language::En),
            );
            self.prefs = AppliedPrefs {
                show: self.settings_form.show,
                units: self.settings_form.units,
                gray_zero: self.settings_form.gray_zero,
                graph_points: self.settings_form.graph_points(),
            };
            // TUI-002 tail: the visibility toggles are live THIS frame. The
            // Performance resource anchor re-checks the selection against the
            // just-applied `show` set (the same projection-backed visibility
            // the batch fold consults), so a resource whose family the save
            // just switched off fails closed to the first still-backed
            // resource now instead of waiting for the next platform batch;
            // enabling a family only adds resources, so a still-visible
            // selection does not drift.
            self.reconcile_perf_device_anchor();
            // The graph window preference re-points the shared rolling
            // history store every frontend graph reads (G-02).
            self.shell.set_history_capacity(self.prefs.graph_points);
            let skin = self.theme_params.skin.label();
            self.report_notice(
                FeedbackSource::Settings,
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                format!("Settings saved · {skin}"),
            );
            self.dismiss_local_surface_kind(crate::TuiSurfaceKind::Settings);
            true
        } else {
            let error = "configuration queue unavailable".to_owned();
            self.settings_form.save_error = Some(error.clone());
            self.report_notice(
                FeedbackSource::Settings,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("Settings not queued: {error}"),
            );
            false
        }
    }

    /// Freeze the configuration base when the first form value changes.
    pub(crate) fn begin_settings_edit(&mut self) {
        if matches!(self.settings_draft, SettingsDraftLifecycle::Clean { .. }) {
            self.settings_draft = SettingsDraftLifecycle::Dirty {
                base_revision: self.applied_config_revision,
                base: Box::new(self.config_draft.clone()),
            };
        }
    }

    /// Cancel the settings form: close the overlay and re-seed the form from
    /// the persisted configuration (or defaults), so dismissed edits never
    /// leak into the next open.
    pub fn cancel_settings(&mut self) {
        let config = match &self.settings_draft {
            SettingsDraftLifecycle::Conflict { latest, .. } => latest.as_ref().clone(),
            _ => self.config_draft.clone(),
        };
        self.settings_form = SettingsForm::from_config_tokens(
            &config.skin,
            &config.mode,
            config.hc,
            &config.ui_font,
            &config.mono_font,
            &config.density,
            (config.notify_enabled, config.notify_quiet_hours),
        );
        self.settings_form.refresh = REFRESH_CHOICES_MS
            .iter()
            .position(|ms| *ms == config.refresh_ms)
            .unwrap_or(1);
        self.settings_form.show = DeviceVisibilityPreferences::from_config(&config).to_array();
        self.settings_form.units = [
            config.memory_use_bytes,
            config.memory_use_base2,
            config.drive_use_bytes,
            config.drive_use_base2,
            config.network_use_bytes,
            config.network_use_base2,
        ];
        self.settings_form.gray_zero = config.gray_zero_values;
        self.settings_form.graph_points = CAPACITY_CHOICES
            .iter()
            .position(|points| *points == config.graph_data_points as usize)
            .unwrap_or(1);
        self.settings_form.language =
            crate::ui::settings::SettingsForm::language_index_for(config.language.as_deref());
        self.settings_form.history_persistence = config.history_persistence;
        self.settings_form.save_error = None;
        self.settings_draft = SettingsDraftLifecycle::Clean {
            base_revision: self.applied_config_revision,
        };
        self.dismiss_local_surface_kind(crate::TuiSurfaceKind::Settings);
    }
}
