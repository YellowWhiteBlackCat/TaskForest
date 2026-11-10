//! test-intent: behavior
//!
//! Process CPU affinity editor modal behavior over the shell's renderer-neutral lifecycle.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::AssetPlugin;
use bevy::ecs::query::With;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use taskforest_core::core::process::FrozenProcessIdentity;
use taskmanager_application::PlatformEffect;
use taskmanager_shell::ShellApp;
use taskmanager_theme::GlassTier;
use taskmanager_theme::tokens::UiSize;

use super::affinity::{ProcessAffinityModalState, affinity_modal_scene};
use crate::glass::OpticalGlassCard;
use taskmanager_theme::Theme;

fn dummy_identity(pid: u32, name: &str) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(pid, name, 1, 1).expect("valid dummy identity")
}

#[test]
fn affinity_modal_state_open_toggle_and_apply() {
    let mut state = ProcessAffinityModalState::default();
    assert!(state.session.is_none());

    let target = dummy_identity(1234, "test_proc");
    state.open(target.clone(), 4);
    assert!(state.session.is_some());

    let session = state.session.as_ref().unwrap();
    assert_eq!(session.target, target);
    assert_eq!(session.logical_cpu_count, 4);
    // Opening fabricates nothing: until a correlated read lands the grid shows
    // no mask and the toggles keep it that way (the Iced/TUI rule).
    assert!(!session.mask_observed);
    assert!(session.selected_mask.is_empty());
    assert_eq!(session.failure, None);
    state.toggle_cpu(2);
    state.toggle_all();
    assert!(
        state.session.as_ref().unwrap().selected_mask.is_empty(),
        "an unobserved mask is never editable"
    );

    // An authoritative read seeds the grid and unblocks editing.
    let session = state.session.as_mut().unwrap();
    session.selected_mask = vec![0, 1, 2, 3];
    session.mask_observed = true;
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

/// The shared read outcome drives the editor: an authoritative read seeds the
/// grid once, a failed read for the same identity drops the mask and carries
/// the owner's reason (never a `Debug` dump), and an outcome for another
/// identity never enters the editor at all.
#[test]
fn affinity_modal_state_failed_read_is_read_only_and_reports_the_shared_reason() {
    use taskforest_contract::RequestId;
    use taskforest_core::core::failure::FailureKind;
    use taskmanager_application::i18n::control_error_detail;
    use taskmanager_application::{ProcessAffinityReady, ProcessAffinityState, RequestCorrelation};

    let mut state = ProcessAffinityModalState::default();
    let target = dummy_identity(99, "failed_proc");
    state.open(target.clone(), 4);

    // Another process's outcome is not this editor's outcome.
    let foreign = ProcessAffinityState::Failed {
        correlation: RequestCorrelation::Request(RequestId::MIN),
        target: dummy_identity(100, "other_proc"),
        failure: FailureKind::TimedOut,
        last_good: None,
    };
    let session = state.session.as_mut().unwrap();
    assert!(!session.absorb_outcome(&foreign));
    assert_eq!(session.failure, None, "a foreign failure never paints here");

    // An authoritative read for this identity seeds the grid once.
    let ready = ProcessAffinityState::Ready(ProcessAffinityReady {
        request_id: RequestId::MIN,
        target: target.clone(),
        cpus: vec![0, 1],
    });
    let session = state.session.as_mut().unwrap();
    assert!(session.absorb_outcome(&ready));
    assert!(session.mask_observed);
    assert_eq!(session.selected_mask, vec![0, 1]);
    assert!(session.is_editable());

    // The user's in-progress edit is not clobbered by a repeated read.
    session.selected_mask = vec![0];
    assert!(!session.absorb_outcome(&ready));
    assert_eq!(session.selected_mask, vec![0]);

    // The read for this identity now fails: the mask is dropped, the editor is
    // read-only, and the status line carries the owner's reason.
    let failure = FailureKind::PermissionDenied;
    let failed = ProcessAffinityState::Failed {
        correlation: RequestCorrelation::Request(RequestId::MIN),
        target: target.clone(),
        failure,
        last_good: None,
    };
    let session = state.session.as_mut().unwrap();
    assert!(session.absorb_outcome(&failed));
    assert!(!session.is_editable());
    assert!(session.selected_mask.is_empty());

    let mut shell = ShellApp::new();
    assert!(
        state.apply(&mut shell).is_none(),
        "a failed read never submits"
    );
    assert!(state.session.is_some(), "the editor stays open for a retry");

    state.toggle_cpu(1);
    state.toggle_all();
    assert!(state.session.as_ref().unwrap().selected_mask.is_empty());

    let reason = control_error_detail(failure);
    assert!(
        !reason.contains(&format!("{failure:?}")),
        "the status is never a Debug dump of the typed failure: {reason}"
    );
    let theme = Theme::default();
    let palette = crate::palette::ui_palette(&theme, UiSize::Standard);
    let scene = affinity_modal_scene(state.session.as_ref().unwrap(), &palette);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    let world = app.world_mut();
    world
        .spawn_scene(scene)
        .expect("the failed-read modal scene resolves");
    let painted: Vec<String> = world
        .query::<&bevy::ui::widget::Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    assert!(
        painted.iter().any(|line| line.contains(reason)),
        "the modal must paint the owner's reason, painted: {painted:?}"
    );
    assert!(
        painted
            .iter()
            .all(|line| !line.contains(&format!("{failure:?}"))),
        "the modal must never paint a Debug dump, painted: {painted:?}"
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
        failure: None,
    };

    let theme = Theme::default();
    let palette = crate::palette::ui_palette(&theme, UiSize::Standard);
    let scene = affinity_modal_scene(&session, &palette);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    let world = app.world_mut();
    world
        .spawn_scene(scene)
        .expect("the affinity modal scene resolves without app resources");
    let card = world
        .query_filtered::<&OpticalGlassCard, With<bevy::ui::Node>>()
        .single(world)
        .expect("the affinity modal is an optical glass candidate");
    assert!(card.enabled && card.tier == GlassTier::Modal);
}
