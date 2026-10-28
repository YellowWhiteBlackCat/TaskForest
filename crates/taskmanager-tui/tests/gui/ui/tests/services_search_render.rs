//! Services-page search rendering: the page's own field, the filtered rows,
//! the match highlight, and the honest no-match state.

use super::*;
use ratatui::style::Modifier;
use taskmanager_application::AppPage;
use taskmanager_shell::SearchSlot;

/// The first cell of `needle` carries the search-highlight bold style.
fn span_is_bold(terminal: &Terminal<TestBackend>, needle: &str) -> bool {
    let buffer = terminal.backend().buffer();
    let symbols: Vec<char> = buffer
        .content
        .iter()
        .map(|cell| cell.symbol().chars().next().unwrap_or(' '))
        .collect();
    let needle: Vec<char> = needle.chars().collect();
    let Some(position) = symbols
        .windows(needle.len())
        .position(|window| window == needle)
    else {
        return false;
    };
    buffer.content[position]
        .style()
        .add_modifier
        .contains(Modifier::BOLD)
}

#[test]
fn services_search_paints_the_field_filtered_rows_and_match_highlight() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Services;
    app.shell.set_services_query("Network");
    app.shell.open_search_for(SearchSlot::Services);

    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(120, 36);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, &app, TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();

    assert!(
        text.contains("Search services"),
        "the Services field title is painted, got:\n{text}"
    );
    assert!(text.contains('▌'), "the open field paints its caret");
    assert!(text.contains("NetworkManager.service"));
    assert!(text.contains("systemd-timesyncd.service"));
    assert!(
        !text.contains("docker.service") && !text.contains("bluetooth.service"),
        "non-matching rows are filtered out, got:\n{text}"
    );
    assert!(
        text.contains("2 matches"),
        "the field counts the query's visible rows, got:\n{text}"
    );
    assert!(
        span_is_bold(&terminal, "Network manager"),
        "the matched description run carries the shared search highlight"
    );
}

#[test]
fn services_search_without_matches_reports_the_query_not_an_empty_provider() {
    let mut app = crate::demo_app();
    app.application.active_page = AppPage::Services;
    app.shell.set_services_query("zzz-no-such-service");

    let text = frame_text(&app, 120, 36);

    assert!(
        text.contains("No services match the query."),
        "a filtered-empty list names the query, got:\n{text}"
    );
    assert!(
        !text.contains("No services reported yet."),
        "a filtered-empty list must not claim the provider reported nothing"
    );
    assert!(
        text.contains("Search services"),
        "the field stays painted so the query can be cleared in place"
    );
    assert!(
        text.contains("zzz-no-such-service"),
        "the active query remains visible in the field"
    );
}
