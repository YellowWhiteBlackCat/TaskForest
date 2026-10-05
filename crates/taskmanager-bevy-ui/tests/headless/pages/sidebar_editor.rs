//! test-intent: behavior
//! Actual widget activation updates concrete sidebar choices and their mounted order.
use super::{SidebarAction, SidebarControl, SidebarState};
use crate::app::{FrontendTrack, RouteChanged};
use crate::demo_fixture::demo_shell;
use crate::pages::performance::tests::{headless_perf_app, route_to_performance};
use crate::pages::performance::{PerformanceDeviceButton, PerformanceDeviceTarget};
use crate::window::DemoMode;
use crate::window_surface::{WindowSurface, WindowSurfaceState};
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ui_widgets::Activate;

fn activate(app: &mut App, predicate: impl Fn(&SidebarAction) -> bool) {
    let entity = {
        let world = app.world_mut();
        world
            .query::<(Entity, &SidebarControl)>()
            .iter(world)
            .find(|(_, control)| predicate(&control.0))
            .map(|(entity, _)| entity)
            .expect("a real native editor control")
    };
    app.world_mut().trigger(Activate { entity });
    for _ in 0..3 {
        app.update();
    }
}

#[test]
fn native_editor_hides_restores_and_reorders_specific_devices() {
    let mut app = headless_perf_app();
    app.update();
    app.insert_resource(DemoMode);
    app.world_mut().non_send_mut::<FrontendTrack>().shell = demo_shell();
    route_to_performance(&mut app);
    activate(&mut app, |action| matches!(action, SidebarAction::Open));
    assert!(matches!(
        app.world().resource::<WindowSurfaceState>().0,
        Some(WindowSurface::SidebarDevices)
    ));
    activate(&mut app, |action| {
        matches!(
            action,
            SidebarAction::Visibility(PerformanceDeviceTarget::Memory, false)
        )
    });
    let world = app.world_mut();
    assert!(
        !world
            .query::<&PerformanceDeviceButton>()
            .iter(world)
            .any(|button| button.0 == PerformanceDeviceTarget::Memory)
    );
    activate(&mut app, |action| {
        matches!(
            action,
            SidebarAction::Visibility(PerformanceDeviceTarget::Memory, true)
        )
    });
    let world = app.world_mut();
    assert!(
        world
            .query::<&PerformanceDeviceButton>()
            .iter(world)
            .any(|button| button.0 == PerformanceDeviceTarget::Memory)
    );
    activate(&mut app, |action| {
        matches!(
            action,
            SidebarAction::Move(PerformanceDeviceTarget::Memory, -1)
        )
    });
    assert_eq!(
        app.world()
            .resource::<SidebarState>()
            .0
            .sidebar_order
            .first()
            .map(String::as_str),
        Some("memory")
    );
    activate(&mut app, |action| matches!(action, SidebarAction::Close));
    assert!(app.world().resource::<WindowSurfaceState>().0.is_none());
    app.world_mut().trigger(RouteChanged);
    app.update();
    assert_eq!(
        app.world()
            .resource::<SidebarState>()
            .0
            .sidebar_device_overrides
            .last()
            .map(|entry| (&*entry.device, entry.visible)),
        Some(("memory", true))
    );
}
