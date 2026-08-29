// test-intent: behavior
//! The pointer-capture seam routes exactly the two raw event shapes an open
//! process-column drag consumes — cursor motion and the left release — and
//! discards every other raw event so the open session stays traffic-free.
//! (Whether a session is open stays reducer truth in `update::columns`; the
//! subscription only exists while it is.)

use super::process_column_drag_event;
use crate::app::Message;

#[test]
fn cursor_motion_advances_the_open_drag() {
    let event = iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: iced::Point::new(42.0, 7.0),
    });
    match process_column_drag_event(
        event,
        iced::event::Status::Ignored,
        iced::window::Id::unique(),
    ) {
        Some(Message::ProcessColumnDragMoved(position)) => {
            assert_eq!(position, iced::Point::new(42.0, 7.0));
        }
        _ => panic!("cursor motion is forwarded as ProcessColumnDragMoved"),
    }
}

#[test]
fn the_left_release_closes_the_open_drag() {
    let event = iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
        iced::mouse::Button::Left,
    ));
    match process_column_drag_event(
        event,
        iced::event::Status::Captured,
        iced::window::Id::unique(),
    ) {
        Some(Message::ProcessColumnDragReleased) => {}
        _ => panic!("the left release is forwarded as ProcessColumnDragReleased"),
    }
}

#[test]
fn every_other_raw_event_is_discarded() {
    let ignored = [
        iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
            iced::mouse::Button::Right,
        )),
        iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
        iced::Event::Mouse(iced::mouse::Event::CursorLeft),
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
            iced::keyboard::Modifiers::default(),
        )),
    ];
    for event in ignored {
        assert!(
            process_column_drag_event(
                event,
                iced::event::Status::Ignored,
                iced::window::Id::unique()
            )
            .is_none(),
            "the open drag consumes only motion and the left release"
        );
    }
}
