use super::{
    MIN_THUMB_SIZE, ScrollbarInnerState, ScrollbarShow, ThumbGeometry, drag_percentage,
    fade_opacity, is_scrollbar_visible, thumb_geometry, thumb_geometry_for_track,
    track_click_offset, track_click_percentage,
};
use gpui::{point, px};
use std::time::{Duration, Instant};
#[test]
fn thumb_geometry_hides_when_nothing_to_scroll() {
    // Scroll area <= container: no thumb, no division (7.6-1).
    assert_eq!(
        thumb_geometry(100.0, 100.0, 0.0, 0.0),
        ThumbGeometry {
            thumb_length: 0.0,
            thumb_start: 0.0,
            visible: false,
        }
    );
    assert!(!thumb_geometry(80.0, 100.0, -20.0, 0.0).visible);
}

#[test]
fn thumb_geometry_scales_and_clamps_min_size() {
    let g = thumb_geometry(1000.0, 100.0, 0.0, 0.0);
    assert!(g.visible);
    // thumb = 100/1000*100 = 10 -> clamped to MIN_THUMB_SIZE.
    assert_eq!(g.thumb_length, MIN_THUMB_SIZE);
    // Mid-scroll positions the thumb proportionally further down.
    let mid = thumb_geometry(1000.0, 100.0, -450.0, 0.0);
    let start = thumb_geometry(1000.0, 100.0, 0.0, 0.0);
    assert!(mid.thumb_start > start.thumb_start);
    assert!(start.thumb_start <= 0.0);
}

#[test]
fn thumb_geometry_stays_inside_a_compact_inset_track() {
    let geometry = thumb_geometry_for_track(1000.0, 24.0, 16.0, -976.0);

    assert!(geometry.visible);
    assert_eq!(
        geometry.thumb_length, 16.0,
        "the thumb cannot exceed its track"
    );
    assert!(geometry.thumb_start >= 0.0);
    assert!(
        geometry.thumb_start + geometry.thumb_length <= 16.0,
        "compact rail geometry must not paint outside the track: {geometry:?}"
    );
}

#[test]
fn thumb_geometry_uses_visual_track_length_but_viewport_range() {
    let start = thumb_geometry_for_track(1000.0, 100.0, 92.0, 0.0);
    let end = thumb_geometry_for_track(1000.0, 100.0, 92.0, -900.0);

    assert_eq!(start.thumb_length, MIN_THUMB_SIZE);
    assert_eq!(start.thumb_start, 0.0);
    assert_eq!(end.thumb_start + end.thumb_length, 92.0);
}

#[test]
fn painted_thumb_caps_reach_the_inset_hairline_at_the_scroll_end() {
    // A rail's hairline is inset by four pixels, while the scrollbar itself
    // must use the full axis. The painted capsule then applies THUMB_INSET
    // exactly once: at max scroll its visible end is the hairline's end, not
    // another four pixels short.
    let start_geometry = thumb_geometry_for_track(1000.0, 300.0, 300.0, 0.0);
    let end_geometry = thumb_geometry_for_track(1000.0, 300.0, 300.0, -700.0);
    let painted_start = start_geometry.thumb_start + super::THUMB_INSET;
    let painted_end = end_geometry.thumb_start + end_geometry.thumb_length - super::THUMB_INSET;
    assert_eq!(painted_start, super::THUMB_INSET);
    assert_eq!(painted_end, 300.0 - super::THUMB_INSET);
}

#[test]
fn track_click_jump_clamps_percentage() {
    assert_eq!(track_click_percentage(0.0, 0.0, 100.0, 48.0), 0.0);
    assert_eq!(track_click_percentage(200.0, 0.0, 100.0, 48.0), 1.0);
    assert!((track_click_percentage(50.0, 0.0, 100.0, 48.0) - 0.5).abs() < 1e-4);
    // Degenerate track: no movement.
    assert_eq!(track_click_percentage(50.0, 0.0, 40.0, 48.0), 0.0);
}

