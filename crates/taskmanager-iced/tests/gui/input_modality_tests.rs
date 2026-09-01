// test-intent: behavior
//! The renderer-local input-modality tracker: keyboard input is the only
//! focus-visible source (the strict GPUI-parity policy), pointer presses
//! clear it, programmatic focus inherits the previous origin, and the shared
//! ring seam (`theme::focus_ring_color`) consumes the decision. The root
//! message wiring is proven through the headless `IcedApp::demo` update
//! loop; the raw pointer/keyboard event delivery itself is not
//! headless-drivable in iced.

use super::{focus_visible, observe_keyboard, observe_pointer};
use crate::app::Message;
use crate::keys::IcedKey;
use crate::theme::focus_ring_color;
use crate::theme_binding::color;
use taskmanager_theme::Theme;

#[test]
fn pointer_input_clears_the_keyboard_ring_source() {
    observe_keyboard();
    assert!(focus_visible(), "keyboard input is the ring source");

    observe_pointer();
    assert!(!focus_visible(), "a pointer press clears it");
    // Programmatic focus inherits the previous origin: it never rings by
    // itself while the tracker sits on pointer input.
    assert!(!focus_visible());
}

#[test]
fn ring_color_encodes_the_visibility_decision() {
    let theme = Theme::dark();
    let ring = color(theme.palette().ring);

    observe_keyboard();
    let stroke = focus_ring_color(&theme, false);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (ring.r, ring.g, ring.b),
        "the ring hue comes from palette().ring"
    );
    assert_eq!(stroke.a, 1.0, "keyboard focus ⇒ ring drawn opaquely");

    observe_pointer();
    let pointer_stroke = focus_ring_color(&theme, false);
    assert_eq!(
        (pointer_stroke.r, pointer_stroke.g, pointer_stroke.b),
        (ring.r, ring.g, ring.b)
    );
    assert_eq!(pointer_stroke.a, 0.0, "pointer focus ⇒ no ring");
}

#[test]
fn destructive_rings_follow_the_same_visibility_rule() {
    let theme = Theme::dark();
    let danger = color(theme.palette().danger);

    observe_keyboard();
    let stroke = focus_ring_color(&theme, true);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (danger.r, danger.g, danger.b),
        "destructive controls keep the danger ring hue"
    );
    assert_eq!(stroke.a, 1.0);

    observe_pointer();
    assert_eq!(focus_ring_color(&theme, true).a, 0.0);
}

#[test]
fn root_messages_feed_the_modality_tracker() {
    let mut app = crate::IcedApp::demo();
    observe_pointer();

    // Any key press counts as keyboard input, even an unmappable one (the
    // `Other` bucket covers bare modifiers too).
    let _ = app.update(Message::Key(IcedKey::Other));
    assert!(focus_visible(), "a key press marks keyboard input");

    let _ = app.update(Message::PointerPressed);
    assert!(!focus_visible(), "a pointer press over the root clears it");
}
