use super::{Segment, Segmented};
use gpui::{
    App, AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div,
};
use taskmanager_theme::Theme;

/// The connected track renders without panic across the active / hovered /
/// idle states (mirrors `pill_renders_both_states`). Verifies the flush
/// segment layout + the accent/hover/surface fills don't trip the layout
/// engine for any combination a call site builds.
#[gpui::test]
async fn segmented_renders_all_states(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| Harness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
}

#[derive(Default)]
struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        div().child(
            Segmented::new("harness-segmented", palette)
                .segment(
                    Segment::new(
                        "seg-active",
                        "Flat",
                        |_w: &mut Window, _c: &mut App| {},
                        |_: &bool, _w: &mut Window, _c: &mut App| {},
                    )
                    .active(true),
                )
                .segment(
                    Segment::new(
                        "seg-hovered",
                        "Tree",
                        |_w: &mut Window, _c: &mut App| {},
                        |_: &bool, _w: &mut Window, _c: &mut App| {},
                    )
                    .hovered(true),
                )
                .segment(Segment::new(
                    "seg-idle",
                    "Group",
                    |_w: &mut Window, _c: &mut App| {},
                    |_: &bool, _w: &mut Window, _c: &mut App| {},
                )),
        )
    }
}
