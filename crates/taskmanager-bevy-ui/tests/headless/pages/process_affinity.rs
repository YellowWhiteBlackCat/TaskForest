//! test-intent: behavior
//!
//! Process CPU affinity editor modal behavior over the shell's renderer-neutral lifecycle.

use taskmanager_application::PlatformEffect;
use taskmanager_core::core::process::FrozenProcessIdentity;
use taskmanager_shell::ShellApp;

use super::affinity::{ProcessAffinityModalState, affinity_modal_scene};

fn dummy_identity(pid: u32, name: &str) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, name, 1, 1).expect("valid dummy identity")
}

#[test]
fn affinity_modal_state_open_toggle_and_apply() {
    let mut state = ProcessAffinityModalState::default();
    assert!(!state.is_open());

    let target = dummy_identity(1234, "test_proc");
    state.open(target.clone(), 4);
    assert!(state.is_open());

    let session = state.session.as_ref().unwrap();
    assert_eq!(session.target, target);
    assert_eq!(session.logical_cpu_count, 4);
    assert_eq!(session.selected_mask, vec![0, 1, 2, 3]);

    // Toggle CPU 2 off
    state.toggle_cpu(2);
    assert_eq!(state.session.as_ref().unwrap().selected_mask, vec![0, 1, 3]);

    // Toggle CPU 2 back on
    state.toggle_cpu(2);
    assert_eq!(
        state.session.as_ref().unwrap().selected_mask,
        vec![0, 1, 2, 3]
    );

    // Toggle all off (falls back to keeping CPU 0)
    state.toggle_all();
    assert_eq!(state.session.as_ref().unwrap().selected_mask, vec![0]);

    // Toggle all back on
    state.toggle_all();
    assert_eq!(
        state.session.as_ref().unwrap().selected_mask,
        vec![0, 1, 2, 3]
    );

    // Apply into shell
    let mut shell = ShellApp::new();
    let effect = state.apply(&mut shell);
    assert!(state.session.is_none(), "apply closes the modal session");
    assert!(
        matches!(effect, Some(PlatformEffect::ProcessAffinityControl(_))),
        "apply produces ProcessAffinityControl effect"
    );
}

#[test]
fn affinity_modal_scene_renders_without_panic() {
    let target = dummy_identity(5678, "render_proc");
    let session = super::affinity::AffinitySession {
        target,
        logical_cpu_count: 8,
        selected_mask: vec![0, 2, 4, 6],
        mask_observed: true,
    };

    let theme = taskmanager_theme::Theme::default();
    let palette = crate::palette::ui_palette(&theme);
    let _scene = affinity_modal_scene(&session, &palette);
}
