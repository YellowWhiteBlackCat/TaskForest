use super::*;

use crate::core::alerts::{AlertMetric, AlertRule, AlertSeverity, NotificationPolicy};
use crate::core::metrics::{CpuMetrics, CpuScalarObservations, ScalarObservation, SystemSnapshot};
use crate::gpui_app::processes_view::rows::SortCol;
use crate::gpui_app::root::TopPage;
use gpui::{AppContext, TestAppContext};

#[test]
fn presentation_changes_submit_on_the_next_owner_tick_without_duplicate_resubmission() {
    let initial = PresentationFingerprint::default();
    assert_eq!(
        config_submission_reason(1, None, initial),
        Some(ConfigSubmissionReason::PresentationChanged),
        "the owner must publish its initial canonical projection"
    );
    assert_eq!(config_submission_reason(2, Some(initial), initial), None);

    let changed = PresentationFingerprint {
        appearance: initial.appearance + 1,
        ..initial
    };
    assert_eq!(
        config_submission_reason(3, Some(initial), changed),
        Some(ConfigSubmissionReason::PresentationChanged),
        "a presentation mutation must not wait for the periodic save window"
    );
    assert_eq!(
        config_submission_reason(25, Some(changed), changed),
        Some(ConfigSubmissionReason::PeriodicSnapshot),
        "non-presentation persisted state still receives its bounded periodic fold"
    );
}

#[test]
fn pristine_first_launch_has_no_gpui_recovery_feedback() {
    let dir = crate::test_support::scratch_dir("config-pristine-default");
    let path = dir.join("config.json");
    let coordinator = taskmanager_application::ConfigCoordinator::start_path(&path)
        .expect("start configuration runtime");
    let mut client = coordinator.client();
    let taskmanager_application::ConfigBootstrap::Published(publication) =
        client.wait_for_initial(std::time::Duration::from_secs(2))
    else {
        panic!("expected initial publication");
    };
    let taskmanager_application::ConfigPublicationOutcome::Loaded(recovery) = publication.outcome()
    else {
        panic!("expected initial load outcome");
    };

    assert!(initial_config_recovery_message(*recovery).is_none());

    drop(client);
    drop(coordinator);
    crate::test_support::remove_scratch(&dir);
}

#[gpui::test]
fn runtime_config_apply_preserves_ephemeral_alert_history_and_runtime_owners(
    cx: &mut TestAppContext,
) {
    let root = cx.new(|cx| RootView::new(crate::gpui_app::theme::Theme::dark(), cx));
    let snapshot = SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(95.0, 1_000),
            ..Default::default()
        }),
        ..Default::default()
    };
    let rule = AlertRule::new(
        "config-preserves-alert",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        90.0,
        std::time::Duration::ZERO,
        0.0,
    );
    let config = taskmanager_application::Config {
        notify_enabled: true,
        history_persistence: true,
        ui_size: "Large".into(),
        show_cpu: false,
        network_use_bytes: true,
        graph_data_points: 240,
        sidebar_width: 320.0,
        gray_zero_values: true,
        language: Some("zh".into()),
        process_hidden_columns_configured: true,
        process_hidden_columns: Vec::new(),
        ..taskmanager_application::Config::default()
    };

    root.update(cx, |view, cx| {
        view.page = TopPage::Apps;
        view.pending_search_focus = Some(TopPage::Apps);
        view.sidebar_edit_mode = true;
        view.history_runtime.request(true);
        view.processes_state.hidden_cols.insert(SortCol::User);
        let (_sender, receiver) = std::sync::mpsc::channel();
        view.instance_rx = Some(receiver);
        let telemetry_owner = std::sync::Arc::as_ptr(&view.telemetry);
        let fingerprint_before = view.presentation_fingerprint();

        view.shell
            .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Import {
                rules: vec![taskmanager_application::ManagedAlertRule::new(rule, true)],
                mode: taskmanager_application::AlertRuleImportMode::Replace,
            })
            .unwrap();
        view.shell.set_alert_policy(NotificationPolicy {
            enabled: true,
            cooldown_ms: 0,
            ..NotificationPolicy::default()
        });
        assert_eq!(
            view.shell
                .evaluate_alerts(&snapshot, 1_000)
                .notifications
                .len(),
            1
        );

        apply_root_runtime_config(view, &config, cx);

        assert_eq!(view.page, TopPage::Apps);
        assert_eq!(view.pending_search_focus, Some(TopPage::Apps));
        assert!(view.sidebar_edit_mode);
        assert!(view.history_runtime.enabled_next_start());
        assert!(view.history_runtime.unavailable_reason().is_some());
        assert!(view.instance_rx.is_some());
        assert_eq!(std::sync::Arc::as_ptr(&view.telemetry), telemetry_owner);
        let presentation = view.presentation_snapshot();
        let fingerprint_after = presentation.fingerprint();
        assert!(fingerprint_after.appearance() > fingerprint_before.appearance());
        assert!(fingerprint_after.devices() > fingerprint_before.devices());
        assert!(fingerprint_after.units() > fingerprint_before.units());
        assert!(fingerprint_after.graphs() > fingerprint_before.graphs());
        assert!(fingerprint_after.sidebar() > fingerprint_before.sidebar());
        assert!(fingerprint_after.apps() > fingerprint_before.apps());
        assert_eq!(presentation.language(), Some(crate::i18n::Language::Zh));
        assert_eq!(
            super::super::super::persistence::config_from_view(view)
                .language
                .as_deref(),
            Some("zh")
        );
        assert!(
            view.processes_state.hidden_cols.is_empty(),
            "an explicitly configured empty set must show every column"
        );

        let repeat = view.shell.evaluate_alerts(&snapshot, 2_000);
        assert_eq!(repeat.active.len(), 1);
        assert!(
            repeat.notifications.is_empty(),
            "policy updates must preserve active/delivery history"
        );
    });
}
