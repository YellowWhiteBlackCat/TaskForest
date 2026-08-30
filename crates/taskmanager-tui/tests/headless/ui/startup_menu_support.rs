//! Test-only startup menu overlay adapter.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::render_startup_menu_at;
use crate::TuiTheme;
use crate::ui::TuiFocusPlan;
use crate::ui::frame_plan::overlay_popup;

pub(crate) fn render_startup_menu(
    frame: &mut Frame<'_>,
    menu: &StartupMenuTarget,
    theme: TuiTheme,
    focus: TuiFocusPlan,
    area: Rect,
) {
    render_startup_menu_at(
        frame,
        menu,
        theme,
        focus,
        overlay_popup(
            area,
            crate::TuiInputScope::LocalSurface(crate::TuiSurfaceKind::StartupMenu),
        )
        .unwrap_or(Rect::ZERO),
    );
}

pub(crate) use super::StartupMenuTarget;
