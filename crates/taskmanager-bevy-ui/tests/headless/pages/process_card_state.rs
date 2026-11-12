//! test-intent: behavior
//!
//! Atomic cards keep their shared Card-tier glass while the state overlay
//! resolves one readable ink and edge from hover, pressed, focus and shell
//! selection. The overlay fills the card's rounded box without changing its
//! layout or hiding the active row's glass material.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::color::Color;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ecs::system::RunSystemOnce;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::hover::PickingInteraction;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::Pressed;
use bevy::ui::prelude::{
    BackgroundColor, BorderColor, BorderRadius, ComputedNode, Node, UiRect, Val,
};
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::{GlassTier, Theme, tokens::UiSize};
use taskmanager_ui_contract::presentation::card_flow::{self, CardFlowAdmission};
use taskmanager_ui_contract::presentation::control_ink::{ControlInkPriority, ControlInkState};

use crate::app::{FrontendTrack, Page, PageContext, Route};
use crate::drain::ShellProjectionFolded;
use crate::pages::processes::card_lane::{
    CardFlowBudget, ProcessAtomicCard, ProcessCardStateOverlay, ProcessViewMode,
    runtime::{card_state_ink, sync_card_state_overlays},
};
use crate::pages::processes::content;
use crate::palette::{UiPalette, ui_palette};
use crate::widgets::controls::{ControlTone, ControlVisual, control_ink_priority};
use crate::window::WindowPalette;

fn process(pid: u32, name: &str, status: &str) -> ProcessItem {
    let mut item = ProcessItem::new(pid, name);
    item.status = status.to_owned();
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        cpu_percentage: ScalarObservation::available(12.5, 1),
        memory_bytes: ScalarObservation::available(64 * 1024 * 1024, 1),
        ..ProcessScalarObservations::default()
    });
    item
}

fn shell_with(items: Vec<ProcessItem>) -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| *processes = Some(items));
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}

fn palette() -> UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

fn card_lane_app(shell: ShellApp, palette: UiPalette) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app.add_plugins(crate::input::InputPlugin);
    app.insert_resource(WindowPalette {
        inner: palette.clone(),
    });
    app.insert_resource(ProcessViewMode::Cards);
    app.insert_resource(CardFlowBudget {
        measured: true,
        admission: CardFlowAdmission::Groups(card_flow::CardFlowBudget {
            columns: 2,
            groups: 1,
            card_height: 200.0,
        }),
        columns: 2,
        ..Default::default()
    });
    app.init_resource::<Route>();
    app.init_resource::<InputFocus>();
    app.world_mut().resource_mut::<Route>().page = Page::Processes;
    let history = crate::pages::history::HistoryProjectionResource::default();
    let context = PageContext {
        sidebar: &Default::default(),
        gray_zero_values: false,
        shell: &shell,
        palette: &palette,
        history: &history.0,
    };
    app.world_mut()
        .spawn_scene(content(&context))
        .expect("the page scene resolves without assets");
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });
    crate::pages::processes::card_lane::register(&mut app);
    app.update();
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app
}

fn unselected_card(app: &mut App) -> bevy::ecs::entity::Entity {
    app.world_mut()
        .query_filtered::<(
            bevy::ecs::entity::Entity,
            &ProcessAtomicCard,
            &ControlVisual,
        ), ()>()
        .iter(app.world())
        .find(|(_, _, visual)| !visual.1)
        .map(|(entity, _, _)| entity)
        .expect("the card flow mounts one unselected card")
}

fn overlay_of(card: bevy::ecs::entity::Entity, app: &mut App) -> bevy::ecs::entity::Entity {
    app.world_mut()
        .query_filtered::<(bevy::ecs::entity::Entity, &ChildOf), With<ProcessCardStateOverlay>>()
        .iter(app.world())
        .find(|(_, parent)| parent.parent() == card)
        .map(|(entity, _)| entity)
        .unwrap_or_else(|| panic!("every atomic card owns one state overlay"))
}

fn paint_states(app: &mut App) {
    app.world_mut()
        .run_system_once(sync_card_state_overlays)
        .expect("the state overlay system resolves its declared resources");
}

#[test]
fn conflicting_interaction_bits_resolve_one_shared_card_ink() {
    let palette = palette();
    let visual = ControlVisual(ControlTone::Card, true);
    let shared = ControlInkPriority::resolve(true, true, true, false, true);
    assert_eq!(
        shared,
        ControlInkState::Pressed,
        "press outranks hover, focus and selection in the shared fold"
    );
    assert_eq!(
        control_ink_priority(&visual, PickingInteraction::Hovered, true).state(),
        shared,
        "Bevy's hover, official press and selection project into the same verdict"
    );
    assert_eq!(
        control_ink_priority(&visual, PickingInteraction::Hovered, false).state(),
        ControlInkState::Hovered,
        "hover outranks the durable selected bit"
    );
    let (wash, edge) = card_state_ink(&visual, PickingInteraction::Hovered, true, &palette);
    assert_eq!(wash, palette.selection_bg);
    assert_eq!(edge, palette.focus_ring);
    let (hover_wash, hover_edge) =
        card_state_ink(&visual, PickingInteraction::Hovered, false, &palette);
    assert_eq!(hover_wash, palette.hover_bg);
    assert_eq!(
        hover_edge, palette.focus_ring,
        "the durable selected edge survives the transient hover wash"
    );
}

