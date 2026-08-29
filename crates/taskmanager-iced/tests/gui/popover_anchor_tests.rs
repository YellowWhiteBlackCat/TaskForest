// test-intent: behavior
//! Floating-panel placement, stated as the user-visible geometry contract:
//! a context menu opens beside its row, flips above when the window has no
//! room below, stays inside the window at every edge, and a menu larger than
//! the window still renders pinned rather than off-screen. Every assertion
//! talks about the panel's on-screen rectangle relative to the row and the
//! window — never about the arithmetic that produced it.

use super::below;
use iced::{Point, Rectangle, Size};

const WINDOW: Rectangle = Rectangle::new(Point::ORIGIN, Size::new(1280.0, 800.0));
const GAP: f32 = 4.0;
const MARGIN: f32 = 4.0;
const MENU: Size = Size::new(320.0, 260.0);

/// A process-table row: nearly full width, one standard row tall.
fn row(x: f32, y: f32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(1160.0, 32.0))
}

fn panel_at(point: Point) -> Rectangle {
    Rectangle::new(point, MENU)
}

#[test]
fn a_menu_opened_on_a_row_in_the_open_appears_below_that_row() {
    let anchor = row(60.0, 300.0);
    let panel = panel_at(below(anchor, MENU, WINDOW, GAP));

    assert!(
        panel.y >= anchor.y + anchor.height,
        "the menu opens under the row, never over it"
    );
    assert!(
        panel.x + panel.width <= WINDOW.x + WINDOW.width
            && panel.y + panel.height <= WINDOW.y + WINDOW.height,
        "the menu fits fully inside the window"
    );
}

#[test]
fn a_menu_opened_on_a_row_at_the_bottom_of_the_window_flips_above_it() {
    let anchor = row(60.0, WINDOW.height - 60.0);
    let panel = panel_at(below(anchor, MENU, WINDOW, GAP));

    assert!(
        panel.y + panel.height <= anchor.y,
        "with no room below, the menu opens above the row"
    );
    assert!(
        panel.y >= WINDOW.y,
        "the flipped menu still stays inside the window"
    );
}

#[test]
fn a_menu_opened_on_a_row_at_the_right_edge_stays_inside_the_window() {
    let anchor = row(WINDOW.width - 80.0, 300.0);
    let panel = panel_at(below(anchor, MENU, WINDOW, GAP));

    assert!(
        panel.x + panel.width <= WINDOW.x + WINDOW.width - MARGIN,
        "the menu is pulled left so its right edge clears the window"
    );
    assert!(panel.x >= WINDOW.x + MARGIN);
}

#[test]
fn a_menu_that_fits_nowhere_on_a_short_window_pins_instead_of_spilling() {
    // A 200px-tall window with the row mid-window: the menu fits neither
    // above nor below it. The menu then pins toward the top of the window —
    // still fully visible — instead of spilling past the window edge.
    let short_window = Rectangle::new(Point::ORIGIN, Size::new(640.0, 200.0));
    let menu = Size::new(320.0, 180.0);
    let anchor = row(16.0, 80.0);
    let panel = Rectangle::new(below(anchor, menu, short_window, GAP), menu);

    assert!(
        panel.y >= short_window.y && panel.y + panel.height <= short_window.y + short_window.height,
        "the pinned menu stays fully inside the short window"
    );
    assert!(
        panel.y <= anchor.y,
        "a menu that fits nowhere never opens further below its row"
    );
}

#[test]
fn a_menu_opened_on_a_row_at_the_very_top_stays_inside_the_window() {
    let anchor = row(60.0, 0.0);
    let panel = panel_at(below(anchor, MENU, WINDOW, GAP));

    assert!(
        panel.y >= WINDOW.y,
        "a row at the top edge still yields a fully visible menu"
    );
    assert!(
        panel.x + panel.width <= WINDOW.x + WINDOW.width
            && panel.y + panel.height <= WINDOW.y + WINDOW.height
    );
}
