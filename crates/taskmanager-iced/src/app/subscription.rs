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
        let keyboard = iced::keyboard::listen().map(message_for_keyboard_event);
        let mut subscriptions = vec![tick, keyboard];
        subscriptions
            .push(iced::window::resize_events().map(|(_, size)| Message::WindowResized(size)));
        subscriptions
            .push(iced::window::events().filter_map(|(_, event)| message_for_window_event(event)));
        subscriptions.push(iced::window::close_requests().map(|_| Message::WindowCloseRequested));
        // The per-frame pump runs only while something actually animates: the
        // capture first-frame marker, an unfinished modal entrance, or the
        // warm-up spinner. Otherwise iced idles between events — subscribing
        // unconditionally would rebuild the view tree every frame for nothing.
        if self.frame_pump_active() {
            subscriptions.push(iced::window::frames().map(Message::Frame));
        }
        // A live process-column drag needs pointer tracking beyond the 6px
        // header edge (an iced `mouse_area` only reports motion inside its
        // own bounds). The capture seam in [`pointer_capture`] owns that
        // subscription: it exists exactly while a session is open and maps
        // only the raw shapes the drag consumes.
        subscriptions.push(super::pointer_capture::subscription(
            self.process_column_sizing
                .drag
                .map(|_| super::pointer_capture::CapturedDrag::ProcessColumnWidth),
        ));
        iced::Subscription::batch(subscriptions)
    }
}

pub(crate) fn message_for_window_event(event: iced::window::Event) -> Option<Message> {
    match event {
        iced::window::Event::Unfocused => Some(Message::WindowUnfocused),
        // The window's DPI scale factor: the chart's sub-pixel alignment is
        // exact only against the product with the app render scale.
        iced::window::Event::Rescaled(scale_factor) => {
            Some(Message::WindowScaleFactorChanged(scale_factor))
        }
        _ => None,
    }
}

fn message_for_keyboard_event(event: iced::keyboard::Event) -> Message {
    match event {
        iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
            Message::Key(map_key(&key, modifiers))
        }
        iced::keyboard::Event::ModifiersChanged(modifiers) => Message::ModifiersChanged(modifiers),
        _ => Message::Tick,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/overlay_interaction_tests.rs"]
mod overlay_interaction_tests;
