use super::containers_support::render_containers_overlay;
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;

use crate::demo_app;

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The title/headers resolve through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_containers_overlay(frame, app, crate::TuiTheme::default(), frame.area())
        })
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn containers_overlay_renders_rollup_rows_and_typed_state() {
    let app = demo_app();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Containers"));
    assert!(text.contains("postgres"));
    assert!(text.contains("docker"));
    assert!(text.contains("12.5%"));
    assert!(text.contains("68.5 MiB"));
    assert!(text.contains("healthy · 2 container(s)"));
    assert!(text.contains("c / Esc"));
}

#[test]
fn containers_overlay_renders_typed_unavailable_state_honestly() {
    let mut app = demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(Some(
            ContainerRollup::unavailable(
                taskmanager_core::core::device_state::DeviceState::default(),
            ),
        )),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("unsupported"));
    assert!(text.contains("No containers are listed"));
}

#[test]
fn containers_overlay_renders_healthy_empty_state() {
    let mut app = demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(Some(
            ContainerRollup::empty_healthy(1_000),
        )),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("No containers running on this host."));
}

#[test]
fn containers_overlay_renders_unavailable_fields_as_dashes() {
    let mut app = demo_app();
    let mut containers = app.shell.projection().containers.clone();
    if let Some(rollup) = containers.as_mut()
        && let Some(container) = rollup.containers.first_mut()
    {
        container.cpu_percentage = ScalarObservation::unavailable(FailureKind::PermissionDenied);
        container.memory_bytes = ScalarObservation::unavailable(FailureKind::PermissionDenied);
    }
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(containers),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("—"));
    assert!(!text.contains("0.0%"));
}

#[test]
fn containers_overlay_caps_rows_and_reports_hidden_count() {
    let (shown, hidden) = container_row_window(203);
    assert_eq!(shown, taskmanager_application::MAX_CONTAINER_ROWS);
    assert_eq!(hidden, 3);
    let label = more_rows_label(hidden);
    assert!(label.contains('3'));
    assert!(!label.contains("{count}"));
}

// ── Modal / KeyHint component contract ───────────────────────────────────────
//
// The shared modal host and footer key-hint vocabulary extracted from the
// overlay surfaces. These tests pin the terminal presentation: geometry stays
// with the caller (the host only paints into the `Rect` it is handed), and
// the visual vocabulary (accent borders, black-on-accent chords, dim labels)
// stays byte-identical to the surfaces it replaced.

/// The style of the first cell of `needle` in the rendered buffer, for
/// assertions on styling that carries no text marker (mirrors the
/// focus-paint helper).
fn first_cell_style(
    terminal: &Terminal<TestBackend>,
    needle: &str,
) -> Option<ratatui::style::Style> {
    let buffer = terminal.backend().buffer();
    let symbols: Vec<char> = buffer
        .content
        .iter()
        .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
        .collect();
    let needle: Vec<char> = needle.chars().collect();
    let position = symbols
        .windows(needle.len())
        .position(|window| window == needle)?;
    Some(buffer.content[position].style())
}

