//! Row-alignment closed loop (TUI-001): on every table page, the row the
//! renderer paints as selected, the keyboard cursor, the pointer-selected
//! row, and the semantic snapshot's `selected` flag must all name the SAME
//! visual row, and a resize must replace the committed geometry instead of
//! letting a stale plan address a row it never painted.
//!
//! Every test drives the production entries only: a full frame is painted
//! through `render_with_plan` with the exact plan a pointer event would
//! consume, selection moves through `apply_terminal_event_with_plan` (the
//! same dispatch the run loop uses for keys and clicks), and semantic state
//! is read through the published `TuiApp::semantic_snapshot`. Nothing here
//! rebuilds the row projection or the geometry by hand — the painted `› `
//! highlight marker is resolved back through the committed plan's typed
//! HitMap, so renderer and hit-test can only pass by sharing one projection.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use taskmanager_application::{AppAction, AppPage, PlatformEffect};
use taskmanager_ui_contract::SemanticRole;

use crate::runtime::apply_terminal_event_with_plan;
use crate::ui::frame_plan::TABLE_DATA_ROW_OFFSET;
use crate::ui::{TuiFramePlan, TuiHitTarget, render_with_plan};

const FRAME: Rect = Rect::new(0, 0, 120, 40);
/// A taller frame for the resize scenario: the Applications table gains six
/// data rows, so the bounded row window re-centers around the cursor.
const RESIZED: Rect = Rect::new(0, 0, 120, 46);

/// Every page whose panel paints pointer-addressable rows.
const TABLE_PAGES: [AppPage; 4] = [
    AppPage::Applications,
    AppPage::Services,
    AppPage::Startup,
    AppPage::Users,
];

fn click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Paint the whole frame through the exact committed plan a pointer event
/// would consume, and return the frame rows next to that plan. The rows are
/// serialized from the backend buffer cells (not `to_string`, whose 0.30
/// format decorates each line) so a column index in a row string IS the cell
/// x coordinate. Pins English so the marker scan and text needles cannot
/// depend on the host locale.
fn painted(app: &crate::TuiApp, area: Rect) -> (Vec<String>, TuiFramePlan) {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let plan = TuiFramePlan::build(app, area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_with_plan(frame, app, crate::TuiTheme::default(), &plan))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    let lines: Vec<String> = (0..usize::from(buffer.area.height))
        .map(|row| {
            buffer.content[row * width..(row + 1) * width]
                .iter()
                .map(|cell| cell.symbol().to_owned())
                .collect()
        })
        .collect();
    (lines, plan)
}

/// The absolute y of the row the renderer highlighted with the `› ` marker
/// inside the committed plan's painted data rows, or `None` when no selected
/// row is on screen. The scan is bounded to the plan's own bounded window so
/// a marker elsewhere in the frame can never be mistaken for the table's.
fn painted_selected_row(lines: &[String], plan: &TuiFramePlan) -> Option<u16> {
    let panel = plan.table_panel()?;
    let visible = panel.window.end.saturating_sub(panel.window.start);
    (0..visible).find_map(|offset| {
        let y = panel
            .area
            .y
            .saturating_add(TABLE_DATA_ROW_OFFSET)
            .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
        let marked = lines
            .get(usize::from(y))
            .and_then(|line| line.chars().nth(usize::from(panel.area.x + 1)))
            .is_some_and(|cell| cell == '›');
        marked.then_some(y)
    })
}

fn painted_selected_row_or_panic(lines: &[String], plan: &TuiFramePlan, page: AppPage) -> u16 {
    painted_selected_row(lines, plan).unwrap_or_else(|| {
        let panel = plan.table_panel().expect("the page exposes a table panel");
        let scanned: Vec<String> = lines
            [usize::from(panel.area.y)..usize::from(panel.area.y + panel.area.height)]
            .to_vec();
        panic!(
            "{page:?} must paint its `› ` selected row (panel {area:?}, window {window:?}, \
             scanned rows:\n{scanned:#?})",
            area = panel.area,
            window = panel.window,
        )
    })
}

