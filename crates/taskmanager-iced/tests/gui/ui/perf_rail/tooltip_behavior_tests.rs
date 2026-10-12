// test-intent: behavior
//! Interaction evidence for the production Performance rail tooltip consumer.

use super::super::{RailCategory, RailRow, RailValueFormat, device_card};
use crate::app::{FocusTarget, Message, PerfDevice};
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Element, Event, Point, Rectangle, Size, Vector, mouse};
use std::rc::Rc;
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

struct FocusTargetOperation {
    target: Id,
    matched: bool,
}

impl Operation<()> for FocusTargetOperation {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            state.focus();
            self.matched = true;
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

#[test]
fn mounted_rail_tooltip_opens_on_hover_not_focus_and_dismisses_on_exit_not_click_or_escape() {
    let theme = Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    );
    let row = RailRow {
        device: PerfDevice::Cpu,
        heading: "CPU".into(),
        subtitle: String::new(),
        cap1: "20%".into(),
        cap2: String::new(),
        samples: Rc::from([10.0_f32, 20.0].as_slice()),
        max: 100.0,
        category: RailCategory::Cpu,
        value_format: RailValueFormat::Percent,
    };
    let mut card = device_card(row, theme, false);
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
        "the native tooltip starts hidden"
    );

    // Focus the actual card through Iced's focusable operation, then activate
    // it by keyboard. The card responds, but the native tooltip has no focus
    // trigger, so no overlay appears.
    let mut focus = FocusTargetOperation {
        target: Id::from(crate::focus::focus_id(FocusTarget::PerfDeviceTab(
            PerfDevice::Cpu,
        ))),
        matched: false,
    };
    card.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
    assert!(focus.matched, "the mounted rail card is keyboard focusable");
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "keyboard focus alone does not open Iced's native tooltip"
    );
    let keyboard_activation = dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(
        keyboard_activation
            .iter()
            .any(|message| matches!(message, Message::SelectPerfDevice(PerfDevice::Cpu))),
        "keyboard activation still reaches the actual rail-card action"
    );
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "keyboard activation does not open the native tooltip"
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
        "one pointer-hover event opens the zero-delay native tooltip"
    );

    dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "pointer press does not dismiss Iced's native tooltip"
    );
    let click_messages = dispatch(
        &mut card,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        click_messages
            .iter()
            .any(|message| matches!(message, Message::SelectPerfDevice(PerfDevice::Cpu))),
        "the tooltip does not block activation of its underlying card"
    );
    assert!(
        tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "pointer click leaves Iced's native tooltip visible"
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
        tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "Escape does not dismiss Iced's native tooltip"
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
    assert!(
        !tooltip_is_open(&mut card, &mut tree, &node, &renderer, &viewport),
        "pointer exit dismisses Iced's native tooltip"
    );
}
