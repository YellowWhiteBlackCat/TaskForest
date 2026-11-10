//! Capture readiness for the calibrated zero-gray Applications row.

use super::capture_marker::CaptureAccess;
use crate::pages::processes::ProcessScrollIntent;

/// The apps-zero-gray scenario waits for the scrolled zero row to be painted
/// in the calibrated muted ink before it admits the frame's data.
pub(super) fn apps_zero_gray_ready(access: &mut CaptureAccess) -> bool {
    let Some(scroll) = access
        .process_scroll
        .as_ref()
        .filter(|scroll| scroll.viewport_rows > 0)
    else {
        return false;
    };
    let desired = access
        .track
        .shell
        .visible_processes()
        .len()
        .saturating_sub(scroll.viewport_rows);
    if scroll.top != desired {
        if let Ok(root) = access.process_roots.single() {
            access.commands.trigger(ProcessScrollIntent {
                entity: root,
                rows: isize::MAX,
            });
        }
        return false;
    }
    let muted = crate::widgets::table::contrast::calibrate_muted_ink(
        access.palette.inner.dim_color,
        access.palette.inner.body_color,
        &[
            access.palette.inner.content_bg,
            access.palette.inner.nav_active_bg,
        ],
    );
    if !access.zero_cells.iter().any(|(zero, text, node, color)| {
        zero.0 && text.0 == "0.0%" && node.size().x > 0.0 && node.size().y > 0.0 && color.0 == muted
    }) {
        return false;
    }
    if !access.state.data_presented {
        access.state.data_presented = true;
        return false;
    }
    true
}