/// The fixture row identity the painted line must show for the visual row at
/// `index`: the pid for process rows and the page's own first-column identity
/// for the flat inventories. Values come from the same accessors the
/// renderers and the seam consume (`process_at`, the sorted page lists), and
/// Applications hierarchy headers — which carry no single-process identity —
/// resolve to an empty needle (only the hit-index contract applies to them).
fn row_identity_needle(app: &crate::TuiApp, page: AppPage, index: usize) -> String {
    match page {
        AppPage::Applications => {
            let rows = app.process_rows_snapshot();
            crate::process_view::process_at(&rows, index)
                .map_or(String::new(), |process| process.pid.to_string())
        }
        AppPage::Services => app.sorted_services()[index].name.clone(),
        AppPage::Startup => app.sorted_startup_entries()[index].name.clone(),
        AppPage::Users => {
            let session = app.sorted_sessions()[index];
            session.tty.clone().unwrap_or_else(|| session.user.clone())
        }
        _ => panic!("{page:?} has no table rows"),
    }
}

/// The stable semantic id the published snapshot must carry for the visual
/// row at `index`. The `row:`/`group-row:` prefixes are the shared
/// `taskmanager-ui-contract` builder's id scheme (one vocabulary for TUI,
/// Iced, and GPUI); this helper only translates the visual row into that
/// contract's identity through the same resolvers the key handler uses,
/// instead of duplicating any TUI-internal projection.
fn expected_semantic_id(app: &crate::TuiApp, index: usize) -> String {
    let rows = app.process_rows_snapshot();
    if let Some(process) = crate::process_view::process_at(&rows, index) {
        return format!("row:{}", process.pid);
    }
    let name = crate::process_view::group_name_at(&rows, index)
        .unwrap_or_else(|| panic!("visual row {index} is outside the Applications projection"));
    format!("group-row:{name}")
}

/// The semantic row identities the published snapshot marks `selected`.
fn semantic_selected_ids(app: &crate::TuiApp) -> Vec<String> {
    let snapshot = app
        .semantic_snapshot()
        .expect("the TUI semantic snapshot must build from the demo fixture");
    snapshot
        .nodes()
        .filter(|node| {
            matches!(node.role(), SemanticRole::Row | SemanticRole::TreeItem)
                && node.state().selected == Some(true)
        })
        .map(|node| node.id().as_str().to_owned())
        .collect()
}

/// (a) Same-source paint/hit lock per table page: the painted `› ` row's
/// first cell resolves through the committed plan to exactly the app's
/// selected row, the painted line carries that row's own identity, and panel
/// chrome (the header line) stays outside the typed row map.
#[test]
fn the_painted_selected_row_resolves_through_the_committed_plan_on_every_table_page() {
    for page in TABLE_PAGES {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let (lines, plan) = painted(&app, FRAME);
        let panel = plan
            .table_panel()
            .unwrap_or_else(|| panic!("{page:?} must expose a table panel"));
        let marker = painted_selected_row_or_panic(&lines, &plan, page);

        assert_eq!(
            plan.hit_target(panel.area.x + 1, marker),
            Some(TuiHitTarget::TableRow {
                page,
                index: app.shell.selected,
            }),
            "{page:?}: the painted highlight cell must hit the app's selected row"
        );
        let needle = row_identity_needle(&app, page, app.shell.selected);
        let line = &lines[usize::from(marker)];
        assert!(
            line.contains(&needle),
            "{page:?}: the painted selected row must show the selected row's identity \
             {needle:?}, got: {line:?}"
        );

        // Panel chrome is not a data row: the header line and the top border
        // must not resolve into a row hit.
        assert_eq!(
            plan.hit_target(panel.area.x + 1, panel.area.y + 1),
            None,
            "{page:?}: the header line is not a pointer-addressable data row"
        );
        assert_eq!(
            plan.hit_target(panel.area.x + 1, panel.area.y),
            None,
            "{page:?}: the panel border is not a data row"
        );
    }
}

