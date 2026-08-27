use super::*;
use taskmanager_application::ServiceControlOutcome;

#[test]
fn plain_click_collapses_to_the_single_anchor() {
    let mut selection = ProcessSelection::default();
    selection.select_single(10);
    selection.toggle(11);
    selection.select_single(12);
    assert_eq!(selection.pids(), &HashSet::from([12]));
    assert_eq!(selection.anchor(), Some(12));
    assert_eq!(selection.active_row(), Some(ProcessRowKey::Process(12)));
}

#[test]
fn application_aggregate_selection_has_no_representative_pid() {
    let mut selection = ProcessSelection::default();
    selection.select_single(10);
    selection.select_application(42);
    assert!(selection.pids().is_empty());
    assert_eq!(selection.anchor(), None);
    assert_eq!(selection.active_row(), Some(ProcessRowKey::Application(42)));
    assert_eq!(selection.application_root(), Some(42));
}

#[test]
fn ctrl_toggle_flips_membership_and_tracks_the_anchor() {
    let mut selection = ProcessSelection::default();
    selection.select_single(10);
    selection.toggle(11);
    assert_eq!(selection.pids(), &HashSet::from([10, 11]));
    assert_eq!(selection.anchor(), Some(11));
    selection.toggle(11);
    assert_eq!(selection.pids(), &HashSet::from([10]));
    assert_eq!(selection.anchor(), Some(10));
}

#[test]
fn shift_click_spans_the_display_order_between_anchor_and_end() {
    let display = vec![5, 6, 7, 8, 9];
    let mut selection = ProcessSelection::default();
    selection.select_single(6);
    selection.extend_range(&display, 8);
    assert_eq!(selection.pids(), &HashSet::from([6, 7, 8]));
    assert_eq!(selection.anchor(), Some(8));

    // Reverse direction spans the same members.
    selection.extend_range(&display, 5);
    assert_eq!(selection.pids(), &HashSet::from([5, 6, 7, 8]));
    assert_eq!(selection.anchor(), Some(5));
}

#[test]
fn a_stale_range_endpoint_inserts_nothing() {
    let display = vec![5, 6, 7];
    let mut selection = ProcessSelection::default();
    selection.select_single(6);
    selection.extend_range(&display, u32::MAX);
    assert_eq!(selection.pids(), &HashSet::from([6]));
    assert_eq!(selection.anchor(), Some(u32::MAX));
}

#[test]
fn keyboard_navigation_collapses_unless_preserving() {
    let mut selection = ProcessSelection::default();
    selection.select_single(6);
    selection.move_to(Some(7), false);
    assert_eq!(selection.pids(), &HashSet::from([7]));
    selection.select_single(6);
    selection.move_to(Some(7), true);
    assert_eq!(selection.pids(), &HashSet::from([6, 7]));
    selection.move_to(None, true);
    assert_eq!(selection.anchor(), None);
    assert_eq!(selection.pids(), &HashSet::from([6, 7]));
}

#[test]
fn pruning_drops_dead_pids_and_clears_a_dead_anchor() {
    let mut selection = ProcessSelection::default();
    selection.select_single(6);
    selection.toggle(7);
    selection.retain_live(&HashSet::from([6]));
    assert_eq!(selection.pids(), &HashSet::from([6]));
    assert_eq!(
        selection.anchor(),
        None,
        "a dead anchor clears, never jumps"
    );
}

#[test]
fn batch_targets_prefer_the_sorted_set_and_fall_back_to_the_anchor() {
    let mut selection = ProcessSelection::default();
    assert!(selection.batch_targets().is_empty());
    selection.select_single(9);
    assert_eq!(selection.batch_targets(), vec![9]);
    selection.toggle(3);
    selection.toggle(6);
    assert_eq!(selection.batch_targets(), vec![3, 6, 9]);
}

#[test]
fn pid_range_handles_reversed_and_missing_endpoints() {
    let display = vec![4, 5, 6];
    assert_eq!(pid_range(&display, 6, 4), vec![4, 5, 6]);
    assert_eq!(pid_range(&display, 4, 99), Vec::<u32>::new());
    assert_eq!(pid_range(&display, 99, 4), Vec::<u32>::new());
}

