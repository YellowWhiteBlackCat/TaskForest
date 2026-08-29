//! Typed pointer-capture sessions for drags that must outlive widget bounds.
//!
//! An iced `mouse_area` only reports motion while the pointer stays inside
//! its own bounds, so any drag that may leave its origin — the process-table
//! column resize today; scrollbar rails and slider tracks as the component
//! layer grows — needs the raw runtime pointer stream. This module is the one
//! seam for that: the owning reducer keeps the session truth (which drag, the
//! anchor, the start width), and this module turns an open session into the
//! single raw subscription and maps only the event shapes that session
//! consumes into the owner's messages. With no session open there is no raw
//! subscription at all, so idle pointer traffic stays free (the same doctrine
//! the frame pump follows in [`super::subscription`]).

use super::Message;

/// The drags that may hold the raw pointer stream. One variant per owning
/// reducer domain; the open kind routes raw events without re-checking
/// surfaces in the subscription closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CapturedDrag {
    /// Applications-table column width; `update::columns` owns the session
    /// and the reducer consumes the mapped messages.
    ProcessColumnWidth,
}

/// The raw-event subscription for one open capture session, or none while no
/// session is open — iced drops the stream entirely between drags.
pub(crate) fn subscription(capture: Option<CapturedDrag>) -> iced::Subscription<Message> {
    match capture {
        None => iced::Subscription::none(),
        Some(CapturedDrag::ProcessColumnWidth) => {
            iced::event::listen_with(process_column_drag_event)
        }
    }
}

/// Map the raw runtime pointer events an open process-column drag needs.
/// Cursor motion advances the session; the left release closes it. All other
/// events are discarded so the subscription stays traffic-free outside the
/// two shapes the drag actually consumes.
fn process_column_drag_event(
    event: iced::Event,
    _status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match event {
        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::ProcessColumnDragMoved(position))
        }
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
            Some(Message::ProcessColumnDragReleased)
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/pointer_capture_tests.rs"]
mod tests;
