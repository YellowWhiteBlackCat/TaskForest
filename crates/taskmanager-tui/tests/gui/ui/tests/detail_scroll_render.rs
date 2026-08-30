//! Vertical-scroll tests for the short-terminal detail/insights panel and the
//! Process Properties modal tab body. These prove the scroll offset (a) reaches
//! content that the fixed viewport clips, (b) clamps at the last line, and
//! (c) resets when the table selection moves / the modal reopens. The pure
//! clamp helper is pinned separately from the render path.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use taskmanager_application::{AppAction, AppPage};

use crate::ui::process_details::process_details_support::render_process_details;
use crate::ui::process_details::{clamped_scroll, wrapped_content_height};
use crate::ui::process_properties::process_properties_support::render_process_properties;
use crate::ui::process_properties::{ProcessDetailsSection, ProcessPropertiesTarget};
use crate::{TuiApp, TuiTheme};

/// Pin English + serialize against the language-flipping i18n test, then render
/// the inline detail panel alone into a `width × height` TestBackend. Rendering
/// the panel in isolation (not the full frame) gives a deterministic viewport
/// height independent of the search box / table split.
fn detail_panel_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            let area = Rect::new(0, 0, width, height);
            render_process_details(
                frame,
                app,
                TuiTheme::default(),
                area,
                app.focus_panel == crate::FocusPanel::Details,
            );
        })
        .expect("draw");
    terminal.backend().to_string()
}

/// Same as [`detail_panel_text`] but for the Process Properties modal, rendered
/// alone so the body viewport height is deterministic.
fn modal_text(target: &ProcessPropertiesTarget, app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            let area = Rect::new(0, 0, width, height);
            render_process_properties(
                frame,
                target,
                app,
                TuiTheme::default(),
                modal_focus_plan(target.section),
                area,
            );
        })
        .expect("draw");
    terminal.backend().to_string()
}

/// The focus plan that mirrors this modal fixture: the shared Properties
/// surface addressing `section`. The renderer must highlight from this plan.
fn modal_focus_plan(section: ProcessDetailsSection) -> crate::ui::frame_plan::TuiFocusPlan {
    use crate::ui::frame_plan::{TuiFocusControl, TuiFocusOrder, TuiFocusPlan, TuiFocusTarget};
    TuiFocusPlan {
        target: TuiFocusTarget::SharedSurface(
            taskmanager_application::SurfaceKind::ProcessProperties,
        ),
        order: TuiFocusOrder::None,
        control: TuiFocusControl::PropertiesTab(section),
    }
}

/// A demo app parked on the Applications page with the cursor on its first
/// (highest-CPU) process so the detail panel has real content.
fn app_on_applications() -> TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app
}

const COLLECTING: &str = "Loading process insights";

// ── Pure clamp helper ───────────────────────────────────────────────────────

/// `clamped_scroll` returns `(effective, max)` where `max` is the largest
/// offset that keeps the last content line on-screen, and `effective` is the
/// stored intent clamped into `[0, max]`. A content set shorter than the
/// viewport forces both to 0.
#[test]
fn clamped_scroll_caps_offset_and_zeros_when_content_fits() {
    // content 20, viewport 8 → max 12; a mid offset passes through unchanged.
    let (eff, max) = clamped_scroll(20, 8, 5);
    assert_eq!((eff, max), (5, 12));
    // An intent past the end clamps to the max — the viewport never scrolls
    // beyond the last content line (no blank rows trailing the content).
    let (eff, max) = clamped_scroll(20, 8, 99);
    assert_eq!((eff, max), (12, 12));
    // Content that fits the viewport is never scrollable.
    let (eff, max) = clamped_scroll(5, 8, 3);
    assert_eq!((eff, max), (0, 0));
    // A zero-height viewport degenerates to max 0 (no scrolling into nothing).
    let (eff, max) = clamped_scroll(20, 0, 4);
    assert_eq!((eff, max), (0, 0));
}

