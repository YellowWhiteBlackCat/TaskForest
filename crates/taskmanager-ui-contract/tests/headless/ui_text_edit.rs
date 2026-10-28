//! test-intent: behavior
//! Shared text-edit state: UTF-8-safe caret moves, selection replacement,
//! word/line selection, and the delete family.

use super::*;

#[test]
fn typing_inserts_at_the_caret_and_respects_utf8_boundaries() {
    let mut edit = TextEditState::new();
    let mut text = String::new();
    assert!(edit.insert_char(&mut text, 'a'));
    assert!(edit.insert_char(&mut text, 'b'));
    assert_eq!(text, "ab");
    assert_eq!(edit.caret_in(&text), 2);

    edit.set_caret(&text, 1);
    assert!(edit.insert_char(&mut text, 'é'));
    assert_eq!(text, "aéb");
    assert_eq!(edit.caret_in(&text), 3, "the caret follows the UTF-8 bytes");

    assert!(edit.move_caret(&text, TextMotion::Left, false));
    assert_eq!(
        edit.caret_in(&text),
        1,
        "left moves one character, not one byte"
    );
    assert!(edit.move_caret(&text, TextMotion::Right, false));
    assert_eq!(edit.caret_in(&text), 3);

    // A stale offset from a longer buffer clamps onto a boundary.
    edit.set_caret(&text, 99);
    assert_eq!(edit.caret_in(&text), text.len());
    edit.set_caret(&text, 2);
    assert_eq!(edit.caret_in(&text), 1, "a mid-character offset backs up");
}

#[test]
fn selection_replacement_and_the_delete_family_use_one_range_authority() {
    let mut edit = TextEditState::new();
    let mut text = "hello world".to_owned();
    edit.set_caret(&text, 6);
    edit.extend_selection(&text, 11);
    assert_eq!(edit.selection(&text), Some(6..11));
    assert_eq!(edit.selected_text(&text), Some("world"));

    assert!(edit.insert_char(&mut text, 'R'));
    assert_eq!(text, "hello R");
    assert_eq!(edit.selection(&text), None);
    assert_eq!(edit.caret_in(&text), 7);

    edit.set_caret(&text, 7);
    assert!(edit.delete_backward(&mut text));
    assert_eq!(text, "hello ");

    edit.set_caret(&text, 0);
    assert!(edit.delete_forward(&mut text));
    assert_eq!(text, "ello ");
    let mut empty = String::new();
    assert!(!edit.delete_forward(&mut empty));

    let mut words = "one two three".to_owned();
    edit.set_caret(&words, words.len());
    assert!(edit.delete_word_backward(&mut words));
    assert_eq!(
        words, "one two",
        "word backspace takes the preceding gap too"
    );
    edit.set_caret(&words, 0);
    assert!(edit.delete_word_forward(&mut words));
    assert_eq!(words, "two", "word delete takes the following gap too");

    assert!(edit.clear(&mut words));
    assert_eq!(words, "");
    assert_eq!(edit.caret_in(&words), 0);
}

#[test]
fn select_all_word_and_line_ranges_match_the_click_grammar() {
    let mut edit = TextEditState::new();
    let text = "192.168.1.1 aa:bb:cc\nsecond line".to_owned();
    assert_eq!(edit.select_word_at(&text, 5), 0..11);
    assert_eq!(edit.selected_text(&text), Some("192.168.1.1"));

    assert_eq!(edit.select_word_at(&text, 12), 12..20);
    assert_eq!(edit.selected_text(&text), Some("aa:bb:cc"));

    // A click on a separator selects that separator alone.
    assert_eq!(edit.select_word_at(&text, 20), 20..21);

    assert_eq!(edit.select_line_at(&text, 13), 0..20);
    assert_eq!(edit.select_line_at(&text, 25), 21..text.len());

    edit.select_all(&text);
    assert_eq!(edit.selection(&text), Some(0..text.len()));
}

#[test]
fn motions_collapse_or_extend_the_selection_and_cross_lines() {
    let mut edit = TextEditState::new();
    let text = "alpha beta\ngamma delta".to_owned();

    edit.set_caret(&text, 15);
    assert!(edit.move_caret(&text, TextMotion::WordLeft, false));
    assert_eq!(edit.caret_in(&text), 11);
    assert!(!edit.move_caret(&text, TextMotion::Home, false));
    assert_eq!(edit.caret_in(&text), 11, "home on a line start is a no-op");
    assert!(edit.move_caret(&text, TextMotion::End, false));
    assert_eq!(edit.caret_in(&text), text.len());
    assert!(edit.move_caret(&text, TextMotion::TextStart, false));
    assert_eq!(edit.caret_in(&text), 0);
    assert!(edit.move_caret(&text, TextMotion::TextEnd, false));
    assert_eq!(edit.caret_in(&text), text.len());

    // Shift-style moves grow a selection; a plain move collapses it to the
    // edge the motion faces.
    edit.set_caret(&text, 5);
    assert!(edit.move_caret(&text, TextMotion::Right, true));
    assert!(edit.move_caret(&text, TextMotion::Right, true));
    assert_eq!(edit.selection(&text), Some(5..7));
    assert!(edit.move_caret(&text, TextMotion::Left, false));
    assert_eq!(edit.selection(&text), None);
    assert_eq!(edit.caret_in(&text), 5);

    edit.set_caret(&text, 2);
    assert!(edit.move_caret(&text, TextMotion::Right, true));
    assert!(edit.move_caret(&text, TextMotion::Right, true));
    assert!(edit.move_caret(&text, TextMotion::Right, true));
    assert_eq!(edit.selection(&text), Some(2..5));
    assert!(edit.move_caret(&text, TextMotion::Right, false));
    assert_eq!(edit.caret_in(&text), 5, "right collapses onto the end edge");
}

#[test]
fn single_line_paste_collapses_breaks_and_multiline_paste_keeps_them() {
    let mut edit = TextEditState::new();
    let mut single = String::new();
    assert!(edit.insert_str(&mut single, "a\n\n\tb\r\u{7}c", true));
    assert_eq!(single, "a bc");
    assert_eq!(edit.caret_in(&single), single.len());

    let mut multi = String::new();
    assert!(edit.insert_str(&mut multi, "a\nb\tc", false));
    assert_eq!(multi, "a\nb\tc");
    assert_eq!(line_range(&multi, 3), 2..5);
}
