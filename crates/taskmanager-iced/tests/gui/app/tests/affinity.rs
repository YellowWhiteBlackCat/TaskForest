use super::*;
use taskmanager_application::i18n::{Language, set_language};

#[test]
fn process_affinity_editor_freezes_identity_and_applies_a_sorted_mask() {
    set_language(Language::En);
    let mut app = IcedApp::demo();
    let target = app
        .shell
        .selected_process_identity()
        .expect("demo process must have an authoritative identity");
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ProcessAffinity(Some(
            taskmanager_application::ProcessAffinityReady {
                request_id: taskmanager_platform_contract::RequestId::MIN,
                target: target.clone(),
                cpus: vec![2, 0],
            },
        )),
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
        Some(taskmanager_application::PlatformEffect::ProcessAffinityControl(request)) => {
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