#[test]
fn track_click_offset_uses_scrollable_range_not_content_size() {
    // Content 1000 / viewport 100: 50% of the RANGE (900), not of the
    // content — the old `-scroll_area * percentage` jumped to -500 and
    // the content overshot where the thumb was clicked.
    assert_eq!(track_click_offset(1000.0, 100.0, 0.5), -450.0);
    assert_eq!(track_click_offset(1000.0, 100.0, 0.0), 0.0);
    assert_eq!(track_click_offset(1000.0, 100.0, 1.0), -900.0);
    // Nothing to scroll: never leaves 0 (defensive).
    assert_eq!(track_click_offset(80.0, 100.0, 1.0), 0.0);
}

#[test]
fn drag_percentage_tracks_pointer_with_grab_offset() {
    // Holding the thumb's center: pointer == grab_offset + track_start -> 0.
    assert_eq!(drag_percentage(10.0, 10.0, 0.0, 100.0, 48.0), 0.0);
    // Out-of-range clamps both ends.
    assert_eq!(drag_percentage(-50.0, 0.0, 0.0, 100.0, 48.0), 0.0);
    assert_eq!(drag_percentage(500.0, 0.0, 0.0, 100.0, 48.0), 1.0);
}

#[test]
fn fade_opacity_follows_delay_then_power_falloff() {
    assert_eq!(fade_opacity(0.0), 1.0);
    assert_eq!(fade_opacity(1.9), 1.0);
    let mid = fade_opacity(2.5);
    assert!(mid < 1.0 && mid > 0.0);
    assert_eq!(fade_opacity(3.0), 0.0);
    assert_eq!(fade_opacity(10.0), 0.0);
}

#[test]
fn visibility_respects_show_mode_drag_and_hover() {
    assert!(is_scrollbar_visible(
        false,
        false,
        Some(0.5),
        ScrollbarShow::Scrolling
    ));
    assert!(!is_scrollbar_visible(
        false,
        false,
        Some(4.0),
        ScrollbarShow::Scrolling
    ));
    assert!(is_scrollbar_visible(
        true,
        false,
        Some(10.0),
        ScrollbarShow::Scrolling
    ));
    assert!(is_scrollbar_visible(
        false,
        false,
        None,
        ScrollbarShow::Always
    ));
    // Hover mode: visible only while hovered/dragged (was a dead stub).
    assert!(is_scrollbar_visible(
        false,
        true,
        None,
        ScrollbarShow::Hover
    ));
    assert!(is_scrollbar_visible(
        true,
        false,
        None,
        ScrollbarShow::Hover
    ));
    assert!(!is_scrollbar_visible(
        false,
        false,
        None,
        ScrollbarShow::Hover
    ));
    // Hovering also pins the thumb in Scrolling mode.
    assert!(is_scrollbar_visible(
        false,
        true,
        Some(10.0),
        ScrollbarShow::Scrolling
    ));
}

#[test]
fn inner_state_records_scroll_events() {
    let now = Instant::now();
    let offset = point(px(-25.0), px(0.0));
    let state = ScrollbarInnerState::default().with_scroll(offset, now);
    assert_eq!(state.last_offset, offset);
    assert!(state.last_scroll.is_some());
}

#[test]
fn drag_refresh_coalesces_events_but_keeps_the_latest_offset() {
    let now = Instant::now();
    let mut state = ScrollbarInnerState::default();
    assert!(state.allow_drag_refresh(now, 60));
    let latest = point(px(-40.0), px(0.0));
    state = state.with_scroll(latest, now);
    assert!(!state.allow_drag_refresh(now, 60));
    assert!(state.refresh_pending);
    assert_eq!(state.last_offset, latest);
    assert!(
        state
            .frame_started()
            .allow_drag_refresh(now + Duration::from_millis(20), 60)
    );
}
