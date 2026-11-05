//! test-intent: behavior
//! Device selection must survive first mounting and route replacement.
use super::tests::{headless_perf_app, route_to_performance};
use super::{
    CurveCard, DeviceCategoryKind, DeviceViewCategory, PerformanceDeviceButton,
    PerformanceDeviceFocus, PerformanceDeviceTarget, PerformanceFocus, SystemCurve,
};
use crate::app::{Page, Route, RouteChanged};
use crate::glass::GlassNoiseLayer;
use crate::palette::ui_palette;
use bevy::app::App;
use bevy::color::Alpha;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ui::{BackgroundColor, BorderColor, BoxShadow, Display, Node, Val};
use bevy::ui_widgets::Activate;
use taskmanager_theme::tokens::UiSize;
use taskmanager_theme::{GlassTier, Theme};

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
fn device_panel_gap_has_no_painted_or_interactive_divider() {
    use crate::pages::performance::{PerformanceDeviceSidebar, PerformancePageRoot};

    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    let world = app.world_mut();
    let mut roots = world.query::<(&PerformancePageRoot, &Children)>();
    let children = roots
        .iter(world)
        .next()
        .expect("the performance page is mounted")
        .1
        .to_vec();
    assert!(world.get::<PerformanceDeviceSidebar>(children[0]).is_some());
    let gap = children[1];
    assert!(
        world
            .get::<BackgroundColor>(gap)
            .is_none_or(|color| color.0.alpha() == 0.0),
        "the gap beside the device panel must be transparent"
    );
    assert!(
        world
            .get::<Children>(gap)
            .is_none_or(|children| children.is_empty()),
        "the gap must not contain a painted divider or false resize control"
    );
}

#[test]
fn hero_curve_card_wears_the_card_glass_material() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    let (fill, border, shadow, padding, has_grain) = {
        let world = app.world_mut();
        let mut cards = world.query::<(
            &CurveCard,
            &BackgroundColor,
            &BorderColor,
            Option<&BoxShadow>,
            &Node,
            &Children,
        )>();
        let mut found = None;
        for (card, fill, border, shadow, node, children) in cards.iter(world) {
            if card.0 == SystemCurve::default() {
                found = Some((
                    fill.0,
                    border.top,
                    shadow.cloned(),
                    node.padding,
                    children
                        .iter()
                        .any(|child| world.get::<GlassNoiseLayer>(*child).is_some()),
                ));
            }
        }
        found.expect("the selected hero curve card is mounted")
    };

    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    assert_eq!(fill, palette.glass_fill(GlassTier::Card));
    assert_eq!(border, palette.glass_edge(GlassTier::Card));
    assert!(
        shadow.is_none_or(|shadow| shadow.0.is_empty()),
        "the inline chart must not paint an elevation shadow"
    );
    assert_eq!(padding.left, Val::Rem(palette.panel_content_inset_rem()));
    assert!(has_grain, "the card carries the shared tiled grain");
}

#[test]
fn compact_device_rail_keeps_its_content_track_opaque() {
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);

    let (scroll, surface) = {
        let world = app.world_mut();
        let mut roots = world.query::<(
            &crate::pages::performance::PerformanceDeviceSidebar,
            &Children,
        )>();
        let scroll = roots
            .iter(world)
            .next()
            .expect("the device rail is mounted")
            .1
            .first()
            .copied()
            .expect("the rail owns its scroll viewport");
        let surface = world
            .get::<Children>(scroll)
            .and_then(|children| children.first())
            .copied()
            .expect("the scroll viewport owns its content surface");
        (scroll, surface)
    };
    assert!(
        app.world().get::<GlassNoiseLayer>(scroll).is_none(),
        "the scroll viewport is not a glass panel"
    );
    let (fill, border, has_grain) = {
        let world = app.world_mut();
        let fill = world
            .get::<BackgroundColor>(surface)
            .expect("the rail surface has a fill")
            .0;
        let border = world
            .get::<BorderColor>(surface)
            .expect("the rail surface has a border")
            .top;
        let has_grain = world.get::<Children>(surface).is_some_and(|children| {
            children
                .iter()
                .any(|child| world.get::<GlassNoiseLayer>(*child).is_some())
        });
        (fill, border, has_grain)
    };
    assert_eq!(fill, palette.content_bg);
    assert_eq!(border, palette.border_color);
    assert!(
        !has_grain,
        "the dense device rail stays an opaque content track"
    );
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
    use taskforest_core::core::device_state::DeviceState;
    use taskforest_core::core::metrics::SmartAvailability;
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
