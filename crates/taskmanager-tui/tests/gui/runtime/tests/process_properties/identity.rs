//! Frozen target, live facts, complete facet tails and normal keyboard controls.
use super::*;
use taskmanager_application::{PlatformEffect, ProcessInsightFacet, ProcessInsightFacetState};
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::process_insights::{
    process_insights_projection, seed_process_properties_history,
};
fn key(app: &mut TuiApp, code: ratatui::crossterm::event::KeyCode) -> Option<PlatformEffect> {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE))
}
#[test]
fn normal_properties_keep_frozen_identity_and_live_facts_across_filtering_and_pid_reuse() {
    use ratatui::crossterm::event::KeyCode;
    let mut app = app_on_processes();
    seed_process_properties_history(&mut app.shell);
    app.reconcile_applications_cursor();
    let _ = key(&mut app, KeyCode::Enter);
    let target = app
        .shell
        .process_properties_target()
        .expect("target")
        .clone();
    let identity = target.live_key().expect("key");
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(process_insights_projection(target.clone()))),
    );
    app.shell.query = "hidden by filter".to_owned();
    edit_processes(&mut app.shell, |rows| {
        let current = rows
            .as_mut()
            .expect("rows")
            .iter_mut()
            .find(|p| ProcessLiveKey::from_process(p) == Some(identity))
            .expect("current");
        let mut scalars = *current.scalar_observations();
        scalars.cpu_percentage = ScalarObservation::available(63.0, 43_000);
        current.apply_scalar_observations(scalars);
    });
    let _ = key(&mut app, KeyCode::Tab);
    assert!(
        frame_text(&app, 140, 48).contains("63.0%"),
        "accepted live values replace the open-time row cache"
    );
    let _ = key(&mut app, KeyCode::Tab);
    let _ = key(&mut app, KeyCode::Tab);
    for (number, facet, observed) in [
        ('1', ProcessInsightFacet::Network, "12.4 ms"),
        ('2', ProcessInsightFacet::Gpu, "768.0 MiB"),
        ('3', ProcessInsightFacet::Resources, "384.0 MiB"),
        ('4', ProcessInsightFacet::Isolation, "Seccomp"),
    ] {
        let _ = key(&mut app, KeyCode::Char(number));
        assert_eq!(app.process_properties().expect("view").facet, facet);
        assert!(
            frame_text(&app, 140, 48).contains(observed),
            "{facet:?} normal selector"
        );
    }
    let Some(effect) = key(&mut app, KeyCode::Char('r')) else {
        panic!("refresh effect")
    };
    assert_eq!(effect, PlatformEffect::ProcessInsights(target.clone()));
    edit_processes(&mut app.shell, |rows| {
        let current = rows
            .as_mut()
            .expect("rows")
            .iter_mut()
            .find(|p| p.pid == target.pid)
            .expect("pid");
        let mut scalars = *current.scalar_observations();
        scalars.start_token = ScalarObservation::available(identity.start_token() + 1, 44_000);
        current.name = "replacement".to_owned();
        current.apply_scalar_observations(scalars);
    });
    assert!(frame_text(&app, 140, 48).contains("no longer running"));
    assert!(key(&mut app, KeyCode::Char('r')).is_none());
}
#[test]
fn complete_thread_tail_remains_reachable_and_partial_network_authorization_is_identity_bound() {
    use ratatui::crossterm::event::KeyCode;
    let mut app = app_on_processes();
    let _ = key(&mut app, KeyCode::Enter);
    let target = app
        .shell
        .process_properties_target()
        .expect("target")
        .clone();
    let mut projection = process_insights_projection(target.clone()).expect("projection");
    let ProcessInsightFacetState::Current(threads) = &mut projection.threads else {
        panic!("threads")
    };
    for index in 2..80 {
        let mut row = threads.threads[0].clone();
        row.comm = format!("tail-worker-{index}");
        row.tid = 5000 + index;
        threads.threads.push(row);
    }
    let ProcessInsightFacetState::Current(network) = &mut projection.network else {
        panic!("network")
    };
    network.traffic_failure = Some(FailureKind::RequiresEscalation);
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
    );
    for _ in 0..3 {
        let _ = key(&mut app, KeyCode::Tab);
    }
    let _ = key(&mut app, KeyCode::Char('5'));
    app.process_properties_mut().expect("view").scroll = usize::MAX;
    assert!(frame_text(&app, 96, 20).contains("tail-worker-79"));
    let _ = key(&mut app, KeyCode::Char('1'));
    assert!(frame_text(&app, 120, 36).contains("Enable per-process network"));
    let Some(effect) = key(&mut app, KeyCode::Char('e')) else {
        panic!("authorize")
    };
    assert_eq!(effect, ShellApp::request_process_network_escalation());
    let wrong = ProcessLiveKey::new(
        target.pid,
        target.authoritative_start_token().expect("token") + 1,
    )
    .expect("wrong");
    assert!(!crate::ui::process_details::network_requires_escalation(
        &app, wrong
    ));
}
