//! test-intent: behavior
//! Device selection must survive first mounting and route replacement.
use super::tests::{headless_perf_app, route_to_performance};
use super::{
    CurveCard, DeviceCategoryKind, DeviceViewCategory, PerformanceDeviceButton,
    PerformanceDeviceFocus, PerformanceDeviceTarget, PerformanceFocus, SystemCurve,
};
use crate::app::{Page, Route, RouteChanged};
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ui::{Display, Node};
use bevy::ui_widgets::Activate;

fn visible_categories(app: &mut App) -> Vec<DeviceCategoryKind> {
    let world = app.world_mut();
    let mut query = world.query::<(&DeviceViewCategory, &Node)>();
    query
        .iter(world)
        .filter(|(_, node)| node.display != Display::None)
        .map(|(category, _)| category.0)
        .collect()
}

#[test]
fn initial_selection_is_painted_on_the_first_mounted_scene() {
    let mut app = headless_perf_app();
    app.insert_resource(PerformanceDeviceFocus(PerformanceDeviceTarget::Memory));
    app.insert_resource(PerformanceFocus(SystemCurve::Memory));
    app.update();
    route_to_performance(&mut app);
    assert_eq!(
        visible_categories(&mut app),
        vec![DeviceCategoryKind::Memory]
    );
    let world = app.world_mut();
    let mut cards = world.query::<(&CurveCard, &Node)>();
    let visible: Vec<_> = cards
        .iter(world)
        .filter(|(_, node)| node.display != Display::None)
        .map(|(card, _)| card.0)
        .collect();
    assert_eq!(visible, vec![SystemCurve::Memory]);
}

#[test]
fn native_device_activation_survives_leaving_and_returning_to_performance() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);
    let button = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &PerformanceDeviceButton)>();
        buttons
            .iter(world)
            .find(|(_, button)| button.0 == PerformanceDeviceTarget::Memory)
            .map(|(entity, _)| entity)
            .expect("mounted memory button")
    };
    app.world_mut()
        .commands()
        .trigger(Activate { entity: button });
    app.update();
    assert_eq!(
        visible_categories(&mut app),
        vec![DeviceCategoryKind::Memory]
    );
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    app.world_mut().commands().trigger(RouteChanged);
    app.update();
    app.update();
    assert!(visible_categories(&mut app).is_empty());
    route_to_performance(&mut app);
    assert_eq!(
        visible_categories(&mut app),
        vec![DeviceCategoryKind::Memory]
    );
    assert_eq!(
        app.world().resource::<PerformanceDeviceFocus>().0,
        PerformanceDeviceTarget::Memory
    );
}

#[test]
fn smart_source_recovery_replaces_the_mounted_guidance_without_changing_identity() {
    use crate::app::FrontendTrack;
    use crate::demo_fixture::{demo_shell, seed_capture_confirmation_scenario};
    use crate::drain::ShellProjectionFolded;
    use crate::pages::performance::{DynField, DynText};
    use bevy::ui::widget::Text;
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::metrics::SmartAvailability;
    use taskmanager_shell::fixture::edit_snapshot;
    use taskmanager_test_support::pin_english;
    pin_english();
    let mut app = headless_perf_app();
    app.update();
    let mut shell = demo_shell();
    seed_capture_confirmation_scenario(&mut shell, "smart-missing-tool");
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    route_to_performance(&mut app);
    let guidance = |app: &mut App| {
        let world = app.world_mut();
        world
            .query::<(&DynText, &Text)>()
            .iter(world)
            .find(|(field, _)| matches!(field.0, DynField::SmartGuidance(_)))
            .map(|(_, text)| text.0.clone())
            .expect("mounted guidance line")
    };
    assert!(guidance(&mut app).contains("Install the required"));
    edit_snapshot(
        &mut app.world_mut().non_send_mut::<FrontendTrack>().shell,
        |snapshot| {
            let disk = &mut snapshot.as_mut().expect("snapshot").disks[0];
            disk.smart_availability = SmartAvailability::Available;
            disk.smart_state = DeviceState::healthy(2);
            disk.smart_temperature_c = Some(42.0);
        },
    );
    app.world_mut().trigger(ShellProjectionFolded);
    app.update();
    assert!(
        guidance(&mut app).is_empty(),
        "the old recovery instruction is withdrawn"
    );
}
