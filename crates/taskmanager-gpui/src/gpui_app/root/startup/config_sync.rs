//! GPUI projection and non-blocking fold of shared configuration publications.

use super::super::{
    DeviceVisibilityPreferences, GraphPreferences, PresentationFingerprint, SIDEBAR_MAX_WIDTH,
    SIDEBAR_MIN_WIDTH,
};
use super::{
    AsyncApp, ConfigClient, Duration, RootView, SharedString, WeakEntity, apply_process_config,
    config_from_view, i18n, normalize_graph_data_points, normalize_sidebar_preferences,
    startup_page_from_token, text_rendering_from_token,
};

pub(super) fn apply_root_persisted_projection(
    view: &mut RootView,
    cfg: &crate::core::config::Config,
) {
    let mut next = view.presentation_snapshot();
    next.appearance = view.resolved_persisted_appearance(cfg);
    if let Some(density) = super::super::persistence::density_from_token(&cfg.density) {
        next.appearance.density = density;
    }
    next.appearance.ui_size = taskmanager_theme::tokens::UiSize::from_config_token(&cfg.ui_size);
    next.appearance.text_rendering = text_rendering_from_token(&cfg.text_rendering);
    next.appearance.language = cfg.language.as_deref().and_then(i18n::Language::from_code);
    next.startup_page = SharedString::from(startup_page_from_token(&cfg.startup_page));
    next.devices = DeviceVisibilityPreferences {
        cpu: cfg.show_cpu,
        memory: cfg.show_memory,
        disks: cfg.show_disks,
        network: cfg.show_network,
        network_wired: cfg.show_network_wired,
        network_wireless: cfg.show_network_wireless,
        network_vpn: cfg.show_network_vpn,
        network_virtual: cfg.show_network_virtual,
        network_other: cfg.show_network_other,
        gpus: cfg.show_gpus,
    };
    next.units = crate::gpui_app::formatting::DisplayUnits {
        memory_use_bytes: cfg.memory_use_bytes,
        memory_use_base2: cfg.memory_use_base2,
        drive_use_bytes: cfg.drive_use_bytes,
        drive_use_base2: cfg.drive_use_base2,
        network_use_bytes: cfg.network_use_bytes,
        network_use_base2: cfg.network_use_base2,
    };
    next.graphs = GraphPreferences {
        data_points: normalize_graph_data_points(cfg.graph_data_points),
        sliding: cfg.sliding_graphs,
        network_dynamic_scaling: cfg.network_dynamic_scaling,
    };
    view.smart_history
        .set_capacity(next.graphs.data_points as usize);
    let (sidebar_order, sidebar_device_overrides) =
        normalize_sidebar_preferences(&cfg.sidebar_order, &cfg.sidebar_device_overrides);
    next.sidebar.order = sidebar_order;
    next.sidebar.device_overrides = sidebar_device_overrides;
    if cfg.sidebar_width.is_finite() && cfg.sidebar_width >= SIDEBAR_MIN_WIDTH {
        next.sidebar.width = gpui::Pixels::from(cfg.sidebar_width.min(SIDEBAR_MAX_WIDTH));
    }
    next.gray_zero_values = cfg.gray_zero_values;
    view.replace_presentation(next);
    view.shell.set_alert_policy(cfg.notification_policy());
    apply_process_config(view, cfg);
}

pub(super) fn apply_root_runtime_config(
    view: &mut RootView,
    cfg: &crate::core::config::Config,
    cx: &mut gpui::Context<RootView>,
) {
    let history_preference_changed =
        view.history_runtime.enabled_next_start() != cfg.history_persistence;
    apply_root_persisted_projection(view, cfg);
    if history_preference_changed {
        view.history_runtime.request(cfg.history_persistence);
        view.sync_history_persistence_sink();
    }
    view.sync_theme_from_presentation(cx);
    let interval =
        taskmanager_application::TelemetryInterval::clamped(Duration::from_millis(cfg.refresh_ms));
    view.telemetry_refresh_policy
        .apply(taskmanager_application::TelemetryRefreshPolicyChange::SetInterval(interval));
    if let Some(platform) = &mut view.platform {
        platform.set_telemetry_interval(interval);
    }
    if let Some(language) = view.appearance_preferences().language {
        i18n::set_language(language);
    }
    cx.notify();
}

