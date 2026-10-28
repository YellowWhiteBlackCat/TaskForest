//! test-intent: behavior
//!
//! Native search-editor bridge: attach, shell synchronisation, and the
//! Escape clear/close ladder. The native text-input plugin itself is absent
//! in this headless composition, so these tests drive the bridge directly.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::Assets;
use bevy::ecs::entity::Entity;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::FocusedInput;
use bevy::text::{EditableText, Font};
use taskmanager_shell::{SearchSlot, ShellApp};

use super::{NativeTextEditing, SearchEditor, sync_search_editor};
use crate::app::FrontendTrack;
use crate::pages::processes::ProcessSearchInput;
use crate::pages::services::ServicesSearchInput;

fn app_with_shell(query: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(crate::input::InputPlugin);
    app.init_resource::<Assets<Font>>();

    let mut shell = ShellApp::new();
    shell.open_search();
    if !query.is_empty() {
        shell.push_search_text(query);
    }
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });

    let editor = app
        .world_mut()
        .spawn((ProcessSearchInput, SearchEditor(query.to_owned())))
        .id();
    app.update();
    (app, editor)
}

fn shell_query(app: &App) -> &str {
    &app.world().non_send::<FrontendTrack>().shell.query
}

fn escape_event() -> KeyboardInput {
    KeyboardInput {
        key_code: KeyCode::Escape,
        logical_key: Key::Escape,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    }
}

#[test]
fn native_buffer_attaches_and_edits_commit_to_the_shell() {
    let (mut app, editor) = app_with_shell("alpha");
    assert_eq!(
        app.world()
            .get::<EditableText>(editor)
            .expect("the attach observer installs the buffer")
            .value()
            .to_string(),
        "alpha"
    );

    app.world_mut()
        .get_mut::<EditableText>(editor)
        .expect("the buffer exists")
        .editor_mut()
        .set_text("beta");
    app.update();

    assert_eq!(shell_query(&app), "beta");
}

#[test]
fn shell_writes_reach_the_native_buffer_without_echoing_back() {
    let (mut app, editor) = app_with_shell("alpha");
    let query = "gamma";
    {
        let world = app.world_mut();
        let mut buffer = world
            .get_mut::<EditableText>(editor)
            .expect("the buffer exists");
        sync_search_editor(&mut buffer, query);
    }
    assert_eq!(
        app.world()
            .get::<EditableText>(editor)
            .expect("the buffer exists")
            .value()
            .to_string(),
        query
    );
    app.update();
    assert_eq!(
        shell_query(&app),
        query,
        "a shell-owned write is not committed back as a native edit"
    );
}

#[test]
fn escape_clears_the_query_then_closes_the_native_search() {
    let (mut app, editor) = app_with_shell("alpha");
    app.insert_resource(NativeTextEditing(true));

    app.world_mut().commands().trigger(FocusedInput::new(
        editor,
        escape_event(),
        Entity::PLACEHOLDER,
    ));
    app.update();
    assert_eq!(shell_query(&app), "", "the first Escape clears the query");
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .search_active(),
        "the field stays open after clearing"
    );

    app.world_mut().commands().trigger(FocusedInput::new(
        editor,
        escape_event(),
        Entity::PLACEHOLDER,
    ));
    app.update();
    assert!(
        !app.world()
            .non_send::<FrontendTrack>()
            .shell
            .search_active(),
        "the second Escape closes the field"
    );
}

#[test]
fn escape_is_ignored_without_the_native_text_plugin() {
    let (mut app, editor) = app_with_shell("alpha");
    app.insert_resource(NativeTextEditing(false));
    app.world_mut().commands().trigger(FocusedInput::new(
        editor,
        escape_event(),
        Entity::PLACEHOLDER,
    ));
    app.update();

    assert_eq!(
        shell_query(&app),
        "alpha",
        "the shell-backed fallback owns Escape when the native plugin is absent"
    );
    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .search_active(),
        "the fallback keeps the field open"
    );
}

#[test]
fn services_editor_commits_to_the_services_slot_only() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(crate::input::InputPlugin);
    app.init_resource::<Assets<Font>>();

    let mut shell = ShellApp::new();
    shell.set_query("process query");
    shell.open_search_for(SearchSlot::Services);
    app.insert_non_send(FrontendTrack {
        shell,
        initial_refresh_submitted: true,
        process_tree_expansion: crate::pages::process_tree::ProcessTreeExpansion::default(),
    });
    let editor = app
        .world_mut()
        .spawn((ServicesSearchInput, SearchEditor(String::new())))
        .id();
    app.update();

    app.world_mut()
        .get_mut::<EditableText>(editor)
        .expect("the buffer exists")
        .editor_mut()
        .set_text("Network");
    app.update();

    let track = app.world().non_send::<FrontendTrack>();
    assert_eq!(track.shell.services_query, "Network");
    assert_eq!(
        track.shell.query, "process query",
        "the Services editor never writes the process query"
    );

    app.insert_resource(NativeTextEditing(true));
    app.world_mut().commands().trigger(FocusedInput::new(
        editor,
        escape_event(),
        Entity::PLACEHOLDER,
    ));
    app.update();

    let track = app.world().non_send::<FrontendTrack>();
    assert_eq!(
        track.shell.services_query, "",
        "the first Escape clears the Services query"
    );
    assert!(track.shell.search_active(), "the field stays open");
    assert_eq!(track.shell.query, "process query");

    app.world_mut().commands().trigger(FocusedInput::new(
        editor,
        escape_event(),
        Entity::PLACEHOLDER,
    ));
    app.update();
    assert!(
        !app.world()
            .non_send::<FrontendTrack>()
            .shell
            .search_active(),
        "the second Escape closes the field"
    );
}
