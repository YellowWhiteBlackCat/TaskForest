//! test-intent: behavior
//!
//! The Applications atomic cards consume the same shared glass material as
//! the focus dashboard; their interaction overlay stays a separate transparent
//! layer with the same rounded boundary.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::color::Color;
use bevy::ecs::query::With;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::prelude::{BackgroundColor, BorderColor, BoxShadow, Node, Val};
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::{GlassTier, Theme, tokens::UiSize};

use crate::app::{FrontendTrack, Page, PageContext, Route};
use crate::drain::ShellProjectionFolded;
use crate::glass::{BackdropMirror, OpticalGlassCard};
use crate::pages::processes::card_lane::{
    CardFlowBudget, ProcessAtomicCard, ProcessCardStateOverlay, ProcessViewMode,
};
use crate::pages::processes::content;
use crate::palette::{UiPalette, ui_palette};
use crate::window::WindowPalette;
use taskmanager_ui_contract::presentation::card_flow::{self, CardFlowAdmission};

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

#[test]
fn atomic_cards_share_the_focus_dashboard_glass_tier() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let mut item = process(42, "glass-card", "Running");
    item.cmdline = "/usr/bin/glass-card --visible".to_owned();
    let sibling = process(43, "sibling-card", "Running");
    let mut app = card_lane_app(shell_with(vec![item, sibling]), palette.clone());
    for _ in 0..3 {
        app.update();
    }

    let world = app.world_mut();
    let mut cards = world.query_filtered::<(
        &ProcessAtomicCard,
        &Node,
        &BackgroundColor,
        &BorderColor,
        &BoxShadow,
        &OpticalGlassCard,
        &BackdropMirror,
    ), ()>();
    let (_, node, fill, edge, shadows, optical, mirror) = cards
        .iter(world)
        .next()
        .expect("the measured lane paints an atomic card");

    assert_eq!(optical.tier, GlassTier::Card);
    assert!(optical.enabled);
    assert_eq!(mirror.tier, GlassTier::Card);
    assert_eq!(fill.0, palette.glass_fill(GlassTier::Card));
    assert_eq!(edge, &BorderColor::all(palette.glass_edge(GlassTier::Card)));
    assert_eq!(shadows, &BoxShadow(palette.glass_shadow(GlassTier::Card)));
    assert_eq!(
        node.border_radius,
        bevy::ui::BorderRadius::all(Val::Rem(palette.panel_radius_rem()))
    );

    let mut overlays =
        world.query_filtered::<(&Node, &BackgroundColor), With<ProcessCardStateOverlay>>();
    let overlay = overlays
        .iter(world)
        .next()
        .expect("interaction ink rides the card state overlay");
    assert_eq!(
        overlay.1.0,
        Color::NONE,
        "the idle overlay leaves the shared painted fallback visible"
    );
    assert_eq!(
        overlay.0.border_radius,
        bevy::ui::BorderRadius::all(Val::Rem(palette.panel_radius_rem()))
    );
}
