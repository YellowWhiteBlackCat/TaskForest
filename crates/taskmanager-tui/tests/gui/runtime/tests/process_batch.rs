//! Process batch-control tests: the extended action menu (Suspend / Resume /
//! Kill / priority), the gated batch confirmation, and the multi-select mark
//! key.

use super::super::*;

use crate::ui::process_menu::ProcessMenuAction;
use taskmanager_application::AppAction;
use taskmanager_application::PriorityTier;
use taskmanager_application::ProcessBatchAction;

fn open_process_menu(app: &mut TuiApp) {
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('a'),
            KeyModifiers::NONE,
        ),
    );
    assert!(app.process_menu().is_some(), "a must open the action menu");
}

#[test]
fn process_menu_offers_the_batch_control_vocabulary() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    open_process_menu(&mut app);
    let labels: Vec<&'static str> = crate::ui::process_menu::MENU_ACTIONS
        .into_iter()
        .map(crate::ui::process_menu::action_label)
        .collect();
    assert_eq!(labels.len(), 10);
    assert!(labels[0].contains("End task"));
    assert!(
        labels[1].contains("End process tree"),
        "the tree-end affordance must be offered (GPUI/Iced parity): {labels:?}"
    );
    assert!(labels[2].contains("Suspend"));
    assert!(labels[3].contains("Resume"));
    assert!(labels[4].contains("Force kill"));
    // The three tier rows read the shared tier→label fold (§4.0 同一律):
    // High / Normal / Low, never a bare "Priority" row hiding the tier.
    assert_eq!(labels[5], "High");
    assert_eq!(labels[6], "Normal");
    assert_eq!(labels[7], "Low");
    assert_eq!(
        crate::ui::process_menu::priority_tier(
            crate::ui::process_menu::ProcessMenuAction::PriorityHigh
        ),
        Some(PriorityTier::High)
    );
    assert_eq!(
        crate::ui::process_menu::priority_tier(
            crate::ui::process_menu::ProcessMenuAction::PriorityLow
        ),
        Some(PriorityTier::Low)
    );
}

#[test]
fn suspend_resume_and_priority_emit_execute_batch_directly() {
    for (action, expected_tier) in [
        (ProcessMenuAction::Suspend, None),
        (ProcessMenuAction::Resume, None),
        (ProcessMenuAction::PriorityHigh, Some(PriorityTier::High)),
    ] {
        let mut app = crate::demo_app();
        open_process_menu(&mut app);
        let index = crate::ui::process_menu::MENU_ACTIONS
            .iter()
            .position(|candidate| *candidate == action)
            .expect("action in menu");
        menu_index_to(&mut app, index);
        let effect = handle_key(
            &mut app,
            KeyEvent::new(
                ratatui::crossterm::event::KeyCode::Enter,
                KeyModifiers::NONE,
            ),
        );
        let Some(PlatformEffect::ExecuteBatch(intent)) = effect else {
            panic!("{action:?} must emit ExecuteBatch directly");
        };
        let expected = match (action, expected_tier) {
            (ProcessMenuAction::Suspend, _) => ProcessBatchAction::Suspend,
            (ProcessMenuAction::Resume, _) => ProcessBatchAction::Resume,
            (ProcessMenuAction::PriorityHigh, Some(tier)) => ProcessBatchAction::SetPriority(tier),
            _ => panic!("unexpected action"),
        };
        assert_eq!(intent.action, expected);
        assert!(!intent.targets.is_empty(), "the frozen target set");
    }
}

