use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::{Rectangle, Size};
use taskmanager_application::AppPage;
use taskmanager_assets::embedded_fonts;

use crate::app::{FocusTarget, Message};

#[derive(Default)]
struct ControlBounds {
    segment: Option<Rectangle>,
    search: Option<Rectangle>,
}

impl Operation<()> for ControlBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::ServiceStatusFilterSegment,
            )))
        {
            self.segment = Some(bounds);
        }
    }

    fn text_input(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _state: &mut dyn iced::advanced::widget::operation::TextInput,
    ) {
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::ServicesSearch,
            )))
        {
            self.search = Some(bounds);
        }
    }
}

fn renderer() -> iced::Renderer {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .expect("font system");
        for font in embedded_fonts() {
            fonts.load_font(font);
        }
    }
    iced::futures::executor::block_on(iced::Renderer::new(
        crate::theme_binding::BUNDLED_UI_FONT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("software renderer")
}

fn control_bounds(size: Size, renderer: &iced::Renderer) -> ControlBounds {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let mut view = crate::ui::view(&app);
    let mut tree = Tree::new(&view);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));
    let mut bounds = ControlBounds::default();
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut bounds);
    bounds
}

#[test]
fn services_status_filter_and_search_stay_inside_responsive_viewports() {
    let renderer = renderer();
    for size in [
        Size::new(480.0, 600.0),
        Size::new(640.0, 480.0),
        Size::new(720.0, 480.0),
        Size::new(1180.0, 780.0),
        Size::new(1280.0, 720.0),
        Size::new(2048.0, 540.0),
        Size::new(1920.0, 1080.0),
    ] {
        let bounds = control_bounds(size, &renderer);
        let segment = bounds.segment.expect("the status filter mounts");
        let search = bounds.search.expect("the Services search mounts");
        for control in [segment, search] {
            assert!(
                control.width > 0.0
                    && control.height > 0.0
                    && control.x >= 0.0
                    && control.y >= 0.0
                    && control.x + control.width <= size.width + 0.5
                    && control.y + control.height <= size.height + 0.5,
                "Services control stays within {size:?}: {control:?}"
            );
        }
        assert!(
            segment.x + segment.width <= search.x + 0.5,
            "status filter and search keep their control-band order in {size:?}: {segment:?}, {search:?}"
        );
    }
}