#[test]
fn modal_host_paints_borders_icon_title_and_returns_inner() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let popup = Rect::new(2, 1, 30, 8);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).expect("test terminal");
    let mut inner = Rect::ZERO;
    terminal
        .draw(|frame| {
            inner = Modal::new(theme, IconId::Service, "Fixture modal").render(frame, popup);
        })
        .expect("draw");

    // The host hands back the bordered interior; the surface lays its own
    // body/footer into it.
    assert_eq!(
        inner,
        Rect::new(popup.x + 1, popup.y + 1, popup.width - 2, popup.height - 2)
    );

    let buffer = terminal.backend().buffer();
    // The full border frame paints the accent tone over the overlay backdrop.
    for y in popup.y..popup.bottom() {
        for x in popup.x..popup.right() {
            let edge =
                y == popup.y || y == popup.bottom() - 1 || x == popup.x || x == popup.right() - 1;
            if edge {
                let style = buffer[(x, y)].style();
                assert_eq!(style.fg, Some(theme.accent), "border cell ({x},{y}) fg");
                assert_eq!(style.bg, Some(theme.overlay_bg), "border cell ({x},{y}) bg");
            }
        }
    }
    assert_eq!(buffer[(popup.x, popup.y)].symbol(), "┌");
    assert_eq!(buffer[(popup.right() - 1, popup.y)].symbol(), "┐");
    assert_eq!(buffer[(popup.x, popup.bottom() - 1)].symbol(), "└");
    assert_eq!(
        buffer[(popup.right() - 1, popup.bottom() - 1)].symbol(),
        "┘"
    );

    // The title row carries the resolved icon glyph immediately before the
    // title text, inside the frame.
    let title_row = text_row(&terminal, popup.y);
    assert!(
        title_row.contains(&format!(
            " {} {} ",
            theme.glyph(IconId::Service),
            "Fixture modal"
        )),
        "title row must carry icon + title, got: {title_row:?}"
    );
    // The host cleared the handed area: interior cells stay blank.
    assert_eq!(buffer[(popup.x + 3, popup.y + 3)].symbol(), " ");
    assert_eq!(
        buffer[(popup.x + 3, popup.y + 3)].style().bg,
        Some(theme.overlay_bg)
    );
}

#[test]
fn modal_alert_paints_plain_title_and_the_typed_border_tone() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let popup = Rect::new(2, 1, 24, 5);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).expect("test terminal");
    terminal
        .draw(|frame| {
            Modal::alert(theme, theme.danger, "Confirm").render(frame, popup);
        })
        .expect("draw");

    // The confirmation family: plain padded title, no icon, and the border
    // tone the caller picked (danger here).
    let title_row = text_row(&terminal, popup.y);
    assert!(
        title_row.contains(" Confirm "),
        "alert title must be the plain padded text, got: {title_row:?}"
    );
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(popup.x, popup.y)].style().fg, Some(theme.danger));
    assert_eq!(
        buffer[(popup.x + 2, popup.y + 2)].style().bg,
        Some(theme.overlay_bg)
    );
}

#[test]
fn keyhint_paints_chords_black_on_accent_and_labels_dim() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("test terminal");
    terminal
        .draw(|frame| {
            frame.render_widget(
                KeyHint::centered(
                    theme,
                    vec![
                        ("Tab", " Next · ".to_owned()),
                        ("Enter", " Select".to_owned()),
                    ],
                ),
                frame.area(),
            );
        })
        .expect("draw");

    // Pairs concatenate chord + dim label in order on one centered line (the
    // separator rides inside the label, exactly as the menu footers compose
    // their "chord label · chord label" runs).
    let row = text_row(&terminal, 0);
    assert_eq!(
        row.trim(),
        "Tab Next · Enter Select",
        "one centered hint line, got: {row:?}"
    );

    // The chord vocabulary: black on accent. The label vocabulary: dim, with
    // no background of its own (the untouched TestBackend cell default).
    let chord = first_cell_style(&terminal, "Tab").expect("chord present");
    assert_eq!(chord.fg, Some(theme.color(Color::Black)));
    assert_eq!(chord.bg, Some(theme.accent));
    let label = first_cell_style(&terminal, "Next").expect("label present");
    assert_eq!(label.fg, Some(theme.dim));
    assert_ne!(label.bg, Some(theme.accent));
}