/// The process-table sort reducers carry the header-click conventions every
/// direct-track frontend's clickable headers rely on: same-column flip,
/// text-like columns start ascending / numeric columns start descending,
/// arrow-key column moves preserve the direction, and the absolute setter
/// (persistence / saved-view restore) overrides both components verbatim.
#[test]
fn process_viewing_sort_reducers_match_the_header_click_conventions() {
    let mut viewing = ProcessViewing::default();
    assert_eq!(viewing.sort(), (SortCol::Cpu, SortDir::Desc));
    viewing.click_sort_column(SortCol::Cpu);
    assert_eq!(viewing.sort(), (SortCol::Cpu, SortDir::Asc));
    viewing.click_sort_column(SortCol::Name);
    assert_eq!(viewing.sort(), (SortCol::Name, SortDir::Asc));
    viewing.click_sort_column(SortCol::Fds);
    assert_eq!(viewing.sort(), (SortCol::Fds, SortDir::Desc));
    viewing.move_sort_column(SortCol::User);
    assert_eq!(viewing.sort(), (SortCol::User, SortDir::Desc));
    viewing.set_sort(SortCol::StartTime, SortDir::Asc);
    assert_eq!(viewing.sort(), (SortCol::StartTime, SortDir::Asc));
}

#[test]
fn process_viewing_holds_the_query_and_status_bucket() {
    let mut viewing = ProcessViewing::default();
    assert_eq!(viewing.query(), "");
    assert_eq!(viewing.status_filter(), ProcessStatusFilter::All);
    viewing.set_query("user:root ");
    viewing.set_status_filter(ProcessStatusFilter::Zombie);
    assert_eq!(viewing.query(), "user:root ");
    assert_eq!(viewing.status_filter(), ProcessStatusFilter::Zombie);
}

#[test]
fn sorts_click_matches_the_shell_track_toggle_rule() {
    let mut sorts = InventorySorts::default();
    assert_eq!(sorts.active(InfoTable::Services), None);
    sorts.click(InfoTable::Services, InfoSortCol::Status);
    assert_eq!(
        sorts.active(InfoTable::Services),
        Some((InfoSortCol::Status, SortDir::Asc))
    );
    sorts.click(InfoTable::Services, InfoSortCol::Status);
    assert_eq!(
        sorts.active(InfoTable::Services),
        Some((InfoSortCol::Status, SortDir::Desc))
    );
    // A different column switches directly to ascending; other tables
    // keep their own slot.
    sorts.click(InfoTable::Services, InfoSortCol::Name);
    assert_eq!(
        sorts.active(InfoTable::Services),
        Some((InfoSortCol::Name, SortDir::Asc))
    );
    assert_eq!(sorts.active(InfoTable::Startup), None);
}

#[test]
fn sorts_apply_absolute_widget_states_including_provider_order() {
    let mut sorts = InventorySorts::default();
    sorts.set(
        InfoTable::Users,
        Some((InfoSortCol::Session, SortDir::Desc)),
    );
    assert_eq!(
        sorts.active(InfoTable::Users),
        Some((InfoSortCol::Session, SortDir::Desc))
    );
    sorts.set(InfoTable::Users, None);
    assert_eq!(sorts.active(InfoTable::Users), None);
}

fn service(name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory("", name, status, "", "", "", "")
}

#[test]
fn services_order_matches_the_shell_track_semantics() {
    let mut rows = vec![
        service("docker.service", ServiceStatus::Failed),
        service("zed.service", ServiceStatus::Inactive),
        service("apparmor.service", ServiceStatus::Active),
    ];
    order_service_rows(&mut rows, Some((InfoSortCol::Status, SortDir::Asc)));
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["apparmor.service", "zed.service", "docker.service"]
    );

    order_service_rows(&mut rows, Some((InfoSortCol::Name, SortDir::Asc)));
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["apparmor.service", "docker.service", "zed.service"]
    );

    order_service_rows(&mut rows, None);
    let statuses: Vec<ServiceStatus> = rows.iter().map(|row| row.status).collect();
    // `None` re-orders nothing; the rows keep the previous (name-asc)
    // order the caller passed in.
    assert_eq!(
        statuses,
        vec![
            ServiceStatus::Active,
            ServiceStatus::Failed,
            ServiceStatus::Inactive
        ]
    );
}

