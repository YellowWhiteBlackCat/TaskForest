//! test-intent: behavior
//!
//! Interaction-model tests for the shared menu and dialog widgets:
//!
//! - the menu keyboard state machine: clamped (never wrapping) moves,
//!   confirm that refuses disabled entries, cancel that is always available,
//!   and empty-list safety — a mutation to wrap-around navigation, to
//!   authorizing disabled actions, or to a panicking empty menu fails here;
//! - the confirmation dialog's double-echo semantics (the shell's
//!   `PendingConfirmation` contract): the confirm outcome echoes the target
//!   id the dialog was armed with — the binding, not any shared slot — and
//!   dismiss never echoes; the rendered body carries the same id so the
//!   user confirms what the wiring receives;
//! - the render adapters that carry those semantics: the cursor row is the
//!   only highlighted one (fill-only, so the text census stays stable).
//!
//! Mounted from `widgets/menu.rs` (the dialog core is exercised through the
//! same module tree; `widgets/dialog.rs` keeps no test mount of its own).

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets};
use bevy::ecs::hierarchy::ChildOf;
use bevy::scene::{Scene, ScenePlugin, WorldSceneExt};
use bevy::text::Font;
use bevy::ui::widget::Text;
use taskmanager_theme::Theme;

use crate::palette::ui_palette;
use crate::widgets::dialog::{
    ConfirmationDialog, DialogInput, DialogOutcome, DialogSpec, confirmation_scene,
};
use crate::widgets::menu::{
    MenuInput, MenuItem, MenuOutcome, MenuSpec, MenuState, menu_row_background, menu_scene_at,
};

fn headless_scene_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    app
}

fn item(label: &str, enabled: bool) -> MenuItem {
    MenuItem {
        label: label.to_owned(),
        enabled,
    }
}

fn two_row_spec() -> MenuSpec {
    MenuSpec {
        title: "Row actions".to_owned(),
        items: vec![item("End task", true), item("Properties", true)],
    }
}

// ---- menu keyboard model ----

#[test]
fn menu_navigation_clamps_at_both_ends_never_wraps() {
    let items = vec![
        item("Open", true),
        item("Properties", true),
        item("Kill", false),
    ];
    let mut state = MenuState::new();
    assert_eq!(state.selection(), 0, "a fresh menu starts at the top");
    assert_eq!(state.advance(&items, MenuInput::Up), None);
    assert_eq!(
        state.selection(),
        0,
        "up from the top clamps — a destructive menu must not wrap to the bottom"
    );
    for _ in 0..items.len() + 2 {
        assert_eq!(state.advance(&items, MenuInput::Down), None);
    }
    assert_eq!(
        state.selection(),
        items.len() - 1,
        "down clamps at the last entry, however often pressed"
    );
}

#[test]
fn menu_confirm_refuses_disabled_entries_and_reports_the_index() {
    let items = vec![item("Open", true), item("Kill", false)];
    let mut state = MenuState::new();
    state.advance(&items, MenuInput::Down);
    assert_eq!(
        state.advance(&items, MenuInput::Confirm),
        None,
        "a disabled entry is never authorized, however it is reached"
    );
    assert_eq!(
        state.selection(),
        1,
        "the refused confirm leaves the menu open on the same row"
    );
    state.advance(&items, MenuInput::Up);
    assert_eq!(
        state.advance(&items, MenuInput::Confirm),
        Some(MenuOutcome::Confirmed(0)),
        "confirming an enabled entry reports its index into the same item slice"
    );
}

#[test]
fn menu_cancel_is_always_available_and_empty_lists_are_inert() {
    let mut state = MenuState::new();
    assert_eq!(
        state.advance(&[], MenuInput::Cancel),
        Some(MenuOutcome::Canceled),
        "cancel needs no items"
    );
    assert_eq!(
        state.advance(&[], MenuInput::Confirm),
        None,
        "an empty menu confirms nothing"
    );
    assert_eq!(
        state.advance(&[], MenuInput::Down),
        None,
        "empty navigation is a safe no-op"
    );
    assert_eq!(
        state.advance(&[], MenuInput::Up),
        None,
        "empty upward navigation is a safe no-op"
    );
}

// ---- menu render adapter ----

