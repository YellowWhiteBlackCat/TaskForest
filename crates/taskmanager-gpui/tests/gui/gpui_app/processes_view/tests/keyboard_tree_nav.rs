//! Apps-table bare Left/Right tree navigation (iced parity), driven through
//! the real click → focus → key-dispatch pipeline.
//!
//! The iced frontend pins the same matrix in
//! `tests/gui/app/tests/visual_navigation.rs`
//! (`visual_left_right_toggles_category_tree_subtrees_and_left_goes_up_to_parent`);
//! these tests are the GPUI side of that contract: Left collapses, Right
//! expands, Left on an already-collapsed row climbs to the nearest visible
//! selectable ancestor, leaf rows and Alt/Shift keep column stepping, and the
//! structural action resolves the LIVE selection (not the focused row).
//!
//! Cadence note: key dispatches must NOT be interleaved with `draw` — a full
//! redraw drops the focused row, so a following arrow would fall through to
//! the root router. Draw only when reading painted bounds; virtualized rows
//! can lag a count change by one frame, hence the double draw there.

use super::*;

fn seed_three_level_tree(
    cx: &mut TestAppContext,
    view: &Entity<RootView>,
    win: WindowHandle<RootView>,
) {
    let unknown = |pid: u32, parent: Option<u32>, name: &str| {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .parent_pid(parent)
            .name(name.to_owned())
            .status("S".to_owned())
            .build()
    };
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.replace_processes_for_test(vec![
            unknown(100, None, "tree-root-worker"),
            unknown(101, Some(100), "tree-child-worker"),
            unknown(102, Some(101), "tree-grandchild-worker"),
        ]);
        cx.notify();
    });
    draw(cx, win);
}

fn press(cx: &mut TestAppContext, win: WindowHandle<RootView>, key: &str) {
    cx.dispatch_keystroke(win.into(), Keystroke::parse(key).unwrap());
}

/// `debug_bounds` takes a `&'static str`, so the small fixed row universe of
/// this fixture is matched to literal selectors instead of formatting keys.
const fn row_bounds_key(index: usize) -> &'static str {
    match index {
        0 => "tm-proc-row-root:0",
        1 => "tm-proc-row-root:1",
        2 => "tm-proc-row-root:2",
        3 => "tm-proc-row-root:3",
        _ => panic!("fixture only renders rows 0..=3"),
    }
}

fn click_row(vcx: &mut VisualTestContext, index: usize) {
    let bounds = vcx
        .debug_bounds(row_bounds_key(index))
        .unwrap_or_else(|| panic!("row {index} must render"));
    let position = bounds.center();
    vcx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 1,
    });
}

fn assert_selected(cx: &mut TestAppContext, view: &Entity<RootView>, key: ProcessRowKey) {
    assert_eq!(
        view.read_with(cx, |v, _| v.selected_process_row()),
        Some(key)
    );
}