#[test]
fn startup_order_is_enabled_first_under_ascending_status() {
    use taskmanager_application::{
        StartupControlPolicy, StartupEntryId, StartupEntryLocator, StartupImpact,
        StartupImpactEvidence, StartupImpactUnknownReason, StartupScope, StartupSource,
    };

    let entry = |name: &str, enabled: bool| StartupEntry {
        id: StartupEntryId::new(name),
        name: name.to_owned(),
        exec: name.to_owned(),
        enabled,
        source: StartupSource::UserService,
        scope: StartupScope::User,
        control_policy: StartupControlPolicy::Direct,
        locator: StartupEntryLocator::new(name),
        impact: StartupImpact::None,
        impact_evidence: StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented,
        },
    };
    let mut rows = vec![entry("zeta", true), entry("alpha", false)];
    order_startup_rows(&mut rows, Some((InfoSortCol::Status, SortDir::Asc)));
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, vec!["zeta", "alpha"]);
    order_startup_rows(&mut rows, Some((InfoSortCol::Status, SortDir::Desc)));
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "zeta"]);
}

#[test]
fn session_order_covers_user_session_and_seat() {
    let session = |id: &str, user: &str, seat: &str| SessionItem {
        id: id.to_owned(),
        user: user.to_owned(),
        seat: Some(seat.to_owned()),
        ..SessionItem::default()
    };
    let mut rows = vec![
        session("3", "alice", "seat1"),
        session("1", "root", "seat0"),
    ];
    order_session_rows(&mut rows, Some((InfoSortCol::Name, SortDir::Asc)));
    assert_eq!(rows[0].user, "alice");
    order_session_rows(&mut rows, Some((InfoSortCol::Session, SortDir::Asc)));
    assert_eq!(rows[0].id, "1");
    order_session_rows(&mut rows, Some((InfoSortCol::Seat, SortDir::Desc)));
    assert_eq!(rows[0].seat.as_deref(), Some("seat1"));
}

#[test]
fn feedback_slots_are_latest_wins() {
    use taskmanager_application::{FailureKind, LatestControlRequest, ServiceAction, ServiceId};

    let mut feedback = crate::FeedbackState::default();
    assert!(feedback.service().is_none());
    let request_id = LatestControlRequest::default().begin();
    let rejected = ServiceControlOutcome {
        request_id,
        service_id: ServiceId::new("a.service".to_owned()),
        action: ServiceAction::Start,
        result: Err(FailureKind::Rejected),
    };
    feedback.record_service(rejected.clone());
    assert_eq!(feedback.service(), Some(&rejected));
    let accepted = ServiceControlOutcome {
        result: Ok(()),
        ..rejected.clone()
    };
    feedback.record_service(accepted.clone());
    assert_eq!(feedback.service(), Some(&accepted));
}

#[test]
fn direct_track_uses_one_state_for_inventory_outcomes_and_runtime_notices() {
    let mut state = DirectTrackState::default();
    let request_id = taskmanager_application::LatestControlRequest::default().begin();
    let service = ServiceControlOutcome {
        request_id,
        service_id: taskmanager_application::ServiceId::new("demo.service"),
        action: taskmanager_application::ServiceAction::Start,
        result: Ok(()),
    };
    state.feedback.record_service(service.clone());
    state.report_notice(
        crate::app::FeedbackSource::Persistence,
        crate::app::FeedbackSeverity::Warning,
        crate::app::FeedbackLifecycle::UntilReplaced,
        "history writer degraded",
    );
    assert_eq!(state.feedback.service(), Some(&service));
    let notice = state.feedback_notice().expect("typed direct-track notice");
    assert_eq!(notice.source(), crate::app::FeedbackSource::Persistence);
    assert_eq!(notice.severity(), crate::app::FeedbackSeverity::Warning);
    assert_eq!(
        notice.lifecycle(),
        crate::app::FeedbackLifecycle::UntilReplaced
    );
    assert_eq!(notice.text(), "history writer degraded");
}
