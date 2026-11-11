//! Focus dashboard / status-group deduplication proofs for the card lane.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_application::i18n::more_rows_label;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_shell::{ShellApp, fixture};
use taskmanager_theme::{Theme, tokens::UiSize};
use taskmanager_ui_contract::presentation::card_flow::{self, CardFlowAdmission};

use crate::app::{FrontendTrack, Page, PageContext, Route};
use crate::drain::ShellProjectionFolded;
use crate::pages::processes::card_lane::{
    CardFlowBudget, CardFlowMetrics, CardFlowWindow, ProcessViewMode, card_flow_plan,
};
use crate::pages::processes::content;
use crate::palette::{UiPalette, ui_palette};
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
fn the_focus_dashboard_represents_the_first_card_of_its_own_bucket() {
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Running"),
        process(30, "gamma", "Sleeping"),
        process(40, "delta", "Zombie"),
    ];
    let shell = shell_with(items);
    let budget = CardFlowBudget {
        measured: true,
        admission: CardFlowAdmission::Groups(card_flow::CardFlowBudget {
            columns: 2,
            groups: 2,
            card_height: 200.0,
        }),
        columns: 2,
        metrics: Some(CardFlowMetrics::from_palette(&palette())),
        ..Default::default()
    };
    let plan = card_flow_plan(&shell, budget, CardFlowWindow::default());
    assert!(plan.dashboard_visible);
    assert!(plan.groups[0].focus_is_first);
    assert_eq!(plan.groups[0].members, &[1]);
    assert_eq!(plan.groups[0].total, 2);
    assert!(!plan.groups[1].focus_is_first);
    assert_eq!(plan.groups[1].members, &[2]);
}

#[test]
fn the_focus_dashboard_is_not_double_counted_in_the_overflow_action() {
    let items = vec![
        process(10, "alpha", "Running"),
        process(20, "beta", "Running"),
        process(30, "gamma", "Running"),
        process(40, "delta", "Running"),
        process(50, "epsilon", "Running"),
    ];
    let shell = shell_with(items);
    let budget = CardFlowBudget {
        measured: true,
        admission: CardFlowAdmission::Groups(card_flow::CardFlowBudget {
            columns: 1,
            groups: 1,
            card_height: 200.0,
        }),
        columns: 1,
        metrics: Some(CardFlowMetrics::from_palette(&palette())),
        ..Default::default()
    };
    let plan = card_flow_plan(&shell, budget, CardFlowWindow::default());
    let mut app = card_lane_app(shell, palette());
    let world = app.world_mut();
    let group = &plan.groups[0];
    let rows: Vec<crate::pages::processes::ProcessRowView> = {
        let shell = &world.non_send::<FrontendTrack>().shell;
        crate::pages::processes::projection::row_views_for_indices(shell, &group.members, &[])
    };
    let scene = crate::pages::processes::card_lane::group_scene(
        group,
        &rows,
        &[],
        "",
        &palette(),
        CardFlowMetrics::from_palette(&palette()).card_height_px,
    );
    world.spawn_scene(scene).expect("the group scene mounts");
    app.update();
    let texts: Vec<String> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    let expected = more_rows_label(3);
    assert!(
        texts.iter().any(|text| text == &expected),
        "the dashboard represents one of five rows; overflow states three: {texts:?}"
    );
}
