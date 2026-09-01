//! Test-only Process Details entries: the canonical-borrowed entry the
//! isolated headless tests resolve through. Production renders consume the
//! lazy-indexed twin, which paints the identical bytes without materializing
//! the O(N) pointer vector.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_details_for_selection;
use crate::{TuiApp, TuiTheme};

pub(crate) fn render_process_details(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    focused: bool,
) {
    app.with_canonical_rows(|ids, visible| {
        render_process_details_with_focus_from_canonical(
            frame, app, theme, area, focused, ids, visible,
        );
    });
}

pub(crate) fn render_process_details_with_focus_from_canonical(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    focused: bool,
    ids: &[taskmanager_shell::ProcessTreeRow],
    visible: &[&taskmanager_core::process::ProcessItem],
) {
    let selected =
        crate::process_view::process_view_support::id_process(ids, visible, app.selected);
    render_details_for_selection(frame, app, theme, area, focused, ids, selected);
}
