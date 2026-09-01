//! Headless behavior tests for the Processes table chrome.
//!
//! The pure tests pin the header-navigation semantics (visible-column
//! projection + wrap-around stepping); the `#[gpui::test]` exercises the real
//! key-dispatch pipeline — focus a sort-header cell via the Tab-stop cycle,
//! dispatch ArrowLeft/ArrowRight, and assert the `RootView` sort state moved.
//! No pixels, no filesystem, no process signals.

#[path = "tests/category.rs"]
mod category;
#[path = "tests/keyboard_tree_nav.rs"]
mod keyboard_tree_nav;
#[path = "tests/landscape.rs"]
mod landscape;
#[path = "tests/projection.rs"]
mod projection;
#[path = "tests/projection_cache.rs"]
mod projection_cache;
#[path = "tests/readability.rs"]
mod readability;
#[path = "tests/scroll_behavior.rs"]
mod scroll_behavior;

use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, Keystroke, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, WindowHandle, div, px, size,
};
use std::rc::Rc;
use taskmanager_core::core::process::ProcessLiveKey;

use crate::gpui_app::root::{RootView, TopPage};
use taskmanager_core::core::process::{
    ApplicationIconAsset, ApplicationIconFormat, ProcessApplicationIdentity,
    ProcessMetadataObservation,
};
/// The expected row id of one fixture process (token from
/// `fixture_start_token`, the builder's single source).
fn row_id(pid: u32) -> taskmanager_shell::ProcessRowId {
    taskmanager_shell::ProcessRowId::Process(
        ProcessLiveKey::from_parts(pid, taskmanager_test_support::fixture_start_token(pid))
            .expect("fixture pid and token are non-zero"),
    )
}

use taskmanager_shell::SortCol;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{RowDensity, UiSize};