#[test]
fn the_state_overlay_bounds_and_theme_material_are_one_rounded_card() {
    let palette = palette();
    let mut app = card_lane_app(
        shell_with(vec![
            process(10, "state-one", "Running"),
            process(20, "state-two", "Running"),
        ]),
        palette.clone(),
    );
    let card = unselected_card(&mut app);
    let overlay = overlay_of(card, &mut app);
    let (card_node, fill, edge, shadows) = {
        let world = app.world();
        let node = world.get::<Node>(card).expect("the card owns a node");
        let fill = world
            .get::<BackgroundColor>(card)
            .expect("the card owns its glass fill");
        let edge = world
            .get::<BorderColor>(card)
            .expect("the card owns its glass edge");
        let shadows = world
            .get::<bevy::ui::prelude::BoxShadow>(card)
            .expect("the card owns its layered shadow");
        (node.clone(), *fill, *edge, shadows.clone())
    };
    let (overlay_node, overlay_fill, overlay_edge, overlay_computed) = {
        let world = app.world();
        let node = world
            .get::<Node>(overlay)
            .expect("the state overlay owns a node");
        let fill = world
            .get::<BackgroundColor>(overlay)
            .expect("the state overlay owns a fill");
        let edge = world
            .get::<BorderColor>(overlay)
            .expect("the state overlay owns an edge");
        let computed = world
            .get::<ComputedNode>(overlay)
            .expect("the state overlay is measured");
        (node.clone(), *fill, *edge, *computed)
    };
    let panel_radius = Val::Rem(palette.panel_radius_rem());
    assert_eq!(card_node.border_radius, BorderRadius::all(panel_radius));
    assert_eq!(overlay_node.border_radius, card_node.border_radius);
    assert_eq!(overlay_node.border, card_node.border);
    assert_eq!(
        overlay_node.border,
        UiRect::all(Val::Rem(GlassTier::Card.edge_width_rem()))
    );
    assert_eq!(overlay_node.width, Val::Percent(100.0));
    assert_eq!(overlay_node.height, Val::Percent(100.0));
    assert_eq!(
        overlay_computed.size(),
        app.world().get::<ComputedNode>(card).unwrap().size()
    );
    assert_eq!(fill.0, palette.glass_fill(GlassTier::Card));
    assert_eq!(edge, BorderColor::all(palette.glass_edge(GlassTier::Card)));
    assert_eq!(
        shadows,
        bevy::ui::prelude::BoxShadow(palette.glass_shadow(GlassTier::Card))
    );
    assert_eq!(overlay_fill.0, Color::NONE);
    assert_eq!(overlay_edge, BorderColor::DEFAULT);
}

#[test]
fn hover_pressed_and_focus_resolve_distinct_theme_state_ink() {
    let palette = palette();
    let mut app = card_lane_app(
        shell_with(vec![
            process(10, "state-one", "Running"),
            process(20, "state-two", "Running"),
        ]),
        palette.clone(),
    );
    let card = unselected_card(&mut app);
    let overlay = overlay_of(card, &mut app);

    let state = |app: &mut App| {
        paint_states(app);
        let world = app.world();
        (
            world.get::<BackgroundColor>(overlay).unwrap().0,
            *world.get::<BorderColor>(overlay).unwrap(),
        )
    };

    app.world_mut()
        .entity_mut(card)
        .insert(PickingInteraction::Hovered);
    let hover = state(&mut app);
    assert_eq!(hover.0, palette.hover_bg);
    assert_eq!(hover.1, BorderColor::all(palette.accent));

    app.world_mut()
        .entity_mut(card)
        .insert(PickingInteraction::Pressed);
    let pressed = state(&mut app);
    assert_eq!(pressed.0, palette.selection_bg);
    assert_eq!(pressed.1, BorderColor::all(palette.focus_ring));

    app.world_mut().entity_mut(card).insert((
        PickingInteraction::None,
        ControlVisual(ControlTone::Card, true),
    ));
    let selected = state(&mut app);
    assert_eq!(selected.0, palette.selection_bg);
    assert_eq!(selected.1, BorderColor::all(palette.focus_ring));

    app.world_mut().entity_mut(card).insert((
        PickingInteraction::None,
        ControlVisual(ControlTone::Card, false),
    ));
    let idle = state(&mut app);
    assert_eq!(idle.0, Color::NONE);
    assert_eq!(idle.1, BorderColor::DEFAULT);
    assert_ne!(hover.0, pressed.0);
}

#[test]
fn keyboard_focus_and_button_press_keep_the_official_state_ink() {
    let palette = palette();
    let mut app = card_lane_app(
        shell_with(vec![
            process(10, "state-one", "Running"),
            process(20, "state-two", "Running"),
        ]),
        palette.clone(),
    );
    let card = unselected_card(&mut app);
    let overlay = overlay_of(card, &mut app);

    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(card, FocusCause::Navigated);
    app.world_mut().entity_mut(card).insert(Pressed);
    paint_states(&mut app);
    let world = app.world();
    assert_eq!(
        world.get::<BackgroundColor>(overlay).unwrap().0,
        palette.selection_bg,
        "an officially pressed card wears the stronger selection wash"
    );
    assert_eq!(
        *world.get::<BorderColor>(overlay).unwrap(),
        BorderColor::all(palette.focus_ring),
        "the keyboard-focused card's pressed edge reads above the glass"
    );
}
