//! Content-visibility regression net (the 2026-08-10 lesson: whole-frame
//! text assertions stayed green while border math made an entire widget
//! invisible). Two contracts are pinned here:
//!
//! 1. **Page-level**: the body region — the rows between the 4-row header
//!    and the 3-row footer — actually paints content on every page at the
//!    reference and minimum terminal sizes. A page whose body paints zero
//!    rows is invisible regardless of what the header/footer say.
//! 2. **Widget-level**: all-or-nothing projections (the per-core grid)
//!    render every advertised row. If the first core cell is visible, the
//!    last one must be too — a half-eaten grid was exactly the historical
//!    blindspot, and no whole-frame `contains` can catch it.

use taskmanager_application::{AppAction, AppPage};

use super::frame_text;
use crate::TuiApp;

/// The frame's body region text. `render` lays out header(Length 4) /
/// body(Min 8) / footer(Length 3), so the body is frame rows `4..height-3`.
/// Duplicating the chrome sizes here is deliberate: when `render` changes
/// its layout contract this test must be revisited, not silently pass.
fn body_text(app: &TuiApp, width: u16, height: u16) -> String {
    let frame = frame_text(app, width, height);
    let body_rows = height.saturating_sub(4 + 3);
    frame
        .lines()
        .skip(4)
        .take(body_rows as usize)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rows that paint at least one non-whitespace cell.
fn visible_row_count(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

fn body_row_counts(page: AppPage) -> (usize, usize) {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(page));
    let reference = visible_row_count(&body_text(&app, 120, 36));
    let minimum = visible_row_count(&body_text(&app, 54, 16));
    (reference, minimum)
}

#[test]
fn every_page_paints_visible_body_rows_at_reference_and_minimum_sizes() {
    let pages = [
        AppPage::Performance,
        AppPage::Applications,
        AppPage::Services,
        AppPage::System,
        AppPage::Startup,
        AppPage::Users,
        AppPage::AppHistory,
    ];
    let mut minimums = Vec::with_capacity(pages.len());
    for page in pages {
        let (reference, minimum) = body_row_counts(page);
        // Measured contract (2026-08-17): every page paints its ENTIRE
        // body region — 29 of 29 rows at 120x36, 9 of 9 at 54x16. Anything
        // less means layout math ate content rows (the 2026-08-10 blindspot
        // class), so pin the exact counts, not a loose floor.
        assert_eq!(
            reference, 29,
            "{page:?} must paint all 29 body rows at 120x36"
        );
        assert_eq!(minimum, 9, "{page:?} must paint all 9 body rows at 54x16");
        minimums.push((page, reference, minimum));
    }
    // Non-vacuity guard for the extractor itself: the sweep really visited
    // every page (a broken body_text that returns "" would fail the exact
    // counts above, but an empty pages list would pass silently).
    assert_eq!(minimums.len(), 7, "the sweep must cover all seven pages");
}

#[test]
fn per_core_grid_renders_every_demo_core_at_reference_size() {
    // The demo snapshot carries four per-core series (52/41/34/22%), so the
    // reference CPU view must advertise C00..C03 — the shared cores title
    // plus every core cell, none silently eaten by the layout.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    let frame = frame_text(&app, 120, 36);
    for core in ["C00", "C01", "C02", "C03"] {
        assert!(
            frame.contains(core),
            "per-core grid lost {core} at 120x36 — a partial grid is the 2026-08-10 blindspot"
        );
    }
}

#[test]
fn reference_height_keeps_the_complete_demo_topology_across_the_width_range() {
    // At the reference height every legal width can show this four-core demo
    // topology without engaging the short-terminal viewport.
    for width in [54, 60, 70, 80, 100, 120] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
        let frame = frame_text(&app, width, 36);
        let first = frame.contains("C00");
        let last = frame.contains("C03");
        assert_eq!(
            first, last,
            "partial per-core grid at width {width}: first={first} last={last}"
        );
    }
}