fn wrapped_root(cx: &mut TestAppContext) -> (WindowHandle<RootView>, Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

fn draw(cx: &mut TestAppContext, win: WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// Move focus through the Apps-page tab stops until a sort-header cell is
/// reached. Probed by pressing ArrowRight after each `focus_next()`: in Flat
/// mode the only focused element that reacts to "right" before the row list is
/// a header cell (rows are arrow-inert in Flat; pills/buttons/search don't
/// switch the sort column). Each probe press advances the sort column once,
/// which the caller resets before asserting the real sequences.
fn focus_sort_header_cell(
    cx: &mut TestAppContext,
    win: WindowHandle<RootView>,
    view: &Entity<RootView>,
) {
    for _ in 0..64 {
        win.update(cx, |_root, window, _cx| window.focus_next())
            .unwrap();
        let before = view.read_with(cx, |v, _cx| v.process_sort().0);
        cx.dispatch_keystroke(win.into(), Keystroke::parse("right").unwrap());
        let after = view.read_with(cx, |v, _cx| v.process_sort().0);
        if after != before {
            return;
        }
    }
    panic!("no sort-header cell reachable in the Apps-page tab order");
}

/// ArrowLeft/ArrowRight on a focused header cell switch the sort column through
/// the real key-dispatch pipeline, preserving the sort direction, skipping
/// hidden columns, and wrapping at the header ends.
#[gpui::test]
async fn header_arrow_keys_switch_the_sort_column(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        // This test walks the FULL header (Cpu → Memory → … → Nice → Name), so
        // show every column regardless of the (MC-style) default hide set.
        v.processes_state.hidden_cols.clear();
        v.replace_processes_for_test(
            (1..=6)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    draw(cx, win);
    focus_sort_header_cell(cx, win, &view);

    // Right walks the visible columns in canonical order; direction is kept.
    view.update(cx, |v, cx| {
        v.set_process_sort(SortCol::Cpu, taskmanager_shell::SortDir::Desc);
        cx.notify();
    });
    for (key, expected) in [
        ("right", SortCol::Memory),
        ("right", SortCol::Swap),
        ("right", SortCol::DiskRead),
        ("right", SortCol::DiskWrite),
        ("right", SortCol::CpuTime),
        ("right", SortCol::Fds),
        ("right", SortCol::Nice),
        ("right", SortCol::Name),
    ] {
        cx.dispatch_keystroke(win.into(), Keystroke::parse(key).unwrap());
        assert_eq!(
            view.read_with(cx, |v, _cx| v.process_sort().0),
            expected,
            "ArrowRight from Cpu must land on {expected:?}"
        );
        assert!(
            !view.read_with(cx, |v, _cx| matches!(
                v.process_sort().1,
                taskmanager_shell::SortDir::Asc
            )),
            "Arrow-key navigation must preserve the sort direction, not flip it"
        );
    }

    // Left walks back and wraps to the last column.
    for (key, expected) in [
        ("left", SortCol::Nice),
        ("left", SortCol::Fds),
        ("left", SortCol::CpuTime),
        ("left", SortCol::DiskWrite),
        ("left", SortCol::DiskRead),
    ] {
        cx.dispatch_keystroke(win.into(), Keystroke::parse(key).unwrap());
        assert_eq!(
            view.read_with(cx, |v, _cx| v.process_sort().0),
            expected,
            "ArrowLeft from Name must land on {expected:?}"
        );
    }

    // The direction really is preserved: flip sort_asc, step right, assert both.
    view.update(cx, |v, cx| {
        v.set_process_sort(SortCol::Cpu, taskmanager_shell::SortDir::Asc);
        cx.notify();
    });
    cx.dispatch_keystroke(win.into(), Keystroke::parse("right").unwrap());
    assert_eq!(
        view.read_with(cx, |v, _cx| v.process_sort().0),
        SortCol::Memory
    );
    assert!(view.read_with(cx, |v, _cx| matches!(
        v.process_sort().1,
        taskmanager_shell::SortDir::Asc
    )));

    // Hidden columns are skipped by the navigation (same projection as the
    // rendered header). Hide Memory + Swap + DiskWrite: Cpu's right neighbor is
    // DiskRead and its left neighbor is Status.
    view.update(cx, |v, cx| {
        v.set_process_sort(SortCol::Cpu, v.process_sort().1);
        v.processes_state.hidden_cols.insert(SortCol::Memory);
        v.processes_state.hidden_cols.insert(SortCol::Swap);
        v.processes_state.hidden_cols.insert(SortCol::DiskWrite);
        cx.notify();
    });
    cx.dispatch_keystroke(win.into(), Keystroke::parse("right").unwrap());
    assert_eq!(
        view.read_with(cx, |v, _cx| v.process_sort().0),
        SortCol::DiskRead,
        "Right from Cpu must skip the hidden Memory column"
    );
    cx.dispatch_keystroke(win.into(), Keystroke::parse("right").unwrap());
    assert_eq!(
        view.read_with(cx, |v, _cx| v.process_sort().0),
        SortCol::CpuTime,
        "Right must skip the hidden DiskWrite column"
    );
    view.update(cx, |v, cx| {
        v.set_process_sort(SortCol::Cpu, v.process_sort().1);
        cx.notify();
    });
    cx.dispatch_keystroke(win.into(), Keystroke::parse("left").unwrap());
    assert_eq!(
        view.read_with(cx, |v, _cx| v.process_sort().0),
        SortCol::State,
        "Left from Cpu must skip the hidden Memory column"
    );
}

/// Render-path assertion (后置): when process data is present, the virtualized
/// list must actually paint rows with sane geometry. Catches "data arrived but
/// the UI shows nothing" regressions (uniform_list collapsed to zero rows).
#[gpui::test]
async fn process_rows_render_with_expected_geometry(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.replace_processes_for_test(
            (1..=5)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let r0 = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("row 0 must render when processes exist");
    assert!(
        r0.size.height > gpui::px(20.0),
        "process row height collapsed: {r0:?}"
    );
    let r4 = vcx
        .debug_bounds("tm-proc-row-root:4")
        .expect("row 4 must render when processes exist");
    assert!(
        r4.origin.y > r0.origin.y,
        "rows must stack vertically: {r0:?} vs {r4:?}"
    );
}

/// Search highlighting is a paint concern and must not change the row's text
/// layout.  This specifically exercises a long process name with the same
/// one-character query shown in the desktop regression: the name cell must
/// remain one row high after the query turns on the highlight ranges.
#[gpui::test]
async fn process_search_highlight_keeps_name_row_geometry_stable(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.replace_processes_for_test(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(4242)
                .name("taskforest-gui-long-process-name".into())
                .build(),
        ]);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let plain_row = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the unfiltered process row must render");
    let plain_name = vcx
        .debug_bounds("tm-proc-b-name")
        .expect("the unfiltered process name cell must render");

    view.update(cx, |v, cx| {
        v.set_process_query("f");
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());

    let highlighted_row = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the matching process row must remain visible");
    let highlighted_name = vcx
        .debug_bounds("tm-proc-b-name")
        .expect("the highlighted process name cell must remain visible");
    assert_eq!(
        plain_row.size.height, highlighted_row.size.height,
        "search highlighting must not make a process row wrap: plain={plain_row:?}, highlighted={highlighted_row:?}"
    );
    assert_eq!(
        plain_name.size.height, highlighted_name.size.height,
        "search highlighting must not change the process name cell height: plain={plain_name:?}, highlighted={highlighted_name:?}"
    );
    assert!(
        highlighted_name.size.width > px(0.0),
        "the highlighted process name cell must retain a usable width: {highlighted_name:?}"
    );
}

/// Compact Apps chrome keeps one primary action row and moves secondary
/// commands into an anchored menu. This preserves the command vocabulary while
/// returning scarce vertical space to the process table.
#[gpui::test]
async fn compact_apps_action_bar_prioritizes_the_table_without_hiding_commands(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.replace_processes_for_test(
            (1..=4)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("compact-worker-{pid}"))
                        .scalar_observations(
                            taskmanager_core::core::process::ProcessScalarObservations {
                                start_token: taskmanager_core::core::ScalarObservation::available(
                                    u64::from(pid),
                                    1,
                                ),
                                ..Default::default()
                            },
                        )
                        .build()
                })
                .collect(),
        );
        let identity = ProcessLiveKey::from_parts(1, 1).expect("fixture identity");
        v.select_process_single(identity);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let action = vcx
        .debug_bounds("tm-proc-action-bar")
        .expect("the Apps action bar must expose its measured compact bounds");
    let surface = vcx
        .debug_bounds("tm-proc-action-surface")
        .expect("the Apps action bar must expose a bounded visual surface");
    let divider = vcx
        .debug_bounds("tm-proc-action-divider")
        .expect("the Apps action bar must expose a scoped action divider");
    let modes = vcx
        .debug_bounds("tm-proc-mode-switcher")
        .expect("the mode switcher must render below the action bar");
    let filters = vcx
        .debug_bounds("tm-proc-status-filter")
        .expect("the status filter must render below the action bar");
    let table = vcx
        .debug_bounds("tm-procs-table-scroll")
        .expect("the compact Apps table must retain its primary content slot");
    let overflow = vcx
        .debug_bounds("tm-proc-actions-trigger")
        .expect("secondary process commands must remain reachable from compact chrome");
    assert!(
        modes.origin.y >= action.origin.y + action.size.height - px(0.5),
        "mode switcher overlaps the compact action bar: action={action:?}, modes={modes:?}"
    );
    assert!(
        surface.size.height >= action.size.height + px(4.0),
        "action surface must reserve its padding around compact controls: action={action:?}, surface={surface:?}"
    );
    assert!(
        surface.origin.y <= action.origin.y
            && surface.origin.y + surface.size.height >= action.origin.y + action.size.height,
        "action surface must contain the full compact strip: action={action:?}, surface={surface:?}"
    );
    assert!(
        filters.origin.y >= action.origin.y + action.size.height - px(0.5),
        "status filter overlaps the compact action bar: action={action:?}, filters={filters:?}"
    );
    assert!(
        filters.origin.x + filters.size.width <= px(720.5),
        "compact status filters must remain inside the viewport instead of clipping Zombie/Other: {filters:?}"
    );
    assert!(
        divider.size.height <= px(24.5),
        "action divider must stay within one control row: {divider:?}"
    );
    assert!(
        action.size.height <= px(40.0) && surface.size.height <= px(46.0),
        "compact actions must occupy one control row, not the retired three-line strip: action={action:?}, surface={surface:?}"
    );
    assert!(
        table.size.height >= px(180.0),
        "the table must regain useful compact height after secondary actions move to overflow: {table:?}"
    );
    assert!(
        overflow.origin.y >= action.origin.y - px(0.5)
            && overflow.bottom() <= action.bottom() + px(0.5),
        "the overflow trigger must belong to the same primary action row: action={action:?}, overflow={overflow:?}"
    );

    vcx.simulate_click(overflow.center(), Modifiers::none());
    drop(vcx);
    draw(cx, win);
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("the compact actions trigger must open its anchored menu");
    let force_kill = gpui::point(popup.left() + px(40.0), popup.top() + px(17.0));
    vcx.simulate_mouse_move(force_kill, None::<MouseButton>, Modifiers::none());
    vcx.simulate_click(force_kill, Modifiers::none());
    assert_eq!(
        view.read_with(cx, |v, _| {
            v.process_batch_confirmation().map(|intent| intent.action)
        }),
        Some(taskmanager_core::core::process::ProcessBatchAction::Kill),
        "a secondary destructive command must remain reachable through the real compact menu"
    );
}

/// A primary double-click on an Apps aggregate row must use the same expansion
/// projection as the chevron and directional keys. This drives GPUI's real
/// mouse event path with `click_count = 2`, then proves both the RootView state
/// and the rendered visible-row projection changed in both directions.
#[gpui::test]
async fn mc03_apps_doubleclick_case_apps_group_double_click_expands_and_collapses_the_row(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.expanded_apps.clear();
        v.replace_processes_for_test(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(101)
                .name("firefox".into())
                .current_cpu_percentage(4.0)
                .status("R".into())
                .build(),
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(102)
                .name("firefox".into())
                .current_cpu_percentage(2.0)
                .status("S".into())
                .build(),
        ]);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let collapsed = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the collapsed Firefox aggregate row must render");
    assert!(
        vcx.debug_bounds("tm-proc-row-root:1").is_none(),
        "aggregate children must not render while the group is collapsed"
    );

    let position = collapsed.center();
    vcx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
    });
    assert!(
        view.read_with(cx, |v, _cx| v
            .processes_state
            .expanded_apps
            .contains("category:uncategorized")),
        "a primary double-click must expand the category root"
    );

    draw(cx, win);
    assert!(
        vcx.debug_bounds("tm-proc-row-root:1").is_some(),
        "expanded aggregate rows must render their instance child"
    );

    let expanded = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the expanded aggregate row must remain rendered");
    let position = expanded.center();
    vcx.simulate_event(MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
    });
    assert!(
        view.read_with(cx, |v, _cx| !v
            .processes_state
            .expanded_apps
            .contains("category:uncategorized")),
        "a second primary double-click must collapse the same category root"
    );
    view.update(cx, |v, _cx| {
        let (rows, _, _) = v.processes_projection();
        assert_eq!(
            rows.len(),
            1,
            "collapsed group projection must contain only its aggregate row"
        );
    });
}

