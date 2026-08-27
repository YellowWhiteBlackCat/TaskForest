//! Iced event subscriptions kept separate from the application reducer.

use super::{EVENT_POLL, IcedApp, Message};
use crate::keys::map_key;

impl IcedApp {
    /// The iced subscription stream: platform poll tick + keyboard events.
    pub fn subscription(&self) -> iced::Subscription<Message> {
        let tick = iced::Subscription::run_with("tick", |_| {
            iced::futures::stream::unfold((), |()| async move {
                futures_timer::Delay::new(EVENT_POLL).await;
                Some((Message::Tick, ()))
            })
        });
        let keyboard = iced::keyboard::listen().map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Message::Key(map_key(&key, modifiers))
            }
            iced::keyboard::Event::ModifiersChanged(modifiers) => {
                Message::ModifiersChanged(modifiers)
            }
            _ => Message::Tick,
        });
        let mut subscriptions = vec![tick, keyboard];
        subscriptions
            .push(iced::window::resize_events().map(|(_, size)| Message::WindowResized(size)));
        subscriptions.push(iced::window::close_requests().map(|_| Message::WindowCloseRequested));
        // The per-frame pump runs only while something actually animates: the
        // capture first-frame marker, an unfinished modal entrance, or the
        // warm-up spinner. Otherwise iced idles between events — subscribing
        // unconditionally would rebuild the view tree every frame for nothing.
        if self.frame_pump_active() {
            subscriptions.push(iced::window::frames().map(Message::Frame));
        }
        // A live process-column drag needs pointer tracking beyond the 6px
        // header edge: an iced `mouse_area` only reports motion while the
        // pointer is inside its own bounds, so a drag leaving the edge would
        // stall. While a session is open, raw cursor moves and the left
        // button's release feed the column-sizing reducer; everything else
        // maps to `None`, so idle pointer traffic never becomes messages.
        if self.process_column_sizing.drag.is_some() {
            subscriptions.push(iced::event::listen_with(process_column_drag_event));
        }
        iced::Subscription::batch(subscriptions)
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
