use gpui::{
    AppContext, Context, IntoElement, Modifiers, MouseButton, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use taskmanager_theme::Theme;
use taskmanager_ui::primitives::selectable_text::SelectableText;

const SAMPLE: &str = "alpha beta gamma";
const LONG_SAMPLE: &str =
    "A deliberately long selectable value that must keep its complete clipboard truth";

struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(20.0)).text_size(px(16.0)).child(
            SelectableText::new("selectable-text-test", SAMPLE, Theme::dark().palette())
                .debug_selector("tm-test-selectable-text"),
        )
    }
}

fn mounted(cx: &mut TestAppContext) -> (gpui::WindowHandle<Harness>, VisualTestContext) {
    cx.update(taskmanager_ui::init);
    let window = cx.add_window(|_window, _cx| Harness);
    cx.simulate_window_resize(window.into(), size(px(320.0), px(120.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

struct SingleLineHarness;

impl Render for SingleLineHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(12.0)).text_size(px(16.0)).child(
            div().w(px(96.0)).child(
                SelectableText::new(
                    "single-line-selectable",
                    LONG_SAMPLE,
                    Theme::dark().palette(),
                )
                .single_line()
                .debug_selector("tm-test-single-line-selectable"),
            ),
        )
    }
}

#[gpui::test]
async fn single_line_readout_stays_bounded_but_select_all_copies_full_truth(
    cx: &mut TestAppContext,
) {
    cx.update(taskmanager_ui::init);
    let window = cx.add_window(|_window, _cx| SingleLineHarness);
    cx.simulate_window_resize(window.into(), size(px(180.0), px(80.0)));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let bounds = visual
        .debug_bounds("tm-test-single-line-selectable")
        .expect("single-line selectable must render");
    assert!(
        bounds.size.width <= px(96.0) && bounds.size.height <= px(28.0),
        "single-line selectable escaped its bounded row: {bounds:?}"
    );
    visual.simulate_click(bounds.center(), Modifiers::none());
    visual.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some(LONG_SAMPLE),
        "ellipsis is paint-only; selection retains the complete value"
    );
}

#[gpui::test]
async fn real_pointer_drag_copies_only_the_selected_substring(cx: &mut TestAppContext) {
    let (_window, mut visual) = mounted(cx);
    let bounds = visual
        .debug_bounds("tm-test-selectable-text")
        .expect("selectable text must expose real layout bounds");
    let start = point(bounds.left() + bounds.size.width * 0.12, bounds.center().y);
    let end = point(bounds.left() + bounds.size.width * 0.68, bounds.center().y);

    visual.simulate_mouse_move(start, None, Modifiers::none());
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    visual.simulate_keystrokes("ctrl-c");

    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("Ctrl+C must write the dragged selection");
    assert!(!copied.is_empty());
    assert_ne!(copied.as_str(), SAMPLE);
    assert!(
        SAMPLE.contains(copied.as_str()),
        "clipboard text must be a contiguous slice of the rendered value: {copied}"
    );
}

#[gpui::test]
async fn focused_select_all_and_copy_use_the_read_only_text_context(cx: &mut TestAppContext) {
    let (_window, mut visual) = mounted(cx);
    let center = visual
        .debug_bounds("tm-test-selectable-text")
        .expect("selectable text must render")
        .center();
    visual.simulate_click(center, Modifiers::none());
    visual.simulate_keystrokes("ctrl-a ctrl-c");

    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .expect("Ctrl+A then Ctrl+C must copy the complete value");
    assert_eq!(copied.as_str(), SAMPLE);
}

struct MultiHarness;

impl Render for MultiHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .children([
                SelectableText::new("selectable-a", "first value", Theme::dark().palette())
                    .debug_selector("tm-test-selectable-a")
                    .selected_debug_selector("tm-test-selectable-a-selected"),
                SelectableText::new("selectable-b", "second value", Theme::dark().palette())
                    .debug_selector("tm-test-selectable-b")
                    .selected_debug_selector("tm-test-selectable-b-selected"),
            ])
    }
}

#[gpui::test]
async fn a_window_paints_only_one_text_selection_at_a_time(cx: &mut TestAppContext) {
    cx.update(taskmanager_ui::init);
    let window = cx.add_window(|_window, _cx| MultiHarness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);

    let first = visual
        .debug_bounds("tm-test-selectable-a")
        .expect("first text must render");
    visual.simulate_click(first.center(), Modifiers::none());
    visual.simulate_keystrokes("ctrl-a");
    visual.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        visual
            .debug_bounds("tm-test-selectable-a-selected")
            .expect("the first selection probe must render")
            .size
            .width,
        px(1.0)
    );

    let second = visual
        .debug_bounds("tm-test-selectable-b")
        .expect("second text must render");
    visual.simulate_click(second.center(), Modifiers::none());
    visual.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("second value"),
        "the second element must own focus and the active selection"
    );
    visual.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        visual
            .debug_bounds("tm-test-selectable-a-selected")
            .expect("the first selection probe must update")
            .size
            .width,
        px(0.0),
        "starting a second selection must clear the first highlight"
    );
    assert_eq!(
        visual
            .debug_bounds("tm-test-selectable-b-selected")
            .expect("the second selection probe must render")
            .size
            .width,
        px(1.0)
    );
}