/// Objective header-vs-body alignment: measures the RENDERED bounds (via
/// `debug_bounds`) of the header row + header Name cell against the body row +
/// body Name cell and asserts pixel-exact equality. This replaces "a vision
/// model says it looks aligned" with measured truth — the panic prints both
/// bounds if they diverge, localizing the misalignment.
#[gpui::test]
async fn header_body_columns_align_exactly(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        // Show every column so a cumulative drift would be maximally visible.
        v.processes_state.hidden_cols.clear();
        v.replace_processes_for_test(
            (1..=5)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let hdr_row = vcx
        .debug_bounds("tm-proc-hdr-row")
        .expect("header row renders");
    let body_row = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("body row renders");
    let hdr_name = vcx
        .debug_bounds("tm-proc-h-sort-name")
        .expect("header Name cell renders");
    let body_name = vcx
        .debug_bounds("tm-proc-b-name")
        .expect("body Name cell renders");
    eprintln!(
        "ALIGN row  hdr x={:?} w={:?} | body x={:?} w={:?}",
        hdr_row.origin.x, hdr_row.size.width, body_row.origin.x, body_row.size.width
    );
    eprintln!(
        "ALIGN name hdr x={:?} w={:?} | body x={:?} w={:?}",
        hdr_name.origin.x, hdr_name.size.width, body_name.origin.x, body_name.size.width
    );
    assert_eq!(hdr_row.origin.x, body_row.origin.x, "row left edge");
    assert_eq!(hdr_row.size.width, body_row.size.width, "row width");
    assert_eq!(hdr_name.origin.x, body_name.origin.x, "Name left edge");
    assert_eq!(hdr_name.size.width, body_name.size.width, "Name width");
}

/// Column-band keyboard path: a focused flat process row moves the
/// presentation cursor without changing the shell-owned sort or row
/// selection. This is the replacement for dragging the horizontal scrollbar.
#[gpui::test]
async fn apps_column_navigation_moves_without_losing_row_selection(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.hidden_cols.clear();
        v.set_process_sort(SortCol::Cpu, taskmanager_shell::SortDir::Desc);
        v.replace_processes_for_test(vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(101)
                .name("keyboard-worker".into())
                .build(),
        ]);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let row = vcx
        .debug_bounds("tm-proc-row-root:1")
        .expect("expanded category process row renders");
    let position = row.center();
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
    cx.dispatch_keystroke(win.into(), Keystroke::parse("right").unwrap());
    assert_eq!(
        view.read_with(cx, |v, _| v.processes_state.column_cursor),
        SortCol::User,
        "Right must move the Apps column cursor from Name to User"
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.selected_process_identity()),
        ProcessLiveKey::from_parts(101, 1011),
        "column navigation must preserve the selected process row"
    );
    assert_eq!(
        view.read_with(cx, |v, _| v.process_sort().0),
        SortCol::Cpu,
        "column navigation must not change the active sort column"
    );
}

