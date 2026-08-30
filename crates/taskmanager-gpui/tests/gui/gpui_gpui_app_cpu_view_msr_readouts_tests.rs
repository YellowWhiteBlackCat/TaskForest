use super::*;
use taskmanager_application::{MsrReadoutRequestFailure, MsrReadoutSession, MsrReadoutState};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{MsrPackageReadout, MsrReadoutSnapshot};
use taskmanager_platform_contract::{CapabilityStatus, RequestId};

fn inputs<'a>(
    state: &'a MsrReadoutState,
    capability: Option<CapabilityStatus>,
) -> MsrReadoutsInputs<'a> {
    MsrReadoutsInputs { state, capability }
}

/// Drive the real session through the submission path so the projections
/// below observe the same states production admission produces.
fn accept_success(session: &mut MsrReadoutSession, request: u64, snapshot: MsrReadoutSnapshot) {
    let attempt = session.begin_attempt();
    let request_id = RequestId::new(request).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request_id));
    assert!(session.complete(request_id, snapshot));
}

fn accept_failure(session: &mut MsrReadoutSession, request: u64, kind: FailureKind) {
    accept_success(
        session,
        request,
        MsrReadoutSnapshot::failed(kind, "fixture detail"),
    );
}

fn readout(cpu: u32, temperature: Option<f32>, multiplier: Option<f32>) -> MsrPackageReadout {
    MsrPackageReadout {
        cpu,
        bclk_mhz: None,
        temperature_c: temperature,
        multiplier,
        multiplier_min: Some(8.0),
        multiplier_max: Some(58.0),
        vcore_v: Some(1.219),
    }
}

/// Ready renders one row per real register fact in the shared spellings:
/// temperature one decimal °C, multipliers `×NN.N`, volts three decimals.
/// A register the CPU does not implement renders no row — never a dash slot.
#[test]
fn ready_session_projects_per_node_fact_rows() {
    let mut session = MsrReadoutSession::default();
    accept_success(
        &mut session,
        1,
        MsrReadoutSnapshot::success(vec![
            readout(0, Some(54.5), Some(42.0)),
            readout(1, None, None),
        ]),
    );
    let model = msr_readouts_model(&inputs(session.state(), Some(CapabilityStatus::Available)));
    match model {
        MsrReadoutsModel::Rows(rows) => {
            // Node 0: temperature + multiplier + min + max + vcore = 5 rows.
            // Node 1 implements only the ratio bounds + vcore = 3 rows.
            assert_eq!(rows.len(), 8);
            assert_eq!(rows[0].1, "54.5 °C");
            assert_eq!(rows[1].1, "\u{00d7}42.0");
            assert_eq!(rows[2].1, "\u{00d7}8.0");
            assert_eq!(rows[3].1, "\u{00d7}58.0");
            assert_eq!(rows[4].1, "1.219 V");
            assert!(
                !rows.iter().any(|(label, _)| label.starts_with("CPU 1")
                    && label.ends_with(i18n::t("cpu.msr_temperature"))),
                "an unimplemented register is an absent row, never a dash slot"
            );
        }
        other => panic!("accepted payload must project rows, got {other:?}"),
    }
}

/// A refresh keeps the last accepted rows visible: real measured data, not a
/// pending placeholder over known facts.
#[test]
fn loading_keeps_the_last_accepted_rows() {
    let mut session = MsrReadoutSession::default();
    accept_success(
        &mut session,
        1,
        MsrReadoutSnapshot::success(vec![readout(0, Some(54.5), Some(42.0))]),
    );
    let _ = session.begin_attempt();
    assert!(matches!(
        msr_readouts_model(&inputs(session.state(), Some(CapabilityStatus::Available))),
        MsrReadoutsModel::Rows(rows) if rows.len() == 5
    ));

    let mut fresh = MsrReadoutSession::default();
    let _ = fresh.begin_attempt();
    assert_eq!(
        msr_readouts_model(&inputs(fresh.state(), Some(CapabilityStatus::Available))),
        MsrReadoutsModel::Measuring,
        "a first request with no accepted payload is the pending row"
    );
}

/// RequiresEscalation is the affordance state: no numeric row may render and
/// the projection stays distinguishable from a typed failure.
#[test]
fn requires_escalation_projects_the_affordance_not_a_number() {
    let mut session = MsrReadoutSession::default();
    accept_failure(&mut session, 1, FailureKind::RequiresEscalation);
    let model = msr_readouts_model(&inputs(session.state(), Some(CapabilityStatus::Available)));
    assert_eq!(model, MsrReadoutsModel::AuthorizationRequired);
    assert!(
        !matches!(model, MsrReadoutsModel::Rows(_)),
        "an escalation gap must never carry a fabricated register row"
    );
}

/// Other failure kinds keep their typed labels; none of them is a number.
#[test]
fn other_failures_project_typed_unavailable_labels() {
    for (kind, key) in [
        (FailureKind::PermissionDenied, "cpu.msr_readouts_denied"),
        (FailureKind::MissingDependency, "cpu.msr_readouts_helper"),
        (FailureKind::Unsupported, "cpu.msr_readouts_unsupported"),
        (FailureKind::TimedOut, "cpu.msr_readouts_unavailable"),
        (
            FailureKind::TemporarilyUnavailable,
            "cpu.msr_readouts_unavailable",
        ),
        (FailureKind::ProviderFault, "cpu.msr_readouts_unavailable"),
    ] {
        let mut session = MsrReadoutSession::default();
        accept_failure(&mut session, 1, kind);
        assert_eq!(
            msr_readouts_model(&inputs(session.state(), Some(CapabilityStatus::Available))),
            MsrReadoutsModel::Unavailable(key),
            "failure kind {kind:?}"
        );
    }
}

/// Closed renders nothing while no lane exists; a registered escalation
/// lane is the single authorize entry.
#[test]
fn closed_session_renders_nothing_without_a_lane() {
    let closed = MsrReadoutState::Closed;
    assert_eq!(
        msr_readouts_model(&inputs(&closed, None)),
        MsrReadoutsModel::Hidden,
        "no registered capability → no section at all"
    );
    assert_eq!(
        msr_readouts_model(&inputs(&closed, Some(CapabilityStatus::Unsupported))),
        MsrReadoutsModel::Hidden
    );
    assert_eq!(
        msr_readouts_model(&inputs(&closed, Some(CapabilityStatus::Available))),
        MsrReadoutsModel::AuthorizationRequired,
        "a registered lane offers the explicit authorize entry"
    );
    assert_eq!(
        msr_readouts_model(&inputs(&closed, Some(CapabilityStatus::PermissionRequired))),
        MsrReadoutsModel::AuthorizationRequired
    );
}

/// A runtime without a platform client resolves the click into the honest
/// typed failure (not a hang), proving the affordance submits exactly one
/// request through the session.
#[gpui::test]
async fn authorize_affordance_submits_one_request(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(taskmanager_theme::Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        let attempt = view.shell.begin_msr_readout_request();
        view.shell
            .reject_msr_readout_request(attempt, FailureKind::RequiresEscalation);
        view.authorize_msr_readouts(cx);
        match view.shell.msr_readout_state() {
            MsrReadoutState::Failed(failed) => assert_eq!(
                failed.failure,
                MsrReadoutRequestFailure::Submission(FailureKind::TemporarilyUnavailable),
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
        let _ = view.shell.begin_msr_readout_request();
        view.authorize_msr_readouts(cx);
        assert!(
            matches!(
                view.shell.msr_readout_state(),
                MsrReadoutState::Loading { .. }
            ),
            "a non-authorize projection must not submit"
        );
    })
    .unwrap();
}
