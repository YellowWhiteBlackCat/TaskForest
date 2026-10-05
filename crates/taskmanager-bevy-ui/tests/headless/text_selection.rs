//! test-intent: behavior
//!
//! Behavior tests for Bevy UI text selection explicit state transitions,
//! clipboard export, and visual highlight component toggling.

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::picking::hover::PickingInteraction;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{ProcessItem, ProcessScalarObservations};
use taskmanager_shell::FeedbackSeverity;
use taskmanager_shell::{FeedbackSource, ShellApp, fixture};

use super::{
    ClipboardPort, SelectableText, TextSelectionChanged, TextSelectionHighlight,
    TextSelectionSession, TextSelectionState, copy_selection_or_row,
};

fn token_process(pid: u32, name: &str) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10_000, 1),
        ..Default::default()
    });
    process
}

fn shell_with_selection() -> ShellApp {
    let mut shell = ShellApp::new();
    fixture::edit_processes(&mut shell, |processes| {
        *processes = Some(vec![
            token_process(100, "alpha"),
            token_process(200, "beta"),
        ])
    });
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    shell
}

impl ClipboardPort {
    pub(crate) fn get_text(&self) -> Option<&str> {
        self.contents.as_deref()
    }
}

#[test]
fn text_selection_state_transitions_are_explicit_and_discrete() {
    let mut state = TextSelectionState::default();
    assert!(!state.is_active());
    assert_eq!(state.session, None);
    assert_eq!(state.selected_text(), None);

    let target = Entity::from_raw_u32(101).unwrap();
    state.select_range(
        target,
        "sys-cpu-model".into(),
        "AMD Ryzen 9 7950X".into(),
        4..9,
    );
    assert!(state.is_active());
    assert_eq!(
        state.session,
        Some(TextSelectionSession {
            target,
            id: "sys-cpu-model".into(),
            content: "AMD Ryzen 9 7950X".into(),
            range: 4..9,
        })
    );
    assert_eq!(state.selected_text(), Some("Ryzen"));

    state.select_all(target, "sys-cpu-model".into(), "AMD Ryzen 9 7950X".into());
    assert_eq!(state.selected_text(), Some("AMD Ryzen 9 7950X"));

    state.clear();
    assert!(!state.is_active());
    assert_eq!(state.session, None);
    assert_eq!(state.selected_text(), None);
}

#[test]
fn copy_selection_or_row_prioritizes_text_selection_over_row_summary() {
    set_language(Language::En);
    let mut shell = shell_with_selection();
    assert!(shell.selected_row_summary().is_some());

    let mut state = TextSelectionState::default();
    let mut clipboard = ClipboardPort::default();

    // 1. When text selection is active, copy selected text
    let target = Entity::from_raw_u32(1).unwrap();
    state.select_range(
        target,
        "test-id".into(),
        "Selected fact content".into(),
        0..8,
    );

    let copied = copy_selection_or_row(&mut state, &mut clipboard, &mut shell);
    assert!(copied);
    assert!(
        clipboard.get_text().is_none(),
        "queueing cannot claim a completed write"
    );
    let mut written = Vec::new();
    super::flush_clipboard(&mut clipboard, &mut shell, |text| {
        written.push(text.to_owned());
        Ok(())
    });
    assert_eq!(written, ["Selected"]);
    assert_eq!(clipboard.get_text(), Some("Selected"));
    let notice = shell.feedback_notice().expect("feedback recorded");
    assert_eq!(notice.source(), FeedbackSource::Clipboard);
    assert!(notice.text().contains("Selected Text"));

    // 2. When text selection is inactive, fallback to selected row summary
    state.clear();
    let copied_row = copy_selection_or_row(&mut state, &mut clipboard, &mut shell);
    assert!(copied_row);
    super::flush_clipboard(&mut clipboard, &mut shell, |_text| Ok(()));
    assert_eq!(
        clipboard.get_text(),
        shell.selected_row_summary().as_deref()
    );
    let notice = shell.feedback_notice().expect("feedback recorded");
    assert_eq!(notice.source(), FeedbackSource::Clipboard);
    assert!(notice.text().contains("Selected Row"));

    // 3. When neither is available, return false without side effect
    let mut empty_shell = ShellApp::new();
    let mut empty_clipboard = ClipboardPort::default();
    let copied_none = copy_selection_or_row(&mut state, &mut empty_clipboard, &mut empty_shell);
    assert!(!copied_none);
    assert_eq!(empty_clipboard.get_text(), None);
    assert!(empty_shell.feedback_notice().is_none());
}

#[test]
fn text_selection_picking_mounts_and_despawns_highlight() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    super::register(&mut app);

    let entity = app
        .world_mut()
        .spawn((
            SelectableText::new("cpu-fact", "AMD Ryzen"),
            PickingInteraction::None,
        ))
        .id();

    app.update();

    let world = app.world_mut();
    let highlighted = world
        .query_filtered::<Entity, With<TextSelectionHighlight>>()
        .iter(world)
        .count();
    assert_eq!(highlighted, 0, "initially unhighlighted");

    // 1. Pointer press establishes selection and triggers highlight observer
    *app.world_mut()
        .get_mut::<PickingInteraction>(entity)
        .unwrap() = PickingInteraction::Pressed;
    app.update();

    assert!(app.world().resource::<TextSelectionState>().is_active());
    let world = app.world_mut();
    let highlighted = world
        .query_filtered::<Entity, With<TextSelectionHighlight>>()
        .iter(world)
        .count();
    assert_eq!(highlighted, 1, "highlight component mounted");

    // 2. Clear selection removes highlight
    app.world_mut().resource_mut::<TextSelectionState>().clear();
    app.world_mut().commands().trigger(TextSelectionChanged);
    app.update();

    assert!(!app.world().resource::<TextSelectionState>().is_active());
    let world = app.world_mut();
    let highlighted = world
        .query_filtered::<Entity, With<TextSelectionHighlight>>()
        .iter(world)
        .count();
    assert_eq!(highlighted, 0, "highlight component removed");
}

#[test]
fn rejected_clipboard_output_remains_a_failure_and_never_becomes_last_copied_text() {
    let mut shell = ShellApp::new();
    let mut clipboard = ClipboardPort::default();
    clipboard.request_text("private descriptor", "Setup");
    super::flush_clipboard(&mut clipboard, &mut shell, |_text| {
        Err("clipboard unavailable".into())
    });
    assert!(clipboard.get_text().is_none());
    let notice = shell.feedback_notice().expect("write failure");
    assert_eq!(notice.severity(), FeedbackSeverity::Error);
    assert!(notice.text().contains("clipboard unavailable"));
}