/// Bisect the collapsed-list regression: a proc_row rendered as a window root
/// (no uniform_list) must keep a sane height. If this passes but the list
/// renders zero rows, the collapse lives in the uniform_list measurement path.
#[gpui::test]
async fn standalone_proc_row_keeps_its_height(cx: &mut TestAppContext) {
    use crate::gpui_app::processes_view::rows::{ProcRowProps, Toggle, VisibleRow, proc_row};

    struct RowHarness {
        row: VisibleRow,
        entity: Entity<RootView>,
    }
    impl Render for RowHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(proc_row(
                ProcRowProps {
                    theme: &Theme::dark(),
                    row: &self.row,
                    row_idx: 0,
                    is_sel: false,
                    is_hov: false,
                    entity: &self.entity,
                    process_identities: Rc::new(vec![
                        ProcessLiveKey::from_parts(4242, 42421).expect("fixture identity"),
                    ]),
                    row_keys: Rc::new(vec![row_id(4242)]),
                    rows: Rc::new(Vec::new()),
                    gray_zero_values: false,
                    density: RowDensity::Comfortable,
                    ui_size: UiSize::Standard,
                },
                &Default::default(),
                &Default::default(),
            ))
        }
    }
    let host = cx.add_window(|_w, cx| {
        let root: Entity<RootView> = cx.new(|cx| RootView::new(Theme::dark(), cx));
        let _ = &root;
        RowHarness {
            row: VisibleRow {
                name: "important-worker".into(),
                selection_key: Some(row_id(4242)),
                process_identity: Some(row_id(4242).live_key().expect("fixture identity")),
                application_identity: Some(
                    ProcessApplicationIdentity::new(
                        "org.example.Editor.desktop",
                        "Example Editor",
                        Some("example-editor".into()),
                    )
                    .expect("fixture identity must be non-empty"),
                ),
                user: "root".into(),
                status: "S".into(),
                cpu: Some(1.5),
                mem: Some(1024),
                cpu_aggregate: None,
                memory_aggregate: None,
                swap: None,
                disk_read: None,
                disk_write: None,
                threads: Some(4),
                start_time_secs: Some(1000),
                cpu_time_secs: Some(500),
                fds: Some(8),
                nice: Some(0),
                cpu_history: std::rc::Rc::from([]),
                name_highlights: Vec::new(),
                cell_text: Default::default(),
                depth: 0,
                has_children: false,
                collapsed: false,
                parent_key: None,
                badge: None,
                toggle: Toggle::None,
            },
            entity: root,
        }
    });
    cx.update_window(host.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(host.into(), cx);
    let r = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("standalone row must render");
    assert!(
        r.size.height > gpui::px(20.0),
        "standalone proc_row collapsed to {r:?}"
    );
    assert!(
        vcx.debug_bounds("tm-proc-app-icon").is_some(),
        "verified application identity must render the generic app glyph"
    );
}