#[test]
fn keyhint_toned_pairs_paint_danger_and_inverse_chords_with_default_labels() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("test terminal");
    terminal
        .draw(|frame| {
            frame.render_widget(
                KeyHint::line_toned(
                    theme,
                    vec![
                        (KeyHintTone::Danger, " y ", " Confirm".to_owned()),
                        (KeyHintTone::Inverse, " n / Esc ", " Cancel".to_owned()),
                    ],
                ),
                frame.area(),
            );
        })
        .expect("draw");

    // Pairs concatenate chord + label in order on one line (the confirmation
    // family's two-tone footer).
    let row = text_row(&terminal, 0);
    assert!(
        row.contains(" y  Confirm n / Esc  Cancel"),
        "the toned pairs render in order, got: {row:?}"
    );

    // The Danger chord: black on the theme danger tone. The Inverse chord:
    // black on resolved white. Both confirmation labels keep the default
    // foreground (no dim restyle) and no background of their own.
    let confirm = first_cell_style(&terminal, " y ").expect("confirm chord present");
    assert_eq!(confirm.fg, Some(theme.color(Color::Black)));
    assert_eq!(confirm.bg, Some(theme.danger));
    let dismiss = first_cell_style(&terminal, " n / Esc ").expect("dismiss chord present");
    assert_eq!(dismiss.fg, Some(theme.color(Color::Black)));
    assert_eq!(dismiss.bg, Some(theme.color(Color::White)));
    let label = first_cell_style(&terminal, "Confirm").expect("confirm label present");
    assert_eq!(
        label.fg,
        Some(Color::Reset),
        "danger-tone labels keep the raw default foreground, not dim"
    );
    assert_eq!(
        label.bg,
        Some(Color::Reset),
        "no chord background on the label"
    );
    let cancel = first_cell_style(&terminal, "Cancel").expect("cancel label present");
    assert_eq!(
        cancel.fg,
        Some(Color::Reset),
        "inverse-tone labels keep the raw default foreground, not dim"
    );
    assert_eq!(
        cancel.bg,
        Some(Color::Reset),
        "no chord background on the label"
    );
}

/// The rendered text of one buffer row.
fn text_row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    let width = buffer.area.width as usize;
    let start = y as usize * width;
    let row: String = buffer.content[start..start + width]
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    row
}

// ── Windowed bordered table (inventory-page primitive) ──────────────────────
//
// The shared bordered-table-with-window paint behind the Services / Startup /
// Users pages. These fixtures drive the primitive directly and assert the
// buffer: the window clipping, the selection highlight, the sort marker, and
// the honest zero-row branch. Page-level regressions live with the pages'
// render tests (Services rows: `source_render`, Startup columns:
// `startup_render`, Users rows: below).

/// Whole-frame render helper for the page-level regressions (mirrors the
/// `frame_text` convention of the render tests): pin English and serialize
/// against the language-flipping i18n test.
fn page_frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::render(frame, app, crate::TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn windowed_table_paints_the_window_clips_rows_and_highlights_the_selection() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    // A 12-high panel paints 8 body rows (2 borders + header + header margin
    // are chrome), centred on the global cursor by the shared window rule.
    let area = Rect::new(2, 3, 60, 12);
    let (total, selected) = (50, 25);
    let mut terminal = Terminal::new(TestBackend::new(64, 16)).expect("test terminal");
    let mut outcome = None;
    terminal
        .draw(|frame| {
            outcome = Some(render_windowed_table(
                frame,
                WindowedTableProps {
                    theme,
                    panel: TablePanelProjection::new(area, total, selected),
                    title: "Fixture table",
                    header: sort_header_row(
                        ["Name", "State"],
                        theme,
                        Some((1, taskmanager_shell::SortDir::Desc)),
                    ),
                    widths: vec![Constraint::Percentage(50), Constraint::Min(10)],
                    column_spacing: 2,
                    state_area: area,
                    state_message: "fixture state",
                },
                |index| {
                    Row::new([
                        Cell::from(format!("row{index:02}")),
                        Cell::from(if index % 2 == 0 { "even" } else { "odd" }),
                    ])
                },
            ));
        })
        .expect("draw");

    assert_eq!(outcome, Some(WindowedTableOutcome::Table));
    assert_eq!(
        crate::ui::frame_plan::table_window(total, selected, area),
        crate::ui::frame_plan::TableWindow {
            start: 21,
            end: 29,
            selected: 4
        },
        "the primitive consumes the shared window rule's geometry"
    );
    let text = terminal.backend().to_string();
    assert!(
        text.contains("Fixture table"),
        "bordered panel title paints"
    );
    assert!(text.contains("Name"), "the header row paints");
    assert!(
        text.contains("State ▼"),
        "the Desc sort marker rides the header"
    );
    assert!(text.contains("row21"), "the window's head row materializes");
    assert!(text.contains("row28"), "the window's tail row materializes");
    assert!(text.contains("row25"), "the selected row materializes");
    assert!(
        !text.contains("row20"),
        "the row above the window is clipped"
    );
    assert!(
        !text.contains("row29"),
        "the row below the window is clipped"
    );
    assert!(
        !text.contains("row00"),
        "rows before the window never materialize"
    );
    assert!(
        !text.contains("row49"),
        "rows after the window never materialize"
    );

    // The selection highlight rides exactly the selected row.
    let selected_style = first_cell_style(&terminal, "row25").expect("selected row painted");
    assert_eq!(
        selected_style.bg,
        Some(theme.highlight_bg),
        "the selected row carries the shared highlight"
    );
    let neighbour_style = first_cell_style(&terminal, "row26").expect("neighbour row painted");
    assert_ne!(
        neighbour_style.bg,
        Some(theme.highlight_bg),
        "unselected rows keep the default background"
    );
}