fn menu_index_to(app: &mut TuiApp, index: usize) {
    while app
        .process_menu()
        .is_some_and(|menu| menu.selection < index)
    {
        let _ = handle_key(
            app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
}

#[test]
fn kill_gates_behind_the_batch_confirmation_and_y_confirms() {
    let mut app = crate::demo_app();
    open_process_menu(&mut app);
    let index = crate::ui::process_menu::MENU_ACTIONS
        .iter()
        .position(|candidate| *candidate == ProcessMenuAction::Kill)
        .expect("kill in menu");
    menu_index_to(&mut app, index);
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "Kill is gated, no effect yet");
    assert!(
        app.shell.pending_batch().is_some(),
        "Kill must open the batch confirmation"
    );
    assert!(app.process_menu().is_none(), "the menu closes on pick");

    // y confirms: the ExecuteBatch effect carries the frozen targets.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    );
    let Some(PlatformEffect::ExecuteBatch(intent)) = effect else {
        panic!("confirm must produce an ExecuteBatch effect");
    };
    assert_eq!(intent.action, ProcessBatchAction::Kill);
    assert_eq!(app.shell.pending_batch(), None);
}

#[test]
fn end_process_tree_gates_the_shared_pending_batch_with_a_frozen_tree() {
    let mut app = crate::demo_app();
    open_process_menu(&mut app);
    // Seed an honest fixture tree under the frozen row: root → child →
    // grandchild (the demo list ships with no parent links).
    let root_pid = app.process_menu().expect("menu open").item.pid;
    let mut chain = app
        .shell
        .projection()
        .processes
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|process| process.pid)
        .filter(|pid| *pid != root_pid)
        .collect::<Vec<_>>();
    chain.truncate(2);
    assert_eq!(chain.len(), 2, "the demo fixture must provide two pids");
    let [child_pid, grandchild_pid] = [chain[0], chain[1]];
    let mut processes = app.shell.projection().processes.clone();
    if let Some(processes) = processes.as_deref_mut() {
        for process in processes.iter_mut() {
            match process.pid {
                pid if pid == child_pid => process.parent_pid = Some(root_pid),
                pid if pid == grandchild_pid => process.parent_pid = Some(child_pid),
                _ => {}
            }
        }
    }
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(processes),
    );

    let index = crate::ui::process_menu::MENU_ACTIONS
        .iter()
        .position(|candidate| *candidate == ProcessMenuAction::EndProcessTree)
        .expect("end-process-tree in menu");
    menu_index_to(&mut app, index);
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "End process tree is gated, no effect yet");
    assert!(
        app.shell.pending_batch().is_some(),
        "the tree-end must arm the shared batch confirmation"
    );
    // The frozen intent covers the whole tree leaf-first through the shared
    // core traversal.
    let intent = app.shell.pending_batch().expect("pending batch");
    assert_eq!(intent.action, ProcessBatchAction::End);
    assert_eq!(
        intent
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>(),
        vec![grandchild_pid, child_pid, root_pid],
        "the tree intent freezes descendants leaf-first, root last"
    );

    // y confirms: the ExecuteBatch effect carries the frozen tree.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    );
    assert!(
        matches!(effect, Some(PlatformEffect::ExecuteBatch(intent)) if intent.targets.len() == 3),
        "confirm must emit the frozen tree batch"
    );
    assert_eq!(app.shell.pending_batch(), None);
}

#[test]
fn batch_confirmation_n_dismisses_without_an_effect() {
    let mut app = crate::demo_app();
    open_process_menu(&mut app);
    let index = crate::ui::process_menu::MENU_ACTIONS
        .iter()
        .position(|candidate| *candidate == ProcessMenuAction::Kill)
        .expect("kill in menu");
    menu_index_to(&mut app, index);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('n'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none());
    assert_eq!(app.shell.pending_batch(), None);
}

#[test]
fn mark_key_toggles_the_multi_select_set_and_status() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(app.shell.selected_pids().is_empty());

    // Mark the anchor row.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.shell.selected_pids().len(), 1);
    assert!(app.shell.feedback_text().contains("1 processes marked"));

    // Unmark it (toggle off) clears the set and the status.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.shell.selected_pids().len(), 0);
    assert!(app.shell.feedback_text().contains("Selection cleared"));
}

#[test]
fn mark_key_resolves_the_visual_row_pid_in_the_category_tree() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let rows = app.process_rows_snapshot();
    app.selected = rows
        .iter()
        .position(|row| matches!(row, crate::process_view::ProcessRow::TreeNode { .. }))
        .expect("category tree exposes a process row");
    let expected = app
        .selected_detail_process()
        .expect("grouped row resolves to a process")
        .pid;
    assert!(app.shell.selected_pids().is_empty());

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.shell.selected_pids().contains(&expected),
        "m must mark the pid under the grouped cursor ({expected})"
    );
    // A second m on the same row unmarks it.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    assert!(!app.shell.selected_pids().contains(&expected));
}

#[test]
fn shift_arrows_extend_the_marked_range_in_the_category_tree() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let rows = app.process_rows_snapshot();
    app.selected = rows
        .iter()
        .position(|row| matches!(row, crate::process_view::ProcessRow::TreeNode { .. }))
        .expect("category tree exposes a process row");

    // Mark the anchor, then Shift+Down walks the visual rows, keeping every
    // visited process in the set (headers carry no pid and are skipped).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    let marked_after_first = app.shell.selected_pids().len();
    assert_eq!(marked_after_first, 1);

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    assert!(
        app.shell.selected_pids().len() >= 2,
        "Shift+Down must extend the marked set in the category tree"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    assert!(
        app.shell.selected_pids().len() >= 3,
        "repeated Shift+Down keeps extending"
    );
}

