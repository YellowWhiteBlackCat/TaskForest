// test-intent: behavior
//! The renderer-local input-modality tracker: keyboard input is the only
//! focus-visible source (the strict GPUI-parity policy), pointer presses
//! clear it, programmatic focus inherits the previous origin, and the shared
//! ring seam (`theme::focus_ring_color`) consumes the decision. The root
//! message wiring is proven through the headless `IcedApp::demo` update
//! loop; the raw pointer/keyboard event delivery itself is not
//! headless-drivable in iced.

use crate::app::Message;
use crate::input_modality::InputModality;
use crate::keys::IcedKey;
use crate::theme::focus_ring_color;
use crate::theme_binding::color;
use taskmanager_theme::Theme;

#[test]
fn pointer_input_clears_the_keyboard_ring_source() {
    assert!(InputModality::Keyboard.shows_focus_ring());
    assert!(!InputModality::Pointer.shows_focus_ring());
    // Programmatic focus inherits the previous origin: it never rings by
    // itself while the window's modality is programmatic.
    assert!(!InputModality::Programmatic.shows_focus_ring());
}

#[test]
fn ring_color_encodes_the_visibility_decision() {
    let theme = Theme::dark();
    let ring = color(theme.palette().ring);

    let keyboard_theme = theme.with_focus_visible(true);
    let stroke = focus_ring_color(&keyboard_theme, false);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (ring.r, ring.g, ring.b),
        "the ring hue comes from palette().ring"
    );
    assert_eq!(stroke.a, 1.0, "keyboard focus ⇒ ring drawn opaquely");

    let pointer_theme = theme.with_focus_visible(false);
    let pointer_stroke = focus_ring_color(&pointer_theme, false);
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

    let keyboard_theme = theme.with_focus_visible(true);
    let stroke = focus_ring_color(&keyboard_theme, true);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (danger.r, danger.g, danger.b),
        "destructive controls keep the danger ring hue"
    );
    assert_eq!(stroke.a, 1.0);

    let pointer_theme = theme.with_focus_visible(false);
    assert_eq!(focus_ring_color(&pointer_theme, true).a, 0.0);
}

#[test]
fn root_messages_feed_the_modality_tracker() {
    let mut app = crate::IcedApp::demo();
    assert!(!app.theme().focus_visible());

    // Any key press counts as keyboard input, even an unmappable one (the
    // `Other` bucket covers bare modifiers too).
    let _ = app.update(Message::Key(IcedKey::Other));
    assert!(
        app.theme().focus_visible(),
        "a key press marks keyboard input"
    );

    let _ = app.update(Message::PointerPressed);
    assert!(
        !app.theme().focus_visible(),
        "a pointer press over the root clears it"
    );
}

#[test]
fn modality_is_isolated_between_app_instances() {
    let mut keyboard = crate::IcedApp::demo();
    let mut pointer = crate::IcedApp::demo();

    let _ = keyboard.update(Message::Key(IcedKey::Other));
    let _ = pointer.update(Message::PointerPressed);

    assert!(keyboard.theme().focus_visible());
    assert!(!pointer.theme().focus_visible());
}
