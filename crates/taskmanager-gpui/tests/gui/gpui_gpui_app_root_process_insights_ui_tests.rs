use super::{ProcessDetailsSection, RootView};
use super::{ProcessInsightsErrorKind, ProcessInsightsLifecycle, ProcessInsightsRenderState};
use gpui::{AppContext, TestAppContext};
use taskmanager_application::ProjectedProcessInsights;
use taskmanager_application::{
    ProcessInsightFacetState, ProcessInsightUnavailable, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::ScalarObservation;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_shell::fixture::process_insights::process_insights_projection;
use taskmanager_theme::Theme;

fn target(pid: u32, start_token: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, format!("process-{pid}"), 10, start_token)
        .expect("fixture identity is authoritative")
}

fn terminal_failure(
    target: FrozenProcessIdentity,
    revision: ProcessInsightsRevision,
) -> ProjectedProcessInsights {
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target, revision);
    let mut projection = tracker.snapshot().expect("begin publishes a projection");
    let unavailable = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::PermissionDenied,
    ));
    projection.network = unavailable;
    projection.gpu = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::PermissionDenied,
    ));
    projection.resources = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.isolation = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.threads = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.open_files = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection.environment = ProcessInsightFacetState::Unavailable(
        ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
    );
    projection
}

#[test]
fn lifecycle_rejects_wrong_target_late_and_duplicate_terminals() {
    let first = target(42, 100);
    let reused_pid = target(42, 200);
    let first_revision = ProcessInsightsRevision::new(7);
    let next_revision = ProcessInsightsRevision::new(8);
    let first_terminal = terminal_failure(first.clone(), first_revision);
    let next_terminal = terminal_failure(reused_pid.clone(), next_revision);
    let mut lifecycle = ProcessInsightsLifecycle::default();

    lifecycle.begin(reused_pid.clone(), next_revision);
    assert!(
        !lifecycle.apply(first_terminal.clone()),
        "same PID with an older provider-native identity is not the active request"
    );
    assert!(matches!(
        lifecycle,
        ProcessInsightsLifecycle::Loading { ref request }
            if request.target == reused_pid && request.revision == next_revision
    ));

    assert!(lifecycle.apply(next_terminal.clone()));
    assert!(matches!(
        lifecycle,
        ProcessInsightsLifecycle::Showing { ref projection, .. }
            if projection.target.live_key() == ProcessLiveKey::from_parts(42, 200)
                && projection.network == ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied))
    ));
    assert!(
        !lifecycle.apply(next_terminal),
        "a terminal phase cannot consume a duplicate completion"
    );
    assert!(
        !lifecycle.apply(first_terminal),
        "an older terminal cannot resurrect after the current request completed"
    );
}

#[test]
fn later_optional_facets_update_the_same_frozen_properties_request() {
    let target = target(42, 100);
    let complete =
        process_insights_projection(target.clone()).expect("fixture has all seven facets");
    let mut initial = complete.clone();
    initial.threads = ProcessInsightFacetState::Pending;
    initial.open_files = ProcessInsightFacetState::Pending;
    initial.environment = ProcessInsightFacetState::Pending;
    let mut lifecycle = ProcessInsightsLifecycle::default();
    lifecycle.begin(target, complete.revision);
    assert!(lifecycle.apply(initial));
    assert!(matches!(
        &lifecycle,
        ProcessInsightsLifecycle::Showing { projection, .. } if projection.is_collecting()
    ));
    assert!(
        lifecycle.apply(complete.clone()),
        "bounded enrichment must update an already painted request"
    );
    let ProcessInsightsLifecycle::Showing { projection, .. } = &lifecycle else {
        panic!("the matching enrichment preserves ready properties");
    };
    let ProcessInsightFacetState::Current(threads) = &projection.threads else {
        panic!("threads arrive");
    };
    let ProcessInsightFacetState::Current(files) = &projection.open_files else {
        panic!("files arrive");
    };
    let ProcessInsightFacetState::Current(environment) = &projection.environment else {
        panic!("environment arrives");
    };
    assert_eq!(threads.threads.len(), 2);
    assert_eq!(files.entries.len(), 4);
    assert_eq!(environment.entries.len(), 3);
    assert!(
        !lifecycle.apply(complete),
        "an identical projection does not repaint"
    );
}

#[test]
fn partial_current_and_distinct_unavailable_reasons_are_not_flattened() {
    let target = target(42, 100);
    let mut partial = process_insights_projection(target.clone()).expect("fixture projection");
    partial.gpu = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::Unsupported,
    ));
    partial.resources = ProcessInsightFacetState::Pending;
    partial.isolation = ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
        FailureKind::PermissionDenied,
    ));
    let mut lifecycle = ProcessInsightsLifecycle::default();
    lifecycle.begin(target, partial.revision);
    assert!(lifecycle.apply(partial));
    let ProcessInsightsRenderState::Projection(rendered) = lifecycle.render_state() else {
        panic!("one pending facet cannot hide another facet's current facts");
    };
    assert!(
        matches!(&rendered.network, ProcessInsightFacetState::Current(network) if !network.connections.is_empty())
    );
    assert_eq!(
        rendered.gpu,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::Unsupported
        ))
    );
    assert_eq!(
        rendered.isolation,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied
        ))
    );
    assert!(matches!(
        rendered.resources,
        ProcessInsightFacetState::Pending
    ));
}

#[gpui::test]
fn refresh_keeps_original_metadata_and_reused_pid_cannot_publish(cx: &mut TestAppContext) {
    let entity = cx.new(|cx| RootView::new(Theme::dark(), cx));
    entity.update(cx, |view, _| {
        let mut process = ProcessItem::new(42, "original");
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(100, 1),
            ..Default::default()
        });
        view.replace_processes_for_test(vec![process.clone()]);
        let identity = ProcessLiveKey::from_parts(42, 100).expect("key");
        view.open_process_details(identity, ProcessDetailsSection::Insights);
        let frozen = view
            .process_properties_target()
            .expect("properties")
            .clone();
        let projection = process_insights_projection(frozen.clone()).expect("projection");
        view.process_insights
            .begin(frozen.clone(), projection.revision);
        let mut pending = projection.clone();
        pending.environment = ProcessInsightFacetState::Pending;
        assert!(view.apply_process_insights_projection(pending));
        assert!(
            !view.refresh_process_insights(),
            "an active enrichment keeps its original request"
        );
        assert!(view.apply_process_insights_projection(projection.clone()));
        process.name = "renamed".to_owned();
        view.replace_processes_for_test(vec![process.clone()]);
        assert!(view.refresh_process_insights());
        assert_eq!(view.process_insights.target(), Some(&frozen));
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(200, 2),
            ..Default::default()
        });
        view.replace_processes_for_test(vec![process]);
        view.process_insights
            .begin(frozen.clone(), projection.revision);
        assert!(view.apply_process_insights_projection(projection));
        assert!(
            matches!(&view.process_insights, ProcessInsightsLifecycle::Failed { target, error }
            if target == &frozen && error.kind == ProcessInsightsErrorKind::ProcessUnavailable)
        );
    });
}