/// (b) The production arrow-key path moves the selection, and after the plan
/// is rebuilt and the frame repainted the painted `› ` row has moved with it
/// — the paint, the hit map, and the cursor stay one projection.
#[test]
fn keyboard_selection_moves_the_painted_selected_row_with_it_on_every_table_page() {
    for page in TABLE_PAGES {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let (_lines, plan) = painted(&app, FRAME);
        let before = app.shell.selected;
        let panel = plan.table_panel().expect("{page:?} table panel");

        let down = apply_terminal_event_with_plan(&mut app, key(KeyCode::Down), &plan);
        assert!(
            down.dirty,
            "{page:?}: a selection key must demand a repaint"
        );
        assert!(
            down.effect
                .as_ref()
                .is_none_or(|effect| matches!(effect, PlatformEffect::ProcessInsights(_))),
            "{page:?}: bare arrows may only re-request the landing row's insights, \
             never a platform mutation"
        );
        assert_eq!(
            app.shell.selected,
            before + 1,
            "{page:?}: Down must move the cursor exactly one visual row"
        );

        // Repaint with the rebuilt committed plan (the run loop's next paint).
        let (lines, plan) = painted(&app, FRAME);
        let marker = painted_selected_row_or_panic(&lines, &plan, page);
        assert_eq!(
            plan.hit_target(panel.area.x + 1, marker),
            Some(TuiHitTarget::TableRow {
                page,
                index: app.shell.selected,
            }),
            "{page:?}: after Down the painted row must be the new cursor row"
        );
        let needle = row_identity_needle(&app, page, app.shell.selected);
        assert!(
            lines[usize::from(marker)].contains(&needle),
            "{page:?}: the repainted highlight must carry the new row's identity"
        );

        let up = apply_terminal_event_with_plan(&mut app, key(KeyCode::Up), &plan);
        assert!(up.dirty, "{page:?}: the reverse key must demand a repaint");
        assert_eq!(app.shell.selected, before, "{page:?}: Up must return");
        let (lines, plan) = painted(&app, FRAME);
        let marker = painted_selected_row_or_panic(&lines, &plan, page);
        assert_eq!(
            plan.hit_target(panel.area.x + 1, marker),
            Some(TuiHitTarget::TableRow {
                page,
                index: app.shell.selected,
            }),
            "{page:?}: after Up the painted row must be back on the original row"
        );
    }
}

/// (c) The production pointer path: clicking a painted data row through the
/// committed plan selects exactly the row that was painted there, and the
/// repaint afterwards paints the `› ` marker on that same row.
#[test]
fn a_click_on_a_painted_row_selects_it_and_the_repaint_keeps_the_alignment() {
    for page in TABLE_PAGES {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let (lines, plan) = painted(&app, FRAME);
        let panel = plan.table_panel().expect("{page:?} table panel");
        let marker = painted_selected_row_or_panic(&lines, &plan, page);
        let selected_before = app.shell.selected;

        // Click the data row directly below the cursor (every demo page has
        // one below): its absolute y is the marker row plus one row height.
        let clicked =
            apply_terminal_event_with_plan(&mut app, click(panel.area.x + 5, marker + 1), &plan);
        assert!(clicked.dirty, "{page:?}: a landed row click repaints");
        assert_eq!(
            clicked.effect, None,
            "{page:?}: row selection emits no effect"
        );
        assert_eq!(
            app.shell.selected,
            selected_before + 1,
            "{page:?}: the click must select the row painted under the pointer"
        );

        // The repaint after the click must paint the highlight on the clicked
        // row, and the new committed plan must hit exactly that row there.
        let (lines, plan) = painted(&app, FRAME);
        let marker = painted_selected_row_or_panic(&lines, &plan, page);
        assert_eq!(
            plan.hit_target(panel.area.x + 1, marker),
            Some(TuiHitTarget::TableRow {
                page,
                index: selected_before + 1,
            }),
            "{page:?}: the repainted highlight must sit on the clicked row"
        );
        let needle = row_identity_needle(&app, page, selected_before + 1);
        assert!(
            lines[usize::from(marker)].contains(&needle),
            "{page:?}: the repainted highlight must carry the clicked row's identity"
        );
    }
}

