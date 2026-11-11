use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation, TextInput};
use iced::advanced::widget::{Id, Tree};
use iced::{Rectangle, Size};
use taskmanager_application::AppPage;
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::{InfoSortCol, InfoTable};

use crate::app::{FocusTarget, Message};

#[derive(Default)]
struct StartupSearchBounds {
    bounds: Option<Rectangle>,
}

impl Operation<()> for StartupSearchBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn TextInput) {
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::StartupSearch,
            )))
        {
            self.bounds = Some(bounds);
        }
    }
}

#[derive(Default)]
struct StartupHeaderBounds {
    bounds: Option<Rectangle>,
}

impl Operation<()> for StartupHeaderBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::InfoSortHeader {
                    table: InfoTable::Startup,
                    column: InfoSortCol::Status,
                },
            )))
        {
            self.bounds = Some(bounds);
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

fn startup_control_bounds(size: Size, renderer: &iced::Renderer) -> (Rectangle, Rectangle) {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let mut view = crate::ui::view(&app);
    let mut tree = Tree::new(&view);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));

    let mut search = StartupSearchBounds::default();
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut search);
    let mut header = StartupHeaderBounds::default();
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut header);
    (
        search.bounds.expect("the page mounts its search input"),
        header.bounds.expect("the page mounts its sortable header"),
    )
}

#[test]
fn startup_search_slot_stays_inside_responsive_viewports() {
    let renderer = renderer();
    for size in [
        Size::new(480.0, 600.0),
        Size::new(640.0, 480.0),
        Size::new(720.0, 480.0),
        Size::new(1180.0, 780.0),
        Size::new(1280.0, 720.0),
        Size::new(1280.0, 1100.0),
        Size::new(2048.0, 540.0),
        Size::new(1920.0, 1080.0),
    ] {
        let (search, header) = startup_control_bounds(size, &renderer);
        assert!(
            search.width > 0.0
                && search.height > 0.0
                && search.x >= 0.0
                && search.y >= 0.0
                && search.x + search.width <= size.width + 0.5
                && search.y + search.height <= size.height + 0.5,
            "Startup search stays within {size:?}: {search:?}"
        );
        assert!(
            header.y >= search.y + search.height,
            "the table header remains below search in {size:?}: {search:?}, {header:?}"
        );
    }
}
