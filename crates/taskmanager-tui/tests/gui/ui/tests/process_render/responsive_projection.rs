use super::*;
use taskmanager_test_support::pin_english;

fn frame_text_in_language(app: &TuiApp, width: u16, height: u16, language: Language) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(language);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, app, TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();
    set_language(Language::En);
    text
}

#[test]
fn applications_table_fits_whole_columns_and_localized_title_at_54_and_120_cells() {
    pin_english();
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    let saved_order = app.column_order.clone();
    let saved_hidden = app.hidden_columns.clone();

    let compact = frame_text(&app, 54, 16);
    let compact_header = compact
        .lines()
        .find(|line| line.contains("PID") && line.contains("CPU"))
        .expect("compact process header");
    assert!(
        compact_header.contains("PID"),
        "PID remains visible:\n{compact}"
    );
    assert!(
        compact_header.contains("Name"),
        "Name remains visible:\n{compact}"
    );
    assert!(
        compact_header.contains("CPU"),
        "CPU remains visible:\n{compact}"
    );
    assert!(
        compact_header.contains("Memory"),
        "the first optional column fits whole in the 54-column projection:\n{compact}"
    );
    assert!(
        !compact_header.contains("Pss") && !compact_header.contains("Netwo"),
        "the 54-column projection omits the remaining optional tail as whole columns:\n{compact}"
    );
    assert!(
        compact.contains("Processes · Enter details"),
        "the compact title keeps complete localized action groups:\n{compact}"
    );

    let standard = frame_text(&app, 120, 36);
    let standard_header = standard
        .lines()
        .find(|line| line.contains("PID") && line.contains("CPU"))
        .expect("standard process header");
    assert!(
        standard_header.contains("Memory"),
        "the 120-column projection admits its first tail column:\n{standard}"
    );
    assert!(
        !standard_header.contains("Netwo"),
        "the last optional column is omitted as a whole when it does not fit:\n{standard}"
    );
    assert!(
        standard.contains(
            "Processes · ↑/↓ PgUp/PgDn · s sort · ? help · Enter details · Delete end task"
        ),
        "the standard title keeps its complete localized hints:\n{standard}"
    );
    assert_eq!(
        app.column_order, saved_order,
        "responsive fitting never rewrites user order"
    );
    assert_eq!(
        app.hidden_columns, saved_hidden,
        "responsive fitting never rewrites user visibility"
    );

    let zh = frame_text_in_language(&app, 54, 16, Language::Zh);
    assert!(
        zh.contains("进程 · Enter 详情"),
        "the compact title uses complete localized Chinese hint groups:\n{zh}"
    );
}