/// (d) The semantic channel agrees with the paint: after a pointer selection,
/// the published snapshot marks exactly one row `selected`, its stable id is
/// the clicked visual row's contract id, and a keyboard move re-points that
/// flag at the new row's id.
#[test]
fn the_semantic_selected_flag_names_the_painted_selected_rows_stable_id() {
    let page = AppPage::Applications;
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(page));
    let (lines, plan) = painted(&app, FRAME);
    let panel = plan.table_panel().expect("Applications table panel");
    let marker = painted_selected_row_or_panic(&lines, &plan, page);

    // Click one painted data row below the cursor.
    let clicked =
        apply_terminal_event_with_plan(&mut app, click(panel.area.x + 5, marker + 1), &plan);
    assert!(clicked.dirty);
    let selected = app.shell.selected;

    let (lines, plan) = painted(&app, FRAME);
    let marker = painted_selected_row_or_panic(&lines, &plan, page);
    assert_eq!(
        plan.hit_target(panel.area.x + 1, marker),
        Some(TuiHitTarget::TableRow {
            page,
            index: selected,
        }),
        "the repainted highlight must sit on the clicked row"
    );
    assert_eq!(
        semantic_selected_ids(&app),
        vec![expected_semantic_id(&app, selected)],
        "the semantic snapshot must mark exactly the clicked visual row selected"
    );

    // The keyboard path re-points the flag at the next row's stable id.
    let moved = apply_terminal_event_with_plan(&mut app, key(KeyCode::Down), &plan);
    assert!(moved.dirty);
    let (lines, plan) = painted(&app, FRAME);
    let marker = painted_selected_row_or_panic(&lines, &plan, page);
    assert_eq!(
        plan.hit_target(panel.area.x + 1, marker),
        Some(TuiHitTarget::TableRow {
            page,
            index: app.shell.selected,
        }),
        "the repainted highlight must follow the keyboard cursor"
    );
    let moved_id = expected_semantic_id(&app, app.shell.selected);
    assert_eq!(
        semantic_selected_ids(&app),
        vec![moved_id.clone()],
        "the semantic snapshot must follow the keyboard cursor"
    );
    assert_ne!(
        moved_id,
        expected_semantic_id(&app, selected),
        "the stable id must change when the cursor lands on a different row"
    );
}

/// Acceptance clause "the layout functions are pure and do no terminal I/O":
/// the committed plan is a value projection. Rebuilding it for unchanged app
/// state yields an equal plan and a byte-identical repaint (no clock, env, or
/// hidden draw-state may leak into geometry), and painting cannot move the
/// selection or rewrite the published semantic state — planning and painting
/// only ever consume a shared `&TuiApp` borrow, so any mutation would refuse
/// to compile.
#[test]
fn rebuilding_the_committed_plan_for_unchanged_state_is_deterministic() {
    let page = AppPage::Applications;
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(page));
    let selected = app.shell.selected;
    let semantic_before = app.semantic_snapshot().expect("semantic snapshot");

    let (first_lines, first_plan) = painted(&app, FRAME);
    let (second_lines, second_plan) = painted(&app, FRAME);

    assert_eq!(
        first_plan, second_plan,
        "rebuilding the committed plan for unchanged state must be deterministic"
    );
    assert_eq!(
        first_lines, second_lines,
        "repainting unchanged state must produce an identical frame"
    );
    assert_eq!(
        app.shell.selected, selected,
        "painting must not move the cursor"
    );
    assert_eq!(app.page(), page, "painting must not change the page");
    assert_eq!(
        app.semantic_snapshot().expect("semantic snapshot"),
        semantic_before,
        "painting must not rewrite the published semantic state"
    );
}