/// `wrapped_content_height` must reflect the word-wrapper's actual row count:
/// equal to the raw line count at a wide width, and strictly greater once the
/// width forces a wrap.
#[test]
fn wrapped_content_height_tracks_word_wrap() {
    let lines = vec![ratatui::text::Line::from("aaaa bbbb cccc dddd")]; // 19 chars
    // Wide enough: one source line → one rendered row.
    assert_eq!(wrapped_content_height(&lines, 80), 1);
    // Narrow: the wrapper breaks the single source line across multiple rows.
    let narrow = wrapped_content_height(&lines, 5);
    assert!(
        narrow >= 3,
        "a 19-char line at width 5 must wrap to at least 3 rows, got {narrow}"
    );
}

// ── Inline detail panel: scroll reaches clipped content ─────────────────────

/// On a short terminal the inline detail panel clips the late insight rows.
/// At scroll 0 the first row ("zed") is visible and the trailing collecting
/// line is clipped; scrolling to the max offset reveals that clipped line and
/// drops the first row off the top. This proves the offset actually moves the
/// viewport in both directions, not just that the content is present.
#[test]
fn detail_panel_scroll_reaches_clipped_insights_line() {
    let mut app = app_on_applications();
    // The detail content (15 frozen rows + 1 collecting insights line = 16)
    // overflows the 8-row inner viewport of a 10-row panel.
    app.detail_scroll = 0;
    let top = detail_panel_text(&app, 80, 10);
    assert!(
        top.contains("zed"),
        "the first detail row must be visible at scroll 0\ntext:\n{top}"
    );
    assert!(
        !top.contains(COLLECTING),
        "the clipped insights line must NOT be visible at scroll 0\ntext:\n{top}"
    );

    // Scroll to the max offset (16 content − 8 visible = 8). The collecting
    // line is now in view and the first row is scrolled off the top.
    app.detail_scroll = 8;
    let bottom = detail_panel_text(&app, 80, 10);
    assert!(
        bottom.contains(COLLECTING),
        "the clipped insights line must be reachable by scrolling\ntext:\n{bottom}"
    );
    assert!(
        !bottom.contains("zed"),
        "the first detail row must scroll off the top at max offset\ntext:\n{bottom}"
    );
    // The title surfaces the scroll chord + position only while overflowing.
    assert!(top.contains("Ctrl+"));
}

/// A scroll intent far past the end clamps to the last content line: the
/// trailing insights line is visible and rendering does not panic or leave
/// blank rows beyond the content.
#[test]
fn detail_panel_scroll_clamps_past_the_end() {
    let mut app = app_on_applications();
    // Absurd intent — the renderer must clamp, not overflow.
    app.detail_scroll = 9_999;
    let text = detail_panel_text(&app, 80, 10);
    assert!(
        text.contains(COLLECTING),
        "clamping must keep the last content line on-screen\ntext:\n{text}"
    );
}

/// When the terminal is tall enough to fit the whole detail panel, the scroll
/// chord must NOT appear in the title (no false overflow hint) and the content
/// renders in full without scrolling.
#[test]
fn detail_panel_no_scroll_hint_when_content_fits() {
    let mut app = app_on_applications();
    app.detail_scroll = 3; // an intent that should be clamped away to 0
    let text = detail_panel_text(&app, 80, 30);
    assert!(
        !text.contains("Ctrl+"),
        "no scroll hint when the content fits the viewport\ntext:\n{text}"
    );
    assert!(text.contains("zed") && text.contains(COLLECTING));
}

// ── Scroll offset resets on selection move ──────────────────────────────────

/// A category-tree selection move goes through
/// `apply_selection_resolution`, which must reset the inline detail scroll so a
/// stale offset from the previous row does not survive into fresh content.
#[test]
fn detail_scroll_resets_when_category_tree_selection_moves() {
    let mut app = app_on_applications();
    app.selected = 0;
    // Move onto the first member (selected changes) after arming a scroll.
    app.move_nonflat_selection_oneshot(1);
    app.detail_scroll = 7;
    assert_eq!(app.detail_scroll, 7);
    // Move to the next row — a real selection change must reset the offset.
    app.move_nonflat_selection_oneshot(1);
    assert_eq!(
        app.detail_scroll, 0,
        "a selection move must reset the detail scroll offset"
    );
}

