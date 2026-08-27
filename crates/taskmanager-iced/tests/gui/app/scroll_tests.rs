use super::*;

#[test]
fn virtual_scroll_state_initializes_with_fallbacks() {
    let state = VirtualScrollState::new();
    assert_eq!(state.offset_x(), 0.0);
    assert_eq!(state.offset_y(), 0.0);
    assert_eq!(state.viewport_height(500.0), 500.0);
    assert_eq!(state.viewport_width(800.0), 800.0);
    assert_eq!(state.viewport_height(-10.0), 240.0);
    assert_eq!(state.viewport_width(f32::NAN), 240.0);
}

#[test]
fn ensure_row_visible_scrolls_down_when_row_is_below_viewport() {
    let mut state = VirtualScrollState::new();
    state.set_offset(0.0);
    // Viewport 300px, header 32px, row height 30px
    // Row 15: top = 32 + 15*30 = 482, bottom = 512
    // Bottom (512) > current_y (0) + vp (300) = 300 -> target = 512 - 300 = 212
    let target = state.ensure_row_visible(15, 30.0, 32.0, 300.0);
    assert_eq!(target, Some(212.0));
}

#[test]
fn ensure_row_visible_scrolls_up_when_row_is_above_viewport() {
    let mut state = VirtualScrollState::new();
    state.set_offset(300.0);
    // Row 2: top = 32 + 2*30 = 92, bottom = 122
    // Top (92) < current_y (300) -> target = 92
    let target = state.ensure_row_visible(2, 30.0, 32.0, 300.0);
    assert_eq!(target, Some(92.0));
}

#[test]
fn ensure_row_visible_returns_none_when_already_in_view() {
    let mut state = VirtualScrollState::new();
    state.set_offset(100.0);
    // Row 5: top = 32 + 5*30 = 182, bottom = 212
    // current_y = 100, current_y + vp = 400
    // 182 >= 100 and 212 <= 400 -> already visible
    let target = state.ensure_row_visible(5, 30.0, 32.0, 300.0);
    assert_eq!(target, None);
}
