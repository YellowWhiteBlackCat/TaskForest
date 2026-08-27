//! Table-page cursor navigation: Home/End bound jumps and PageUp/PageDown
//! paging (ADR-027).
//!
//! Every TABLE page (Applications + Services + Users + Startup) has a cursor
//! that ranges over a row list; Performance / System / AppHistory are fact or
//! chart surfaces without one. Home/End jump to the first/last visible row
//! and PageUp/PageDown move by the shared page step. On the Applications page
//! the category tree interleaves headers, so the cursor ranges over the
//! VISUAL row projection and the navigation resolves against the TUI's own
//! row lists; every other table page is flat rows and reuses the shell's
//! shared jump (whose `table_row_count` clamps to the page's own list).
//! Extracted from `runtime.rs` so no runtime file exceeds the source line
//! budget; behavior unchanged.

use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
use taskmanager_application::{AppPage, PlatformEffect};
use taskmanager_shell::InputDispatch;

use crate::TuiApp;

/// Route one table-page navigation key with an explicit consumed/unhandled
/// result. Chorded variants (Ctrl+Home, Shift+PageUp, …) are not wired.
#[must_use]
pub(super) fn handle_table_navigation(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if !is_table_page(app.page())
        || key.modifiers.intersects(
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::SHIFT,
        )
    {
        return InputDispatch::Unhandled;
    }
    match key.code {
        ratatui::crossterm::event::KeyCode::Home => {
            InputDispatch::consumed(jump_selection_to_bound(app, false))
        }
        ratatui::crossterm::event::KeyCode::End => {
            InputDispatch::consumed(jump_selection_to_bound(app, true))
        }
        ratatui::crossterm::event::KeyCode::PageUp if page_rows_flat(app) => {
            app.detail_scroll_reset();
            app.move_selection(-(taskmanager_shell::PAGE_STEP as isize));
            InputDispatch::consumed(app.refresh_selected_process_insights())
        }
        ratatui::crossterm::event::KeyCode::PageDown if page_rows_flat(app) => {
            app.detail_scroll_reset();
            app.move_selection(taskmanager_shell::PAGE_STEP as isize);
            InputDispatch::consumed(app.refresh_selected_process_insights())
        }
        ratatui::crossterm::event::KeyCode::PageUp => InputDispatch::consumed(
            app.move_nonflat_selection_oneshot(-(taskmanager_shell::PAGE_STEP as isize)),
        ),
        ratatui::crossterm::event::KeyCode::PageDown => InputDispatch::consumed(
            app.move_nonflat_selection_oneshot(taskmanager_shell::PAGE_STEP as isize),
        ),
        _ => InputDispatch::Unhandled,
    }
}

/// Whether the active page is a TABLE page whose cursor ranges over a list of
/// rows (Applications + Services + Users + Startup). Performance / System /
/// AppHistory are fact or chart surfaces without a cursor.
fn is_table_page(page: AppPage) -> bool {
    matches!(
        page,
        AppPage::Applications | AppPage::Services | AppPage::Users | AppPage::Startup
    )
}

/// Whether the cursor ranges over the shell's flat rows on the current page.
/// Applications always uses its category-tree projection.
fn page_rows_flat(app: &TuiApp) -> bool {
    app.page() != AppPage::Applications
}

/// Jump the cursor to the first (`last == false`) or last (`last == true`)
/// visible row on a TABLE page. The Applications page indexes its interleaved
/// category-tree visual rows, so the jump resolves against the TUI projection
/// (the shell's `active_row_count` counts process facts and would clamp to the
/// wrong bound there). The other table pages are flat
/// rows, so they always take the shared-jump path (whose `table_row_count`
/// clamps to the page's own list). Every path resets the detail-panel scroll
/// and re-requests insights for the landing row, mirroring the Up/Down
/// branches.
#[must_use]
fn jump_selection_to_bound(app: &mut TuiApp, last: bool) -> Option<PlatformEffect> {
    if app.page() != AppPage::Applications {
        app.detail_scroll_reset();
        if last {
            app.move_selection_to_last();
        } else {
            app.move_selection_to_first();
        }
        app.refresh_selected_process_insights()
    } else {
        let rows = app.process_rows_snapshot();
        let index = if last {
            rows.len().saturating_sub(1)
        } else {
            0
        };
        let process = crate::process_view::process_at(&rows, index).cloned();
        let row_key = crate::process_view::row_key_at(&rows, index);
        app.apply_selection_resolution_with_row(index, process, row_key)
    }
}