/// `detail_scroll_reset` is the flat-path hook the runtime calls alongside
/// `move_selection`; it unconditionally zeroes the intent.
#[test]
fn detail_scroll_reset_zeroes_the_intent() {
    let mut app = app_on_applications();
    app.detail_scroll = 12;
    app.detail_scroll_reset();
    assert_eq!(app.detail_scroll, 0);
}

// ── Process Properties modal: tab body scrolls ──────────────────────────────

/// A frozen target for the modal, built from the demo's first process. The
/// scroll field is the user's intent for the tab body.
fn properties_target(pid: u32) -> ProcessPropertiesTarget {
    let item = crate::demo_app()
        .projection()
        .processes
        .as_deref()
        .and_then(|procs| procs.iter().find(|p| p.pid == pid))
        .expect("demo process exists")
        .clone();
    ProcessPropertiesTarget {
        item,
        section: ProcessDetailsSection::default(),
        scroll: 0,
    }
}

/// On a short terminal the modal's Overview tab body clips the late rows. At
/// scroll 0 the early "Parent PID" row shows and "Start time" is clipped;
/// scrolling reveals "Start time" and drops "Parent PID" off the top. (The
/// process name and pid also appear in the modal TITLE, which does not scroll,
/// so they cannot serve as top markers — "Parent PID" is unique to row 3.)
#[test]
fn modal_tab_body_scroll_reaches_clipped_overview_rows() {
    let app = app_on_applications();
    let mut target = properties_target(4201);
    target.section = ProcessDetailsSection::Overview;
    // The Overview tab carries 7 kv rows; a 10-row modal clamps the body to a
    // ~4-row viewport, so the bottom rows are clipped at scroll 0.
    target.scroll = 0;
    let top = modal_text(&target, &app, 96, 10);
    assert!(
        top.contains("Parent PID"),
        "an early overview row must be visible at scroll 0\ntext:\n{top}"
    );
    assert!(
        !top.contains("Start time"),
        "the clipped last overview row must NOT be visible at scroll 0\ntext:\n{top}"
    );

    // Scroll to the max offset — the last row is now in view and the early
    // "Parent PID" row has scrolled off the top.
    target.scroll = 3;
    let bottom = modal_text(&target, &app, 96, 10);
    assert!(
        bottom.contains("Start time"),
        "the clipped overview row must be reachable by scrolling\ntext:\n{bottom}"
    );
    assert!(
        !bottom.contains("Parent PID"),
        "the early overview row must scroll off the top at max offset\ntext:\n{bottom}"
    );
}

/// A modal scroll intent past the end clamps to the last tab row without
/// panicking.
#[test]
fn modal_tab_body_scroll_clamps_past_the_end() {
    let app = app_on_applications();
    let mut target = properties_target(4201);
    target.section = ProcessDetailsSection::Overview;
    target.scroll = 9_999;
    let text = modal_text(&target, &app, 96, 10);
    assert!(
        text.contains("Start time"),
        "clamping must keep the last tab row on-screen\ntext:\n{text}"
    );
}

/// Switching tabs resets the modal scroll offset to 0 — each tab is independent
/// content, so a stale offset from the previous tab must not survive. Drives
/// the real `TuiApp` production methods (`open_process_properties` /
/// `process_properties_scroll_by` / `process_properties_next_tab`).
#[test]
fn modal_tab_switch_resets_scroll_offset() {
    let mut app = app_on_applications();
    assert!(
        app.open_process_properties(),
        "the modal opens on a selected process row"
    );
    // Arm a non-zero scroll on the Overview tab.
    app.process_properties_scroll_by(4);
    assert_eq!(app.process_properties().expect("modal open").scroll, 4,);
    // Switching tabs resets the offset to 0 (each tab is independent content).
    app.process_properties_next_tab();
    let target = app.process_properties().expect("modal open");
    assert_eq!(
        target.scroll, 0,
        "tab switch must reset the modal scroll offset"
    );
    assert_eq!(target.section, ProcessDetailsSection::Performance);
}
