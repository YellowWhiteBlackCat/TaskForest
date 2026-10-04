//! test-intent: behavior

use super::*;
use crate::about_modal::AboutCommand;
use crate::app::SharedRuntimeHandle;
use crate::runtime::SharedRuntime;
use crate::window::tests::scripted_frontend_app;
use crate::window_surface::WindowSurfaceOverlay;
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_test_support::setup_script::platform;

#[test]
fn explicit_setup_actions_submit_once_and_switching_surfaces_destroys_the_old_tree() {
    let mut app = scripted_frontend_app();
    let (platform, recorder) = platform();
    let runtime = Box::leak(Box::new(SharedRuntime::new(platform)));
    app.insert_resource(SharedRuntimeHandle { shared: runtime });
    app.world_mut().resource_mut::<SetupState>().0 =
        FirstRunController::from_observation(Some(setup_script_info()));
    app.world_mut()
        .non_send_mut::<crate::app::FrontendTrack>()
        .initial_refresh_submitted = true;
    app.update();
    assert!(app.world().resource::<WindowSurfaceState>().0.is_none());
    app.world_mut().trigger(FirstRunCommand::Open);
    app.update();
    app.update();
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::FirstRun)
    ));
    let control = app
        .world_mut()
        .query::<(Entity, &SetupControl)>()
        .iter(app.world())
        .find(|(_, control)| matches!(control.0, FirstRunCommand::Action(SetupScriptAction::Run)))
        .map(|(entity, _)| entity)
        .expect("real Run control");
    app.world_mut().trigger(Activate { entity: control });
    app.update();
    assert_eq!(recorder.submissions().expect("recorded").len(), 1);
    assert_eq!(
        app.world().resource::<SetupState>().0.view().phase,
        FirstRunPhase::Running
    );
    app.world_mut()
        .trigger(FirstRunCommand::Action(SetupScriptAction::Revert));
    app.update();
    assert_eq!(recorder.submissions().expect("recorded").len(), 1);
    app.world_mut().trigger(AboutCommand::Open);
    app.update();
    app.update();
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::About)
    ));
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<WindowSurfaceOverlay>>()
            .iter(app.world())
            .count(),
        1
    );
    assert!(
        app.world_mut()
            .query::<&WindowSurfaceOverlay>()
            .iter(app.world())
            .all(|overlay| overlay.0 != WindowSurfaceKind::FirstRun)
    );
    app.world_mut().trigger(FirstRunCommand::Close);
    app.update();
    assert!(
        matches!(
            app.world().resource::<WindowSurfaceState>().0,
            Some(WindowSurface::About)
        ),
        "a stale close cannot dismiss the replacement surface"
    );
}
