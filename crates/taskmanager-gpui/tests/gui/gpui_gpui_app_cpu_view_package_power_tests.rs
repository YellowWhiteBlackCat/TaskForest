use super::*;
use taskmanager_application::{RaplPowerRequestFailure, RaplPowerSession, RaplPowerState};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{RaplPackageRow, RaplPowerSnapshot};
use taskmanager_platform_contract::{CapabilityStatus, RequestId};

fn inputs<'a>(
    state: &'a RaplPowerState,
    capability: Option<CapabilityStatus>,
) -> PackagePowerInputs<'a> {
    PackagePowerInputs { state, capability }
}

/// Drive the real session through the submission path so the projections
/// below observe the same states production admission produces.
fn accept_success(session: &mut RaplPowerSession, request: u64, snapshot: RaplPowerSnapshot) {
    let attempt = session.begin_attempt();
    let request_id = RequestId::new(request).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request_id));
    assert!(session.complete(request_id, snapshot));
}

fn accept_failure(session: &mut RaplPowerSession, request: u64, kind: FailureKind) {
    accept_success(
        session,
        request,
        RaplPowerSnapshot::failed(kind, "fixture detail"),
    );
}

fn watts(packages: &[(f32, &str)]) -> RaplPowerSnapshot {
    RaplPowerSnapshot::success(
        200,
        packages
            .iter()
            .map(|&(power_w, name)| RaplPackageRow {
                name: name.to_owned(),
                power_w,
                energy_delta_uj: 2_500_000,
            })
            .collect(),
    )
}

/// Ready renders one row per real package reading, watts at one decimal in
/// the live readout's shared spelling.
#[test]
fn ready_session_projects_per_package_watt_rows() {
    let mut session = RaplPowerSession::default();
    accept_success(
        &mut session,
        1,
        watts(&[(12.5, "package-0"), (8.0, "package-1")]),
    );
    let model = package_power_model(&inputs(session.state(), Some(CapabilityStatus::Available)));
    match model {
        PackagePowerModel::Packages(rows) => {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].0, "package-0");
            assert_eq!(rows[0].1, "12.5 W");
            assert_eq!(rows[1].0, "package-1");
            assert_eq!(rows[1].1, "8.0 W");
        }
        other => panic!("accepted payload must project rows, got {other:?}"),
    }
}

/// A refresh keeps the last accepted rows visible: real measured data, not a
/// pending placeholder over known facts.
#[test]
fn loading_keeps_the_last_accepted_rows() {
    let mut session = RaplPowerSession::default();
    accept_success(&mut session, 1, watts(&[(12.5, "package-0")]));
    let _ = session.begin_attempt();
    assert!(matches!(
        package_power_model(&inputs(session.state(), Some(CapabilityStatus::Available))),
        PackagePowerModel::Packages(rows) if rows.len() == 1
    ));

    let mut fresh = RaplPowerSession::default();
    let _ = fresh.begin_attempt();
    assert_eq!(
        package_power_model(&inputs(fresh.state(), Some(CapabilityStatus::Available))),
        PackagePowerModel::Measuring,
        "a first request with no accepted payload is the pending row"
    );
}

/// RequiresEscalation is the affordance state: no numeric row may render and
/// the projection stays distinguishable from a typed failure.
#[test]
fn requires_escalation_projects_the_affordance_not_a_number() {
    let mut session = RaplPowerSession::default();
    accept_failure(&mut session, 1, FailureKind::RequiresEscalation);
    let model = package_power_model(&inputs(session.state(), Some(CapabilityStatus::Available)));
    assert_eq!(model, PackagePowerModel::AuthorizationRequired);
    assert!(
        !matches!(model, PackagePowerModel::Packages(_)),
        "an escalation gap must never carry a fabricated watt row"
    );
}

/// Other failure kinds keep their typed labels; none of them is a number.
#[test]
fn other_failures_project_typed_unavailable_labels() {
    for (kind, key) in [
        (FailureKind::PermissionDenied, "cpu.package_power_denied"),
        (FailureKind::MissingDependency, "cpu.package_power_helper"),
        (FailureKind::Unsupported, "cpu.package_power_unsupported"),
        (FailureKind::TimedOut, "cpu.package_power_unavailable"),
        (
            FailureKind::TemporarilyUnavailable,
            "cpu.package_power_unavailable",
        ),
        (FailureKind::ProviderFault, "cpu.package_power_unavailable"),
    ] {
        let mut session = RaplPowerSession::default();
        accept_failure(&mut session, 1, kind);
        assert_eq!(
            package_power_model(&inputs(session.state(), Some(CapabilityStatus::Available))),
            PackagePowerModel::Unavailable(key),
            "failure kind {kind:?}"
        );
    }
}

/// Closed renders nothing while no lane exists; a registered escalation
/// lane is the single authorize entry.
#[test]
fn closed_session_renders_nothing_without_a_lane() {
    let closed = RaplPowerState::Closed;
    assert_eq!(
        package_power_model(&inputs(&closed, None)),
        PackagePowerModel::Hidden,
        "no registered capability → no section at all"
    );
    assert_eq!(
        package_power_model(&inputs(&closed, Some(CapabilityStatus::Unsupported))),
        PackagePowerModel::Hidden
    );
    assert_eq!(
        package_power_model(&inputs(&closed, Some(CapabilityStatus::Available))),
        PackagePowerModel::AuthorizationRequired,
        "a registered lane offers the explicit authorize entry"
    );
    assert_eq!(
        package_power_model(&inputs(&closed, Some(CapabilityStatus::PermissionRequired))),
        PackagePowerModel::AuthorizationRequired
    );
}

/// A runtime without a platform client resolves the click into the honest
/// typed failure (not a hang), proving the affordance submits exactly one
/// request through the session.
#[gpui::test]
async fn authorize_affordance_submits_one_request(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(taskmanager_theme::Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let attempt = view.shell.begin_rapl_power_request();
        view.shell
            .reject_rapl_power_request(attempt, FailureKind::RequiresEscalation);
        view.authorize_package_power(cx);
        match view.shell.rapl_power_state() {
            RaplPowerState::Failed(failed) => assert_eq!(
                failed.failure,
                RaplPowerRequestFailure::Submission(FailureKind::TemporarilyUnavailable),
                "the click must submit; the absent runtime rejects honestly"
            ),
            other => panic!("authorize must leave a terminal state, got {other:?}"),
        }
    })
    .unwrap();
}

/// The handler is gated on the authorize projection: a click while a request
/// is already in flight must not submit a second one.
#[gpui::test]
async fn authorize_affordance_is_gated_on_the_projection(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(taskmanager_theme::Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let _ = view.shell.begin_rapl_power_request();
        view.authorize_package_power(cx);
        assert!(
            matches!(
                view.shell.rapl_power_state(),
                RaplPowerState::Loading { .. }
            ),
            "a non-authorize projection must not submit"
        );
    })
    .unwrap();
}
