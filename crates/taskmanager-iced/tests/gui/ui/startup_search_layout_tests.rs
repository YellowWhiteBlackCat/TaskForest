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
struct StartupControlBounds {
    segment: Option<Rectangle>,
    search: Option<Rectangle>,
}

impl Operation<()> for StartupControlBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::StartupStatusFilterSegment,
            )))
        {
            self.segment = Some(bounds);
        }
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn TextInput) {
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::StartupSearch,
            )))
        {
            self.search = Some(bounds);
        }
    }
}

#[derive(Default)]
struct StartupHeaderBounds {
    header: Option<Rectangle>,
    first_row: Option<Rectangle>,
    action: Option<Rectangle>,
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
            self.header = Some(bounds);
        }
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::StartupRowSwitch { index: 0 },
            )))
        {
            self.first_row = Some(bounds);
        }
        if id
            == Some(&Id::from(crate::focus::focus_id(
                FocusTarget::StartupControl,
            )))
        {
            self.action = Some(bounds);
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

fn startup_control_bounds(size: Size, renderer: &iced::Renderer) -> StartupControlBands {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Startup));
    let mut view = crate::ui::view(&app);
    let mut tree = Tree::new(&view);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));

    let mut controls = StartupControlBounds::default();
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut controls);
    let mut header = StartupHeaderBounds::default();
    view.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut header);
    StartupControlBands {
        segment: controls.segment.expect("the page mounts its status filter"),
        search: controls.search.expect("the page mounts its search input"),
        header: header.header.expect("the page mounts its sortable header"),
        first_row: header.first_row,
        action: header.action,
    }
}

struct StartupControlBands {
    segment: Rectangle,
    search: Rectangle,
    header: Rectangle,
    first_row: Option<Rectangle>,
    action: Option<Rectangle>,
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
        let bands = startup_control_bounds(size, &renderer);
        let segment = bands.segment;
        let search = bands.search;
        let header = bands.header;
        for control in [bands.segment, bands.search] {
            assert!(
                control.width > 0.0
                    && control.height > 0.0
                    && control.x >= 0.0
                    && control.y >= 0.0
                    && control.x + control.width <= size.width + 0.5
                    && control.y + control.height <= size.height + 0.5,
                "Startup control stays within {size:?}: {control:?}"
            );
        }
        assert!(
            bands.segment.x + bands.segment.width <= bands.search.x + 0.5
                && bands.segment.y <= bands.search.y + bands.search.height
                && bands.search.y <= bands.segment.y + bands.segment.height,
            "Startup controls stay one complete horizontal group in {size:?}: {segment:?}, {search:?}"
        );
        assert!(
            bands.header.y >= bands.search.y + bands.search.height,
            "the table header remains below both controls in {size:?}: {segment:?}, {search:?}, {header:?}"
        );
        if size.width >= 720.0 && size.height >= 480.0 {
            let first_row = bands
                .first_row
                .unwrap_or_else(|| panic!("the first Startup row is mounted in {size:?}"));
            let action = bands
                .action
                .unwrap_or_else(|| panic!("the Startup action bar is mounted in {size:?}"));
            assert!(
                first_row.y >= bands.header.y + bands.header.height
                    && first_row.y + first_row.height <= size.height + 0.5,
                "the first row stays inside its bounded slot in {size:?}: {first_row:?}"
            );
            assert!(
                action.y >= first_row.y + first_row.height
                    && action.y + action.height <= size.height + 0.5,
                "the action bar keeps the bottom safety band in {size:?}: {action:?}"
            );
        }
    }
}
