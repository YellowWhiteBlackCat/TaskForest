use super::*;
use taskforest_contract::RequestId;
use taskmanager_application::PlatformEffect;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{ProcessAffinityReady, ProcessAffinityState};
use taskmanager_shell::fixture::ProjectionSeedFact;
use taskmanager_shell::fixture::seed_projection_fact;

#[test]
fn process_affinity_editor_freezes_identity_and_applies_a_sorted_mask() {
    set_language(Language::En);
    let mut app = IcedApp::demo();
    let target = app
        .shell
        .selected_process_identity()
        .expect("demo process must have an authoritative identity");
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::ProcessAffinity(Some(ProcessAffinityState::Ready(
            ProcessAffinityReady {
                request_id: RequestId::MIN,
                target: target.clone(),
                cpus: vec![2, 0],
            },
        ))),
    );

    let _ = app.update(Message::OpenProcessAffinity);
    assert!(app.affinity_open());
    assert_eq!(app.affinity_target(), Some(&target));
    assert!(app.process_presentation.affinity_cpus.is_some());
    assert!(
        app.process_presentation
            .affinity_cpus
            .as_ref()
            .is_some_and(|cpus| cpus.contains(&0) && cpus.contains(&2))
    );

    app.shell.selected = usize::MAX;
    assert!(app.shell.selected_process_identity().is_none());
    let _ = app.update(Message::ToggleProcessAffinityCpu(2));
    let effect = app.apply_process_affinity_effect();
    match effect {
        Some(PlatformEffect::ProcessAffinityControl(request)) => {
            assert_eq!(request.target, target);
            assert_eq!(request.cpus, vec![0]);
        }
        other => panic!("expected an affinity control effect, got {other:?}"),
    }
    assert!(!app.affinity_open(), "Apply closes the local editor");
}

#[test]
fn process_affinity_editor_rejects_unobserved_or_empty_masks() {
    set_language(Language::En);
    let mut app = IcedApp::demo();
    let _ = app.update(Message::OpenProcessAffinity);
    assert!(app.process_presentation.affinity_cpus.is_none());
    assert!(app.apply_process_affinity_effect().is_none());

    let target = app
        .shell
        .selected_process_identity()
        .expect("demo process must have an authoritative identity");
    app.open_local_surface(LocalSurface::ProcessAffinity { target });
    app.process_presentation.affinity_cpus = Some(std::collections::HashSet::new());
    assert!(app.apply_process_affinity_effect().is_none());
    assert!(
        app.affinity_open(),
        "an empty mask must keep the editor open"
    );
    assert!(app.shell.feedback_text().contains("Select at least one"));
}

/// The editor's status line reads the owner's localized reason for a failed
/// read — never a `Debug` dump of the typed failure, and never a sentence
/// spelled inside the frontend. Only an observed mask is summarized; an
/// unobserved one says so.
#[test]
fn process_affinity_status_reads_the_shared_failure_reason() {
    use taskforest_core::core::failure::FailureKind;
    use taskmanager_application::i18n::control_error_detail;
    use taskmanager_application::i18n::t;

    set_language(Language::En);
    let collected = crate::ui::affinity::affinity_status_text(None, false, 0, 8);
    assert_eq!(collected, t("common.collecting_telemetry"));

    let observed = crate::ui::affinity::affinity_status_text(None, true, 3, 8);
    assert!(
        observed.contains('3') && observed.contains('8'),
        "{observed}"
    );

    for failure in [
        FailureKind::PermissionDenied,
        FailureKind::Unsupported,
        FailureKind::TemporarilyUnavailable,
        FailureKind::TimedOut,
        FailureKind::Rejected,
        FailureKind::IdentityChanged,
        FailureKind::MissingDependency,
        FailureKind::ProviderFault,
        FailureKind::RequiresEscalation,
    ] {
        let status = crate::ui::affinity::affinity_status_text(Some(failure), false, 0, 8);
        assert_eq!(status, control_error_detail(failure));
        assert!(
            !status.contains(&format!("{failure:?}")),
            "the status must never be a Debug dump: {status}"
        );
    }
}