#[test]
fn windowed_table_zero_rows_paints_the_state_panel_not_a_bare_header() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = crate::TuiTheme::default();
    let area = Rect::new(1, 1, 38, 4);
    // A page may widen the state area beyond the table's own slot (the
    // Services page folds the log band's slot into its empty state), so the
    // fixture hands the primitive a distinct `state_area`.
    let state_area = Rect::new(1, 6, 38, 5);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).expect("test terminal");
    let mut outcome = None;
    terminal
        .draw(|frame| {
            outcome = Some(render_windowed_table(
                frame,
                WindowedTableProps {
                    theme,
                    panel: TablePanelProjection::new(area, 0, 0),
                    title: "Fixture table",
                    header: sort_header_row(["Name", "State"], theme, None),
                    widths: vec![Constraint::Percentage(50), Constraint::Min(10)],
                    column_spacing: 2,
                    state_area,
                    state_message: "fixture failure: unread source",
                },
                |index| Row::new([Cell::from(format!("row{index:02}"))]),
            ));
        })
        .expect("draw");

    assert_eq!(outcome, Some(WindowedTableOutcome::StatePanel));
    let text = terminal.backend().to_string();
    assert!(
        text.contains("Fixture table"),
        "the state panel keeps the bordered title"
    );
    assert!(
        text.contains("fixture failure: unread source"),
        "the caller's empty/failure text paints verbatim"
    );
    assert!(!text.contains("Name"), "no header paints without rows");
    assert!(!text.contains("row00"), "no rows are fabricated");
    // The table's own slot stays untouched; the state panel owned `state_area`.
    let top_border_row = text_row(&terminal, area.y);
    assert!(
        !top_border_row.contains('┌'),
        "the unpainted table slot must not draw a border"
    );
}

/// Migration regression: the Users page still renders its session rows,
/// columns and title through the shared primitive. (Services rows are pinned
/// by `source_render`, Startup columns by `startup_render`.)
#[test]
fn users_page_renders_session_rows_through_the_shared_table() {
    let mut app = demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let text = page_frame_text(&app, 120, 36);
    assert!(
        text.contains("User sessions"),
        "the Users table title paints"
    );
    assert!(text.contains("devuser"), "session rows paint");
    assert!(text.contains("seat0"), "the seated session row paints");
    assert!(text.contains("pts/4"), "the remote session row paints");
}
