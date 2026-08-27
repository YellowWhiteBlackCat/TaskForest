use super::{Spinner, arc_points};
use gpui::{
    AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div, point, px,
};
use taskmanager_theme::Theme;
#[gpui::test]
async fn spinner_renders(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| Harness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
}

#[test]
fn arc_points_trace_a_closed_ring_when_sweep_is_one() {
    let o = point(px(0.0), px(0.0));
    let pts = arc_points(o, 16.0, 6.5, 0.0, 1.0);
    assert_eq!(pts.len(), 21);
    // First and last points coincide for a full turn.
    let first = pts.first().expect("non-empty");
    let last = pts.last().expect("non-empty");
    assert!(f32::from(first.x - last.x).abs() < 1e-3);
    assert!(f32::from(first.y - last.y).abs() < 1e-3);
}

#[test]
fn arc_points_start_angle_rotates_the_arc() {
    let o = point(px(0.0), px(0.0));
    let zero = arc_points(o, 16.0, 6.5, 0.0, 0.7);
    let half = arc_points(o, 16.0, 6.5, std::f32::consts::PI, 0.7);
    assert_eq!(zero.len(), half.len());
    // The two arcs start at opposite points of the ring.
    let a = zero.first().expect("non-empty");
    let b = half.first().expect("non-empty");
    assert!(f32::from(a.x - b.x).abs() > 5.0, "arcs must start apart");
}

#[derive(Default)]
struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(Spinner::new(Theme::dark().palette()).size(20.0))
    }
}
