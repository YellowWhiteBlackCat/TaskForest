// test-intent: behavior
//! Production header-edge drag behavior: a real header press starts the
//! session, raw pointer events keep resizing after the cursor leaves the edge,
//! and release commits the final typed width.

use super::*;
use crate::app::IcedApp;
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use taskmanager_assets::embedded_fonts;

type TestElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

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

fn layout(
    element: &mut TestElement<'_>,
    tree: &mut iced::advanced::widget::Tree,
    renderer: &iced::Renderer,
    size: Size,
) -> Node {
    element
        .as_widget_mut()
        .layout(tree, renderer, &Limits::new(Size::ZERO, size))
}

fn press(
    element: &mut TestElement<'_>,
    tree: &mut iced::advanced::widget::Tree,
    node: &Node,
    renderer: &iced::Renderer,
    point: Point,
) -> Vec<Message> {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        element.as_widget_mut().update(
            tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(node),
            mouse::Cursor::Available(point),
            renderer,
            &mut clipboard,
            &mut shell,
            &Rectangle::with_size(node.size()),
        );
    }
    messages
}

fn feed_capture_event(app: &mut IcedApp, event: Event) {
    let message = crate::app::pointer_capture::process_column_drag_event(
        event,
        iced::event::Status::Ignored,
        iced::window::Id::unique(),
    )
    .expect("the active column drag consumes this raw event");
    let _ = app.update(message);
}

#[test]
fn header_edge_press_raw_motion_and_release_resize_the_live_column() {
    let theme = Theme::dark();
    let width = column_width(SortCol::Cpu);
    let mut element = header_cell(&theme, SortCol::Cpu, width, (SortCol::Cpu, SortDir::Desc));
    let mut tree = iced::advanced::widget::Tree::new(&element);
    let renderer = renderer();
    let layout_size = Size::new(200.0, 40.0);
    let node = layout(&mut element, &mut tree, &renderer, layout_size);
    let bounds = Layout::new(&node).bounds();
    let edge_point = Point::new(bounds.x + bounds.width - 3.0, bounds.center_y());
    let start_messages = press(&mut element, &mut tree, &node, &renderer, edge_point);

    assert!(start_messages.iter().any(|message| matches!(
        message,
        Message::BeginProcessColumnDrag {
            column: SortCol::Cpu,
            start_width
        } if *start_width == width
    )));
    assert!(
        !start_messages
            .iter()
            .any(|message| matches!(message, Message::SortBy(_))),
        "the trailing edge starts resizing without sorting the header"
    );

    let mut app = IcedApp::demo();
    for message in start_messages {
        let _ = app.update(message);
    }
    assert!(app.process_column_sizing.drag.is_some());
    assert_eq!(app.process_column_width(SortCol::Cpu), width);

    let anchor = Point::new(bounds.x + bounds.width + 80.0, bounds.center_y());
    assert!(
        anchor.x > bounds.x + bounds.width,
        "motion leaves the edge bounds"
    );
    feed_capture_event(
        &mut app,
        Event::Mouse(mouse::Event::CursorMoved { position: anchor }),
    );
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        width,
        "the first captured motion anchors the pointer delta"
    );

    let moved = Point::new(anchor.x + 40.0, anchor.y + 30.0);
    feed_capture_event(
        &mut app,
        Event::Mouse(mouse::Event::CursorMoved { position: moved }),
    );
    assert_eq!(
        app.process_column_width(SortCol::Cpu),
        width + 40.0,
        "captured movement outside the header updates the rendered width"
    );

    feed_capture_event(
        &mut app,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    );
    assert!(app.process_column_sizing.drag.is_none());
    assert!(
        app.config_draft()
            .process_col_widths
            .iter()
            .any(|entry| { entry.column == "CPU" && entry.width == width + 40.0 })
    );
}