#[test]
fn shift_arrows_extend_the_marked_range_and_batches_freeze_the_set() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(
        app.shell.selected_pids().len(),
        2,
        "Shift+Down extends the marked range"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(app.shell.selected_pids().len(), 3);

    // A bare arrow collapses back to the anchor (shared semantics).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(
        app.shell.selected_pids().len(),
        1,
        "bare arrow collapses to the anchor"
    );

    // Shift+Up extends the range toward the start of the list.
    let anchor = app.selected;
    assert!(anchor > 0, "the anchor must not already be the first row");
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Up, KeyModifiers::SHIFT),
    );
    assert_eq!(
        app.shell.selected_pids().len(),
        2,
        "Shift+Up must extend the marked range backward"
    );
    assert_eq!(app.selected, anchor - 1);
    // A bare arrow collapses back to the anchor, restoring the single-pid set
    // the destructive batch below freezes.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
    );
    assert_eq!(app.shell.selected_pids().len(), 1);

    // A destructive batch over the marked set freezes every target.
    let effect = app.shell.request_process_batch(ProcessBatchAction::Kill);
    assert!(effect.is_none(), "Kill gates");
    let intent = app.shell.pending_batch().expect("pending batch");
    assert_eq!(intent.targets.len(), 1, "one pid remains marked");
    let _ = app.shell.confirm_process_batch();
}

#[test]
fn b_key_opens_the_batch_menu_over_the_marked_set() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));

    // No marked rows: B stays closed with an honest status line.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('B'),
            KeyModifiers::SHIFT,
        ),
    );
    assert!(app.batch_menu().is_none(), "B with no marks must not open");
    assert!(
        app.feedback_text().contains("mark"),
        "honest hint: {}",
        app.feedback_text()
    );

    // Mark two rows, then B opens the menu with the frozen count.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    // Shift+Down EXTENDS the marked range (a bare Down would collapse it back
    // to the anchor).
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    assert_eq!(app.shell.selected_pids().len(), 2);
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('B'),
            KeyModifiers::SHIFT,
        ),
    );
    let menu = *app.batch_menu().expect("B opens over marked rows");
    assert_eq!(menu.marked_count, 2, "menu freezes the marked count");
    assert_eq!(
        crate::ui::batch_menu::MENU_ACTIONS.len(),
        8,
        "the full batch vocabulary is offered (three typed priority tiers, \
         matching the GPUI/Iced batch surfaces)"
    );

    // Esc closes without acting.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(app.batch_menu().is_none());
}

#[test]
fn batch_menu_actions_route_through_the_shared_batch_path() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    use crate::ui::batch_menu::BatchMenuAction;
    // Suspend submits ExecuteBatch directly for the whole marked set.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('B'),
            KeyModifiers::SHIFT,
        ),
    );
    let index = crate::ui::batch_menu::MENU_ACTIONS
        .iter()
        .position(|a| *a == BatchMenuAction::Suspend)
        .expect("suspend row");
    for _ in 0..index {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        matches!(effect, Some(PlatformEffect::ExecuteBatch(intent)) if intent.targets.len() == 2),
        "Suspend submits the whole marked set"
    );

    // Kill gates behind the batch confirmation (same as the `a` menu path).
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('B'),
            KeyModifiers::SHIFT,
        ),
    );
    let kill = crate::ui::batch_menu::MENU_ACTIONS
        .iter()
        .position(|a| *a == BatchMenuAction::Kill)
        .expect("kill row");
    for _ in 0..kill {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "Kill gates");
    assert!(
        app.shell.pending_batch().is_some(),
        "pending batch confirmation"
    );
    let _ = app.shell.confirm_process_batch();

    // A priority tier row submits the TYPED tier directly for the whole
    // marked set (the batch surface offers all three tiers, like GPUI's
    // action bar and Iced's pick_list).
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Down,
            KeyModifiers::SHIFT,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('B'),
            KeyModifiers::SHIFT,
        ),
    );
    let high = crate::ui::batch_menu::MENU_ACTIONS
        .iter()
        .position(|a| *a == BatchMenuAction::PriorityHigh)
        .expect("priority-high row");
    for _ in 0..high {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        matches!(
            effect,
            Some(PlatformEffect::ExecuteBatch(intent))
                if intent.action == ProcessBatchAction::SetPriority(PriorityTier::High)
                    && intent.targets.len() == 2
        ),
        "the High tier row must submit SetPriority(High) over the marked set"
    );
}

#[test]
fn batch_menu_clear_empties_the_marked_set() {
    use crate::ui::batch_menu::BatchMenuAction;
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('m'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('B'),
            KeyModifiers::SHIFT,
        ),
    );
    let clear = crate::ui::batch_menu::MENU_ACTIONS
        .iter()
        .position(|a| *a == BatchMenuAction::Clear)
        .expect("clear row");
    for _ in 0..clear {
        let _ = handle_key(
            &mut app,
            KeyEvent::new(ratatui::crossterm::event::KeyCode::Down, KeyModifiers::NONE),
        );
    }
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "Clear emits no platform effect");
    assert_eq!(
        app.shell.selected_pids().len(),
        0,
        "Clear empties the marked set"
    );
}