/// Mirror of iced's
/// `visual_left_right_toggles_category_tree_subtrees_and_left_goes_up_to_parent`:
/// the rows are 0 = expanded Uncategorized header, then the fully expanded
/// tree 100 → 101 → 102. Right/Left toggle subtrees with the selection held,
/// and a second Left on a collapsed row climbs to its parent.
#[gpui::test]
async fn bare_left_right_runs_the_tree_matrix_on_the_category_tree(cx: &mut TestAppContext) {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let (win, view) = wrapped_root(cx);
    seed_three_level_tree(cx, &view, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);

    click_row(&mut vcx, 1);
    assert_selected(cx, &view, ProcessRowKey::Process(100));

    // Handler-liveness probes: Alt+Left/Right must reach the focused row's
    // handler and step the column cursor both ways (Name → User → Name).
    press(cx, win, "alt-right");
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::User,
        "Alt+Right reaches the focused row handler after click"
    );
    press(cx, win, "alt-left");
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::Name,
        "Alt+Left steps the column cursor back"
    );

    // Right on the already-expanded root: an honest no-op — the selection
    // holds and nothing enters the collapsed set.
    press(cx, win, "right");
    assert_selected(cx, &view, ProcessRowKey::Process(100));
    assert!(
        view.read_with(cx, |v, _| v.processes_state.collapsed.is_empty()),
        "Right on an expanded row must not collapse anything"
    );

    // Down to the child; Left collapses ITS subtree while the selection stays
    // on the collapsed row.
    press(cx, win, "down");
    assert_selected(cx, &view, ProcessRowKey::Process(101));
    // The handler must still own the keyboard after the selection moved.
    press(cx, win, "alt-right");
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::User,
        "Alt+Right still reaches the row handler after Down moved the selection"
    );
    press(cx, win, "alt-left");
    press(cx, win, "left");
    assert_selected(cx, &view, ProcessRowKey::Process(101));
    assert!(
        view.read_with(cx, |v, _| v.processes_state.collapsed.contains(&101)),
        "Left must record 101 in the collapsed set"
    );
    // The authoritative projection drops the hidden grandchild row entirely.
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        assert_eq!(
            rows.iter().map(|r| r.process_pid).collect::<Vec<_>>(),
            [None, Some(100), Some(101)],
            "Left on the expanded child hides the grandchild row"
        );
        assert!(rows[2].collapsed, "the acted-on row renders collapsed");
    });

    // Left again: the child is already collapsed, so the selection climbs to
    // its parent (100).
    press(cx, win, "left");
    // "Left on a collapsed node moves the selection to the parent"
    assert_selected(cx, &view, ProcessRowKey::Process(100));

    // Down + Right re-expands the child's subtree.
    press(cx, win, "down");
    press(cx, win, "right");
    assert!(
        !view.read_with(cx, |v, _| v.processes_state.collapsed.contains(&101)),
        "Right on the collapsed child re-expands its subtree"
    );
    assert_selected(cx, &view, ProcessRowKey::Process(101));
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        assert_eq!(rows.len(), 4, "the re-expanded subtree restores all rows");
        assert_eq!(rows[3].process_pid, Some(102));
    });
}

/// The structural action resolves the LIVE selection, not the focused row:
/// after End moves the selection to the leaf while row focus stays behind,
/// Left must step the column cursor (leaf semantics) instead of collapsing
/// the focused row's subtree.
#[gpui::test]
async fn structural_keys_act_on_the_live_selection_not_the_focused_row(cx: &mut TestAppContext) {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let (win, view) = wrapped_root(cx);
    seed_three_level_tree(cx, &view, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);

    // Click the LEAF row (102): row focus + selection on the leaf.
    click_row(&mut vcx, 3);
    assert_selected(cx, &view, ProcessRowKey::Process(102));
    press(cx, win, "right");
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::User,
        "Right on a leaf steps the column cursor (Name → User)"
    );

    // Click the subtree root (100): row focus + selection move there.
    click_row(&mut vcx, 1);
    assert_selected(cx, &view, ProcessRowKey::Process(100));

    // End routes through the root command router: selection jumps to the last
    // visible row (the leaf 102) while row focus remains on row 1.
    press(cx, win, "end");
    assert_selected(cx, &view, ProcessRowKey::Process(102));

    press(cx, win, "left");
    assert_selected(cx, &view, ProcessRowKey::Process(102));
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::Name,
        "Left on the live leaf selection steps the column cursor back"
    );
    assert!(
        view.read_with(cx, |v, _| v.processes_state.collapsed.is_empty()),
        "Left must act on the live leaf, never collapse the focused row's subtree"
    );
}

/// Alt/Shift keep reserving Left/Right for column navigation even when the
/// selected row owns a subtree — the column modifier branch precedes the
/// structural resolver.
#[gpui::test]
async fn alt_right_keeps_column_stepping_on_a_subtree_row(cx: &mut TestAppContext) {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let (win, view) = wrapped_root(cx);
    seed_three_level_tree(cx, &view, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);

    click_row(&mut vcx, 1);
    press(cx, win, "alt-right");
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::User,
        "Alt+Right steps the column cursor on a subtree row"
    );
    assert!(
        view.read_with(cx, |v, _| v.processes_state.collapsed.is_empty()),
        "Alt+Right must not collapse the selected row's subtree"
    );
    // "column navigation preserves the selected row"
    assert_selected(cx, &view, ProcessRowKey::Process(100));
}
