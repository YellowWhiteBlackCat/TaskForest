//! test-intent: behavior

use super::*;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Scrollable};
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Font, Pixels, Rectangle, Size, Vector};

#[derive(Default)]
struct Bounds {
    controls: Vec<Rectangle>,
    scrolls: Vec<(Rectangle, Rectangle)>,
}

impl Operation for Bounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, _id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        self.controls.push(bounds);
    }
    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        _translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        self.scrolls.push((bounds, content));
    }
}

#[test]
fn diagnostic_review_scrolls_the_payload_and_keeps_all_actions_inside_small_frames() {
    let renderer = iced::futures::executor::block_on(iced::Renderer::new(
        Font::DEFAULT,
        Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer");
    for (width, height) in [
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 400.0),
        (720.0, 960.0),
    ] {
        let size = Size::new(width, height);
        let mut app = crate::IcedApp::demo();
        let _ = app.update(Message::WindowResized(size));
        app.open_diagnostic_bundle();
        let Some(crate::app::LocalSurface::DiagnosticBundle(state)) = app.local_surface() else {
            panic!("review")
        };
        let mut view = diagnostic_bundle_overlay(&app, state);
        let mut tree = Tree::new(&view);
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        let mut bounds = Bounds::default();
        view.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        assert_eq!(
            bounds.controls.len(),
            2,
            "Export and Close must both remain reachable"
        );
        for control in bounds.controls {
            assert!(control.width > 0.0 && control.height > 0.0);
            assert!(control.x >= 0.0 && control.y >= 0.0);
            assert!(
                control.x + control.width <= width && control.y + control.height < height,
                "action bounds {control:?} exceed {size:?}"
            );
        }
        assert_eq!(
            bounds.scrolls.len(),
            1,
            "only the diagnostic payload scrolls"
        );
        let (viewport, content) = bounds.scrolls[0];
        assert!(viewport.width > 0.0 && viewport.height > 0.0);
        assert!(viewport.x + viewport.width <= width && viewport.y + viewport.height < height);
        assert!(
            content.height > viewport.height,
            "the full plan must remain available through the bounded viewport"
        );
    }
}