/// A validated provider asset must cross the composition boundary into a real
/// GPUI image element. This deliberately uses the same `Img` path as a live
/// Linux icon asset; the generic glyph test above remains a separate failure
/// fallback assertion.
#[gpui::test]
async fn mc03_app_icon_case_verified_application_asset_mounts_as_a_gpui_image(
    cx: &mut TestAppContext,
) {
    use crate::gpui_app::processes_view::rows::{ProcRowProps, Toggle, VisibleRow, proc_row};

    struct RowHarness {
        row: VisibleRow,
        entity: Entity<RootView>,
    }
    impl Render for RowHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(proc_row(
                ProcRowProps {
                    theme: &Theme::dark(),
                    row: &self.row,
                    row_idx: 0,
                    is_sel: false,
                    is_hov: false,
                    entity: &self.entity,
                    process_identities: Rc::new(vec![
                        ProcessLiveKey::from_parts(4243, 42431).expect("fixture identity"),
                    ]),
                    row_keys: Rc::new(vec![row_id(4243)]),
                    rows: Rc::new(Vec::new()),
                    gray_zero_values: false,
                    density: RowDensity::Comfortable,
                    ui_size: UiSize::Standard,
                },
                &Default::default(),
                &Default::default(),
            ))
        }
    }

    let asset = ApplicationIconAsset::from_bytes(
        ApplicationIconFormat::Svg,
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#4285f4"/></svg>"##.to_vec(),
    )
    .expect("valid SVG icon fixture");
    let identity = ProcessApplicationIdentity::new(
        "org.example.ImageEditor.desktop",
        "Image Editor",
        Some("image-editor".into()),
    )
    .expect("identity fixture")
    .with_icon_resolution(Some(asset), None);
    let host = cx.add_window(|_w, cx| {
        let root: Entity<RootView> = cx.new(|cx| RootView::new(Theme::dark(), cx));
        RowHarness {
            row: VisibleRow {
                name: "image-editor".into(),
                selection_key: Some(row_id(4243)),
                process_identity: Some(row_id(4243).live_key().expect("fixture identity")),
                application_identity: Some(identity),
                user: "root".into(),
                status: "S".into(),
                cpu: Some(1.0),
                mem: Some(1024),
                cpu_aggregate: None,
                memory_aggregate: None,
                swap: None,
                disk_read: None,
                disk_write: None,
                threads: Some(4),
                start_time_secs: Some(1000),
                cpu_time_secs: Some(500),
                fds: Some(8),
                nice: Some(0),
                cpu_history: std::rc::Rc::from([]),
                name_highlights: Vec::new(),
                cell_text: Default::default(),
                depth: 0,
                has_children: false,
                collapsed: false,
                parent_key: None,
                badge: None,
                toggle: Toggle::None,
            },
            entity: root,
        }
    });
    cx.update_window(host.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(host.into(), cx);
    let image = vcx
        .debug_bounds("tm-proc-app-image")
        .expect("validated asset must mount the GPUI image wrapper");
    assert_eq!(image.size.width, gpui::px(18.0));
    assert_eq!(image.size.height, gpui::px(18.0));
    assert!(
        vcx.debug_bounds("tm-proc-app-icon").is_none(),
        "asset rows must not take the generic icon path"
    );
}

