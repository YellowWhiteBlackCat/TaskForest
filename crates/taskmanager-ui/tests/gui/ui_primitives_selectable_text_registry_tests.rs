use super::*;

use gpui::{
    AppContext, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    VisualTestContext, Window, div, px, size,
};
use taskmanager_theme::Theme;

struct RegistryHarness;

impl Render for RegistryHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(12.0)).child(
            SelectableText::new(
                "registry-selectable",
                "released window",
                Theme::dark().palette(),
            )
            .debug_selector("tm-registry-selectable"),
        )
    }
}

fn registry_len(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| cx.global::<SelectionRegistry>().active_by_window.len())
}

#[gpui::test]
async fn repeated_closed_windows_leave_no_selection_registry_residue(cx: &mut TestAppContext) {
    cx.update(init);

    for _ in 0..12 {
        let window = cx.add_window(|_window, _cx| RegistryHarness);
        cx.simulate_window_resize(window.into(), size(px(240.0), px(80.0)));
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .expect("registry harness draws");
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let selectable = visual
            .debug_bounds("tm-registry-selectable")
            .expect("selectable value renders");
        visual.simulate_click(selectable.center(), Modifiers::none());
        assert_eq!(
            registry_len(cx),
            1,
            "the live window owns exactly one selection registry entry"
        );

        cx.update_window(window.into(), |_, window, _cx| window.remove_window())
            .expect("test window closes through the real GPUI lifecycle");
        // Flush the close callback's deferred weak-reference sweep after the
        // keyed element state has released its final strong handle.
        cx.update(|_| {});
        assert_eq!(
            registry_len(cx),
            0,
            "a closed window must not leave a weak selection tombstone"
        );
    }
}
