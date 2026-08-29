// test-intent: behavior
//! The selectable-value selection contract, at the level a user experiences
//! it: clicking into an address takes the whole address on a double-click,
//! word boundaries treat `.` and `:` as part of an identifier, selections
//! never start inside a multi-byte character, and beginning a selection
//! anywhere claims the window's single selection slot. The pixel hit-testing
//! itself needs a live text engine and is covered by real captures, not
//! here.

use super::{char_boundary_at_or_before, word_range};
use crate::IcedApp;
use crate::app::Message;

#[test]
fn a_double_click_inside_an_ipv4_address_selects_the_whole_address() {
    let label = "gateway 192.168.1.1 up";
    let clicked = label.find("68").expect("clicked mid-address");
    assert_eq!(
        word_range(label, clicked),
        label.find("192").unwrap()..label.find("1 up").unwrap() + 1,
        "dots belong to the address: the word is the whole dotted quad"
    );
}

#[test]
fn a_double_click_inside_a_mac_address_selects_the_whole_address() {
    let mac = "aa:bb:cc:dd:ee:ff";
    assert_eq!(
        word_range(mac, 10),
        0..mac.len(),
        "colons belong to the MAC: one click selects the whole identifier"
    );
}

#[test]
fn a_double_click_takes_the_identifier_under_it_never_both_sides() {
    assert_eq!(
        word_range("key=value", 0),
        0..3,
        "the key side selects alone"
    );
    assert_eq!(
        word_range("key=value", 8),
        4..9,
        "the value side selects alone"
    );
    assert_eq!(
        word_range("key=value", 3),
        3..4,
        "the separator selects as its own single run"
    );
    assert_eq!(
        word_range("key=value", 10_000),
        9..9,
        "a click past the text takes nothing"
    );
}

#[test]
fn a_selection_never_starts_inside_a_multibyte_character() {
    let value = "接口 if0";
    // Byte 1 is the middle of the first CJK character.
    assert_eq!(char_boundary_at_or_before(value, 1), 0);
    assert_eq!(
        char_boundary_at_or_before(value, 6),
        6,
        "the boundary after the CJK run is kept"
    );
    assert_eq!(
        char_boundary_at_or_before(value, 10_000),
        value.len(),
        "a caret past the text ends at the text end"
    );
}

#[test]
fn beginning_a_selection_claims_the_one_selection_slot() {
    let mut app = IcedApp::demo();
    let ipv4 = iced::advanced::widget::Id::new("net-ipv4");
    let mac = iced::advanced::widget::Id::new("net-mac");

    let _ = app.update(Message::TextSelectionClaimed(ipv4.clone()));
    assert_eq!(app.text_selection_owner(), Some(ipv4));

    // Selecting a different value moves the single slot — the previous
    // highlight clears on the next frame by contract.
    let _ = app.update(Message::TextSelectionClaimed(mac.clone()));
    assert_eq!(app.text_selection_owner(), Some(mac));
}