#[test]
fn menu_scene_fills_only_the_cursor_row_and_keeps_the_text_census_stable() {
    let palette = ui_palette(&Theme::dark());
    let spec = two_row_spec();

    let mut app = headless_scene_app();
    let world = app.world_mut();
    let root = world
        .spawn_scene(menu_scene_at(&spec, &MenuState::new(), &palette))
        .expect("the menu scene resolves without assets")
        .id();

    // Each row's text node hangs under its row node, so the parent's fill is
    // the row highlight; the title's parent is the panel itself.
    let mut rows = world.query::<(&Text, &ChildOf)>();
    let mut fill_of = |world: &bevy::ecs::world::World, label: &str| -> bevy::color::Color {
        for (text, parent) in rows.iter(world) {
            if text.0 == label {
                return world
                    .get::<bevy::ui::BackgroundColor>(parent.0)
                    .map(|fill| fill.0)
                    .unwrap_or(bevy::color::Color::NONE);
            }
        }
        panic!("the {label} row must render");
    };
    assert_eq!(
        fill_of(world, "End task").to_srgba(),
        menu_row_background(true, &palette).to_srgba(),
        "the cursor row is the highlighted one"
    );
    assert_eq!(
        fill_of(world, "Properties").to_srgba(),
        menu_row_background(false, &palette).to_srgba(),
        "a non-cursor row paints no highlight"
    );
    assert_eq!(
        menu_row_background(false, &palette).to_srgba(),
        bevy::color::Color::NONE.to_srgba(),
        "the idle row fill is transparent, letting the panel show through"
    );
    assert!(world.despawn(root), "the menu scene despawns cleanly");

    // Moving the cursor moves the fill; the census (title + one text per
    // entry) stays identical because the highlight is fill-only.
    let mut moved = MenuState::new();
    moved.advance(&spec.items, MenuInput::Down);
    let mut other = headless_scene_app();
    let world = other.world_mut();
    let root = world
        .spawn_scene(menu_scene_at(&spec, &moved, &palette))
        .expect("the moved-cursor scene resolves")
        .id();
    let texts = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect::<Vec<String>>();
    assert_eq!(
        texts,
        vec![
            "Row actions".to_owned(),
            "End task".to_owned(),
            "Properties".to_owned()
        ],
        "no marker-glyph text appears for any cursor position"
    );
    assert!(world.despawn(root));
}

// ---- dialog double-echo model ----

fn end_task_dialog(target: &str) -> ConfirmationDialog {
    ConfirmationDialog::new(
        DialogSpec {
            title: "End task?".to_owned(),
            body: "This will terminate the process.".to_owned(),
            confirm_label: "End task".to_owned(),
            dismiss_label: "Cancel".to_owned(),
        },
        target,
    )
}

#[test]
fn dialog_confirm_echoes_the_armed_target_id_not_a_shared_slot() {
    let first = end_task_dialog("firefox (pid 4242) · start 1735632000");
    assert_eq!(
        first.activate(DialogInput::Confirm),
        DialogOutcome::Confirmed {
            target_id: "firefox (pid 4242) · start 1735632000".to_owned()
        },
        "the confirm outcome echoes the id the dialog was armed with"
    );
    // A differently-armed dialog echoes ITS id: the echo is the binding,
    // which is what keeps a stale dialog from authorizing another target.
    let second = end_task_dialog("systemd (pid 1)");
    assert_eq!(
        second.activate(DialogInput::Confirm),
        DialogOutcome::Confirmed {
            target_id: "systemd (pid 1)".to_owned()
        }
    );
}

#[test]
fn dialog_dismiss_discards_without_echoing_any_target() {
    let dialog = end_task_dialog("firefox (pid 4242) · start 1735632000");
    assert_eq!(
        dialog.activate(DialogInput::Dismiss),
        DialogOutcome::Dismissed
    );
}

#[test]
fn confirmation_scene_renders_the_target_id_the_confirm_will_echo() {
    let palette = ui_palette(&Theme::dark());
    let dialog = end_task_dialog("firefox (pid 4242) · start 1735632000");
    let mut app = headless_scene_app();
    let world = app.world_mut();
    let root = world
        .spawn_scene(confirmation_scene(&dialog, &palette))
        .expect("the confirmation scene resolves without assets")
        .id();
    let body = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .find(|text| text.contains("terminate"))
        .expect("the body line renders");
    assert!(
        body.contains("firefox (pid 4242) · start 1735632000"),
        "the displayed body carries the exact id the confirm outcome echoes: {body}"
    );
    assert!(
        world.despawn(root),
        "the confirmation scene despawns cleanly"
    );
}

/// The plain dialog panel keeps its four-text shape (title, body, confirm,
/// dismiss) — the shared widget tests rely on it and the confirmation path
/// composes on top of it.
#[test]
fn confirmation_scene_is_still_the_four_text_panel() {
    let palette = ui_palette(&Theme::dark());
    let dialog = end_task_dialog("any target");
    let mut app = headless_scene_app();
    let world = app.world_mut();
    let scene: Box<dyn Scene> = Box::new(confirmation_scene(&dialog, &palette));
    let root = world
        .spawn_scene(scene)
        .expect("the boxed scene resolves like any other")
        .id();
    let count = world.query::<&Text>().iter(world).count();
    assert_eq!(count, 4, "title, echoed body, confirm, dismiss");
    assert!(world.despawn(root));
}