/// Mechanism probe: a plain div with a debug selector must be measurable.
#[gpui::test]
async fn debug_bounds_probe(cx: &mut TestAppContext) {
    struct Probe;
    impl Render for Probe {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(div().debug_selector(|| "tm-probe".into()).h(gpui::px(28.0)))
        }
    }
    let host = cx.add_window(|_w, _cx| Probe);
    cx.update_window(host.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(host.into(), cx);
    assert!(
        vcx.debug_bounds("tm-probe").is_some(),
        "debug_bounds mechanism must work for a plain div"
    );
}

/// Selection identity = the accent rail: the selected row renders a 4px
/// leading-edge rail at exactly the row's left edge (vertical span = row
/// height), and unselected rows render NO rail at all.
#[gpui::test]
async fn selected_row_paints_accent_rail_at_leading_edge(cx: &mut TestAppContext) {
    use crate::gpui_app::processes_view::rows::{ProcRowProps, Toggle, VisibleRow, proc_row};
    use taskmanager_theme::tokens::SELECTION_RAIL;

    struct SelHarness {
        row: VisibleRow,
        entity: Entity<RootView>,
        selected: bool,
    }
    impl Render for SelHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let t = Theme::dark();
            div().size_full().child(proc_row(
                ProcRowProps {
                    theme: &t,
                    row: &self.row,
                    row_idx: 0,
                    is_sel: self.selected,
                    is_hov: false,
                    entity: &self.entity,
                    process_identities: Rc::new(vec![
                        ProcessLiveKey::from_parts(4242, 42421).expect("fixture identity"),
                    ]),
                    row_keys: Rc::new(vec![row_id(4242)]),
                    rows: Rc::new(Vec::new()),
                    gray_zero_values: false,
                    density: RowDensity::Comfortable,
                    ui_size: UiSize::Standard,
                },
                &Default::default(),
                &Default::default(),
            ))
        }
    }

    let row = || VisibleRow {
        name: "rail-worker".into(),
        selection_key: Some(row_id(4242)),
        process_identity: Some(row_id(4242).live_key().expect("fixture identity")),
        application_identity: None,
        user: "root".into(),
        status: "S".into(),
        cpu: Some(2.0),
        mem: Some(2048),
        cpu_aggregate: None,
        memory_aggregate: None,
        swap: None,
        disk_read: None,
        disk_write: None,
        threads: Some(2),
        start_time_secs: Some(1000),
        cpu_time_secs: Some(100),
        fds: Some(3),
        nice: Some(0),
        cpu_history: std::rc::Rc::from([]),
        name_highlights: Vec::new(),
        cell_text: Default::default(),
        depth: 0,
        has_children: false,
        collapsed: false,
        parent_key: None,
        badge: None,
        toggle: Toggle::None,
    };

    // Selected: rail present, 4px wide, flush with the row's left edge, and
    // spanning the row's full vertical extent.
    let host = cx.add_window(|_w, cx| {
        let root: Entity<RootView> = cx.new(|cx| RootView::new(Theme::dark(), cx));
        SelHarness {
            row: row(),
            entity: root,
            selected: true,
        }
    });
    cx.update_window(host.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(host.into(), cx);
    let row_bounds = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("selected row renders");
    let rail = vcx
        .debug_bounds("tm-proc-rail")
        .expect("selected row must paint the accent rail");
    assert_eq!(
        f32::from(rail.size.width),
        f32::from(SELECTION_RAIL),
        "rail must be exactly the SELECTION_RAIL width"
    );
    assert!(
        (rail.origin.x - row_bounds.origin.x).abs() <= gpui::px(0.5),
        "rail must sit flush with the row's left edge: {rail:?} vs {row_bounds:?}"
    );
    assert!(
        rail.size.height >= row_bounds.size.height - gpui::px(0.5),
        "rail must span the row height: {rail:?} vs {row_bounds:?}"
    );

    // Unselected: no rail anywhere in the tree.
    drop(vcx);
    let host = cx.add_window(|_w, cx| {
        let root: Entity<RootView> = cx.new(|cx| RootView::new(Theme::dark(), cx));
        SelHarness {
            row: row(),
            entity: root,
            selected: false,
        }
    });
    cx.update_window(host.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(host.into(), cx);
    assert!(
        vcx.debug_bounds("tm-proc-rail").is_none(),
        "unselected rows must not paint the accent rail"
    );
}