/// (e) Resize: the run loop repaints at the new size and REPLACES the
/// committed plan. The same logical row stays painted-selected under the new
/// geometry, the stale plan fail-closes at the cell the new frame painted,
/// and a click through the new committed plan selects the row the new frame
/// paints — never the row the old geometry used to show there.
#[test]
fn a_resize_replaces_the_committed_geometry_and_the_stale_plan_fails_closed() {
    let page = AppPage::Applications;
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(page));

    // Scroll the cursor to the last visual row through the production End
    // path so the bounded window scrolls with it.
    let (_lines, plan) = painted(&app, FRAME);
    let scrolled = apply_terminal_event_with_plan(&mut app, key(KeyCode::End), &plan);
    assert!(scrolled.dirty);
    let selected = app.shell.selected;

    // Repaint the post-key frame; this is the plan committed at the old size.
    let (lines, plan) = painted(&app, FRAME);
    let panel = plan.table_panel().expect("Applications table panel");
    let old_marker = painted_selected_row_or_panic(&lines, &plan, page);
    assert_eq!(
        plan.hit_target(panel.area.x + 1, old_marker),
        Some(TuiHitTarget::TableRow {
            page,
            index: selected,
        }),
        "the scrolled frame must paint the cursor row highlighted"
    );

    // The resize event itself only demands a repaint; the next paint at the
    // new size rebuilds the geometry and the loop commits THAT plan.
    let resize = apply_terminal_event_with_plan(&mut app, Event::Resize(120, 46), &plan);
    assert!(resize.dirty, "a resize must demand a repaint");
    assert_eq!(resize.effect, None);
    let (lines, resized_plan) = painted(&app, RESIZED);
    let resized_panel = resized_plan
        .table_panel()
        .expect("Applications table panel after resize");
    assert_ne!(
        resized_panel.area, panel.area,
        "the resize must replace the painted table geometry"
    );

    // The same logical row stays selected, now painted under the new window.
    let new_marker = painted_selected_row_or_panic(&lines, &resized_plan, page);
    assert_eq!(
        resized_plan.hit_target(resized_panel.area.x + 1, new_marker),
        Some(TuiHitTarget::TableRow {
            page,
            index: selected,
        }),
        "after the resize the painted highlight must still hit the selected row"
    );

    // The STALE plan cannot address that cell: the new window re-centered, so
    // the painted row sits beyond the old window's rows and the old geometry
    // fail-closes instead of naming a row it never painted.
    assert_eq!(
        plan.hit_target(resized_panel.area.x + 1, new_marker),
        None,
        "the stale plan must fail closed at the repainted selected row"
    );

    // End to end: the cell where the OLD frame painted the selection now
    // shows a different row; clicking it through the committed (new) plan
    // must select the newly painted row, never the stale row.
    let Some(TuiHitTarget::TableRow {
        page: hit_page,
        index: repainted,
    }) = resized_plan.hit_target(panel.area.x + 1, old_marker)
    else {
        panic!("the old selected cell must paint a data row after the resize");
    };
    assert_eq!(hit_page, page);
    assert_ne!(
        repainted, selected,
        "the re-centered window must show a different row at the old cell"
    );
    let clicked = apply_terminal_event_with_plan(
        &mut app,
        click(panel.area.x + 1, old_marker),
        &resized_plan,
    );
    assert!(clicked.dirty);
    assert_eq!(
        app.shell.selected, repainted,
        "the post-resize click must select the row the new frame paints there"
    );
    let (lines, plan) = painted(&app, RESIZED);
    let marker = painted_selected_row_or_panic(&lines, &plan, page);
    assert_eq!(
        plan.hit_target(resized_panel.area.x + 1, marker),
        Some(TuiHitTarget::TableRow {
            page,
            index: repainted,
        }),
        "the repaint after the post-resize click must keep the alignment"
    );
}
