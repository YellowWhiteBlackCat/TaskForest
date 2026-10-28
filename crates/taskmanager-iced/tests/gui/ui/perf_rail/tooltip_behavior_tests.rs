// test-intent: behavior
//! Interaction evidence for the production Performance rail tooltip consumer.

use super::super::{RailCategory, RailRow, RailValueFormat, device_card};
use crate::app::{Message, PerfDevice};
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::Tree;
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Element, Event, Point, Rectangle, Size, Vector, mouse};
use std::rc::Rc;
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

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

fn dispatch(
    card: &mut Element<'static, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    event: Event,
    cursor: mouse::Cursor,
) -> Vec<Message> {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        card.as_widget_mut().update(
            tree,
            &event,
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard,
            &mut shell,
            viewport,
        );
    }
    messages
}

fn tooltip_is_open(
    card: &mut Element<'static, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
) -> bool {
    card.as_widget_mut()
        .overlay(tree, Layout::new(node), renderer, viewport, Vector::ZERO)
        .is_some()
}

fn key_pressed(named: Named, code: Code) -> Event {
    let key = Key::Named(named);
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(code),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    })
}

fn rail_row() -> RailRow {
    RailRow {
        device: PerfDevice::Cpu,
        heading: "CPU".into(),
        subtitle: String::new(),
        cap1: "20%".into(),
        cap2: String::new(),
        samples: Rc::from([10.0_f32, 20.0].as_slice()),
        max: 100.0,
        category: RailCategory::Cpu,
        value_format: RailValueFormat::Percent,
    }
}

fn theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
}

#[test]
fn mounted_rail_tooltip_opens_on_hover_and_dismisses_on_click_or_escape() {
    let mut card = device_card(rail_row(), theme(), false, false);
    let renderer = renderer();
    let viewport = Rectangle::new(Point::new(0.0, 0.0), Size::new(480.0, 160.0));
    let mut tree = Tree::new(&card);
    let node = card.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &Limits::new(Size::ZERO, viewport.size()),
    );
    let bounds = node.bounds();
    let pointer = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );

    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "the tooltip starts hidden"
    );

    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "pointer hover opens the tooltip"
    );

    let _press_messages = dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "pointer press dismisses the tooltip"
    );

    let release_messages = dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        release_messages
            .iter()
            .any(|message| matches!(message, Message::SelectPerfDevice(PerfDevice::Cpu))),
        "the tooltip does not block activation of its underlying card"
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "the click keeps the tooltip closed until the pointer exits"
    );

    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "moving inside the same hover does not reopen a clicked tooltip"
    );

    let outside = Point::new(bounds.x + bounds.width + 20.0, pointer.y);
    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        mouse::Cursor::Available(outside),
    );
    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "a new hover after pointer exit reopens the tooltip"
    );

    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Escape, Code::Escape),
        mouse::Cursor::Unavailable,
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "Escape dismisses the tooltip"
    );
}

#[test]
fn focused_rail_tooltip_opens_on_focus_and_escape_dismisses() {
    let mut card = device_card(rail_row(), theme(), false, true);
    let renderer = renderer();
    let viewport = Rectangle::new(Point::new(0.0, 0.0), Size::new(480.0, 160.0));
    let mut tree = Tree::new(&card);
    let node = card.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &Limits::new(Size::ZERO, viewport.size()),
    );
    let bounds = node.bounds();
    let pointer = Point::new(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );

    assert!(
        tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "a focused card opens the tooltip"
    );

    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Escape, Code::Escape),
        mouse::Cursor::Unavailable,
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "Escape dismisses a focus-opened tooltip"
    );

    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "hover does not reopen a tooltip dismissed while focused"
    );

    let mut fresh = device_card(rail_row(), theme(), false, true);
    let mut fresh_tree = Tree::new(&fresh);
    let fresh_node = fresh.as_widget_mut().layout(
        &mut fresh_tree,
        &renderer,
        &Limits::new(Size::ZERO, viewport.size()),
    );
    assert!(
        tooltip_is_open(
            &mut fresh,
            &mut fresh_tree,
            &fresh_node,
            &renderer,
            &viewport
        ),
        "a fresh focused card opens the tooltip again"
    );
}