pub(super) fn persist_config_if_due(
    tick: u32,
    weak: &WeakEntity<RootView>,
    cx: &mut AsyncApp,
    config_client: &ConfigClient,
    submitted_presentation: &mut Option<PresentationFingerprint>,
) {
    let Ok(presentation_fingerprint) = weak.update(cx, |view, _cx| view.presentation_fingerprint())
    else {
        return;
    };
    if config_submission_reason(tick, *submitted_presentation, presentation_fingerprint).is_none() {
        return;
    }
    let Ok(config) = weak.update(cx, |view, _cx| config_from_view(view)) else {
        return;
    };
    match config_client.try_submit(config) {
        Ok(_) => *submitted_presentation = Some(presentation_fingerprint),
        Err(error) => {
            let canonical = config_client.snapshot().cloned();
            let _ = weak.update(cx, |view, cx| {
                if let Some(canonical) = canonical {
                    apply_root_runtime_config(view, &canonical, cx);
                }
                view.show_local_feedback(format!("Configuration not queued: {error}"), cx);
            });
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigSubmissionReason {
    PresentationChanged,
    PeriodicSnapshot,
}

fn config_submission_reason(
    tick: u32,
    submitted: Option<PresentationFingerprint>,
    current: PresentationFingerprint,
) -> Option<ConfigSubmissionReason> {
    if submitted != Some(current) {
        Some(ConfigSubmissionReason::PresentationChanged)
    } else if tick.is_multiple_of(25) {
        Some(ConfigSubmissionReason::PeriodicSnapshot)
    } else {
        None
    }
}

pub(super) fn drain_config_publications(
    weak: &WeakEntity<RootView>,
    cx: &mut AsyncApp,
    config_client: &mut ConfigClient,
    applied_revision: &mut Option<taskmanager_application::ConfigRevision>,
) {
    let drain = config_client.drain();
    let mut publications = match drain {
        taskmanager_application::ConfigDrain::Empty => return,
        taskmanager_application::ConfigDrain::Publications(publications) => publications,
        taskmanager_application::ConfigDrain::ResyncRequired {
            missed_publications,
            latest,
        } => {
            let _ = weak.update(cx, |view, cx| {
                view.show_local_feedback(
                    format!(
                        "Configuration resynchronised after {missed_publications} missed updates"
                    ),
                    cx,
                );
            });
            vec![latest]
        }
    };
    for publication in publications.drain(..) {
        let failed = publication.outcome().is_failure();
        if failed || *applied_revision != Some(publication.revision()) {
            let snapshot = publication.snapshot().clone();
            let _ = weak.update(cx, |view, cx| {
                apply_root_runtime_config(view, &snapshot, cx);
            });
        }
        if !failed {
            *applied_revision = Some(publication.revision());
        }
        let feedback = match publication.outcome() {
            taskmanager_application::ConfigPublicationOutcome::SaveFailed { error, .. } => Some(
                format!("Configuration not saved: {}", error.kind().stable_code()),
            ),
            taskmanager_application::ConfigPublicationOutcome::RefreshFailed(recovery) => Some(
                config_recovery_message("Configuration refresh failed", *recovery),
            ),
            taskmanager_application::ConfigPublicationOutcome::Refreshed(recovery) => {
                initial_config_recovery_message(*recovery)
            }
            _ => None,
        };
        if let Some(feedback) = feedback {
            let _ = weak.update(cx, |view, cx| view.show_local_feedback(feedback, cx));
        }
    }
}

pub(super) fn initial_config_recovery_message(
    recovery: taskmanager_application::ConfigRecovery,
) -> Option<String> {
    match recovery.initial_notice() {
        taskmanager_application::ConfigRecoveryNotice::None => None,
        taskmanager_application::ConfigRecoveryNotice::Recovered => {
            Some(config_recovery_message("Configuration recovered", recovery))
        }
        taskmanager_application::ConfigRecoveryNotice::Failed => Some(config_recovery_message(
            "Configuration load failed",
            recovery,
        )),
    }
}

pub(super) fn config_recovery_message(
    prefix: &str,
    recovery: taskmanager_application::ConfigRecovery,
) -> String {
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

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_app/root/config_sync_tests.rs"]
mod tests;
