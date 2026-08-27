//! Pure scrollbar geometry, pointer mapping, and visibility policy.

use super::ScrollbarShow;

/// The hit strips stay wider than the painted thumb for pointer access.
pub const SCROLLBAR_WIDTH: f32 = 12.0;
pub const SCROLLBAR_HEIGHT: f32 = 8.0;
pub const MIN_THUMB_SIZE: f32 = 48.0;
pub const THUMB_WIDTH: f32 = 3.0;
pub const THUMB_ACTIVE_WIDTH: f32 = 5.0;
pub const THUMB_INSET: f32 = 4.0;
pub const FADE_OUT_DELAY: f32 = 2.0;
pub const FADE_OUT_DURATION: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThumbGeometry {
    pub thumb_length: f32,
    pub thumb_start: f32,
    pub visible: bool,
}

#[must_use]
pub fn thumb_geometry(
    scroll_area: f32,
    container: f32,
    scroll_offset: f32,
    margin_end: f32,
) -> ThumbGeometry {
    if scroll_area <= container {
        return hidden_thumb();
    }
    let thumb_length = (container / scroll_area * container)
        .max(MIN_THUMB_SIZE)
        .min(container);
    let travel = (container - margin_end - thumb_length).max(0.0);
    let progress = (-scroll_offset / (scroll_area - container)).clamp(0.0, 1.0);
    ThumbGeometry {
        thumb_length,
        thumb_start: progress * travel,
        visible: true,
    }
}

#[must_use]
pub fn thumb_geometry_for_track(
    scroll_area: f32,
    container: f32,
    track_length: f32,
    scroll_offset: f32,
) -> ThumbGeometry {
    if scroll_area <= container || container <= 0.0 || track_length <= 0.0 {
        return hidden_thumb();
    }
    let thumb_length = (container / scroll_area * track_length)
        .max(MIN_THUMB_SIZE)
        .min(track_length);
    let travel = (track_length - thumb_length).max(0.0);
    let progress = (-scroll_offset / (scroll_area - container)).clamp(0.0, 1.0);
    ThumbGeometry {
        thumb_length,
        thumb_start: progress * travel,
        visible: true,
    }
}

const fn hidden_thumb() -> ThumbGeometry {
    ThumbGeometry {
        thumb_length: 0.0,
        thumb_start: 0.0,
        visible: false,
    }
}

#[must_use]
pub fn track_click_offset(scroll_area: f32, container: f32, percentage: f32) -> f32 {
    -((scroll_area - container).max(0.0)) * percentage
}

#[must_use]
pub fn track_click_percentage(click: f32, track_start: f32, track_len: f32, thumb_len: f32) -> f32 {
    if track_len <= thumb_len {
        return 0.0;
    }
    ((click - thumb_len / 2.0 - track_start) / (track_len - thumb_len)).clamp(0.0, 1.0)
}

#[must_use]
pub fn drag_percentage(
    pointer: f32,
    grab_offset: f32,
    track_start: f32,
    track_len: f32,
    thumb_len: f32,
) -> f32 {
    if track_len <= thumb_len {
        return 0.0;
    }
    ((pointer - grab_offset - track_start) / (track_len - thumb_len)).clamp(0.0, 1.0)
}

#[must_use]
pub fn fade_opacity(elapsed_secs: f32) -> f32 {
    if elapsed_secs < FADE_OUT_DELAY {
        1.0
    } else if elapsed_secs >= FADE_OUT_DURATION {
        0.0
    } else {
        1.0 - (elapsed_secs - FADE_OUT_DELAY).powi(10)
    }
}

#[must_use]
pub fn is_scrollbar_visible(
    dragged: bool,
    hovering: bool,
    last_scroll_elapsed: Option<f32>,
    show: ScrollbarShow,
) -> bool {
    match show {
        ScrollbarShow::Always => true,
        ScrollbarShow::Hover => dragged || hovering,
        ScrollbarShow::Scrolling => {
            dragged
                || hovering
                || last_scroll_elapsed.is_some_and(|elapsed| elapsed < FADE_OUT_DURATION)
        }
    }
}
