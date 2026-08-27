//! Text-input unit tests (line split).

mod tests_inner {
    use crate::inputs::text_input::handler::{byte_range_from_utf16, utf16_range_from_bytes};
    use crate::inputs::text_input::{
        TextInput, TextInputState, TextSelection, init, next_boundary, previous_boundary,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext, Window,
        div,
    };
    use taskmanager_theme::Theme;
    #[test]
    fn boundaries_skip_cr_and_char_boundaries() {
        // CRLF paste: caret never lands on \r (6.6-1).
        assert_eq!(previous_boundary("a\r\nb", 3), 2);
        assert_eq!(next_boundary("a\r\nb", 2), 3);
        // CJK: one char = multiple bytes.
        assert_eq!(next_boundary("中文", 0), 3);
        assert_eq!(previous_boundary("中文", 3), 0);
        // ASCII.
        assert_eq!(previous_boundary("abc", 2), 1);
        assert_eq!(next_boundary("abc", 1), 2);
    }

    #[test]
    fn utf16_round_trips_cjk_and_ascii() {
        let text = "a中b";
        assert_eq!(byte_range_from_utf16(text, 1..2), (1, 4));
        assert_eq!(byte_range_from_utf16(text, 0..1), (0, 1));
        assert_eq!(utf16_range_from_bytes(text, 1..4), 1..2);
        assert_eq!(utf16_range_from_bytes(text, 4..5), 2..3);
    }

    #[test]
    fn selection_head_and_collapse() {
        let mut sel = TextSelection::caret(3);
        assert_eq!(sel.head(), 3);
        sel.select(1..5, false);
        assert_eq!(sel.head(), 5);
        sel.collapse_to(2);
        assert!(sel.range.is_empty());
    }

    /// Behavioral: a failing validator rejects the whole pending string for
    /// both insertion and deletion (absorption 6.6-2).
    #[gpui::test]
    async fn validate_rolls_back_whole_string_edits(cx: &mut TestAppContext) {
        let ok = |s: &str| s.len() <= 4;
        let state = cx.new(|cx| {
            let mut state = TextInputState::new(cx);
            state.set_value("ab", cx);
            state.set_validate(ok, cx);
            state
        });
        state.update(cx, |state, cx| {
            state.move_to_end(cx);
            state.replace_selection("cd", cx);
            assert_eq!(state.text, "abcd");
            // Exceeds 4 chars -> whole edit rejected.
            state.replace_selection("e", cx);
            assert_eq!(state.text, "abcd");
        });
    }

    struct Harness {
        state: Entity<TextInputState>,
    }

    impl Render for Harness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div().child(TextInput::new(self.state.clone(), Theme::dark().palette()))
        }
    }

    #[gpui::test]
    async fn keyboard_editing_flow(cx: &mut TestAppContext) {
        let window = cx.add_window(|_window, cx| {
            let state = cx.new(|cx| TextInputState::new(cx));
            Harness { state }
        });
        cx.update(init);
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();

        let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
        vcx.update(|window, cx| window.draw(cx).clear());
        let handle = window
            .read_with(&vcx, |harness, app| {
                harness.state.read(app).focus_handle().clone()
            })
            .unwrap();
        vcx.update(|window, _| handle.focus(window));
        // Paint once more so the focused input registers its platform handler.
        vcx.update(|window, cx| window.draw(cx).clear());
        vcx.simulate_input("hi");
        let value = window
            .read_with(&vcx, |harness, app| {
                harness.state.read(app).value().to_string()
            })
            .unwrap();
        assert_eq!(value, "hi");

        vcx.simulate_keystrokes("home");
        vcx.simulate_input("_");
        let value = window
            .read_with(&vcx, |harness, app| {
                harness.state.read(app).value().to_string()
            })
            .unwrap();
        assert_eq!(value, "_hi");

        vcx.simulate_keystrokes("backspace");
        let value = window
            .read_with(&vcx, |harness, app| {
                harness.state.read(app).value().to_string()
            })
            .unwrap();
        assert_eq!(value, "hi");
    }
}
