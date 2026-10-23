// test-intent: behavior
//! Real Iced widget-event evidence for the compact ribbon's wheel guard
//! (ICED-051): a wheel gesture over the ribbon must never reach the list
//! underneath it.
//!
//! The harness builds the real tree — a vertical scroller (the process
//! table) whose content starts with a horizontal scroller (the ribbon) —
//! and dispatches a real `WheelScrolled` event through Iced's own event
//! pipeline. The assertions are about the OUTER list's scroll behavior,
//! never about source text.

use super::*;
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::Tree;
use iced::widget::{column, row, text};
use iced::{Event, Point, Rectangle, Size, mouse};
use taskmanager_assets::embedded_fonts;

type TestElement = Element<'static, Message, iced::Theme, iced::Renderer>;

/// The viewport the harness lays out and dispatches into.
const VIEWPORT: Size = Size::new(720.0, 480.0);

struct Harness {
    view: TestElement,
    tree: Tree,
    renderer: iced::Renderer,
    viewport: Rectangle,
    /// The messages the tree published while the gesture was dispatched.
    published: Vec<Message>,
}

impl Harness {
    /// Build the tree. `guarded` decides whether the ribbon sits behind the
    /// wheel guard.
    fn new(guarded: bool) -> Self {
        {
            let mut fonts = iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system");
            for font in embedded_fonts() {
                fonts.load_font(font);
            }
        }
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            crate::theme_binding::BUNDLED_UI_FONT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("software renderer");

        // The ribbon: a horizontal scroller whose content is wider than the
        // viewport, so a horizontal gesture really has somewhere to go.
        let ribbon_content: TestElement = row![
            text("preset one"),
            text("preset two"),
            text("preset three"),
            text("preset four"),
            text("preset five"),
            text("preset six"),
            text("preset seven"),
            text("preset eight"),
        ]
        .spacing(6)
        .into();
        let ribbon: TestElement = iced::widget::scrollable(ribbon_content)
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .height(Length::Fixed(32.0))
            .width(Length::Fill)
            .into();
        let ribbon = if guarded { wheel_guard(ribbon) } else { ribbon };

        // The list underneath: a vertical scroller whose content is taller
        // than the viewport, so a wheel gesture really can scroll it.
        // The filler is taller than the viewport, so the list underneath can
        // really scroll.
        let filler: TestElement = column((0..40).map(|index| text(format!("row {index}")).into()))
            .spacing(4)
            .into();
        let view: TestElement = iced::widget::scrollable(column![ribbon, filler].spacing(8))
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .height(Length::Fill)
            .width(Length::Fill)
            .on_scroll(|_| Message::Tick)
            .into();

        let tree = Tree::new(&view);
        Self {
            view,
            tree,
            renderer,
            viewport: Rectangle::new(Point::ORIGIN, VIEWPORT),
            published: Vec::new(),
        }
    }

    fn layout(&mut self) -> Node {
        self.view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &Limits::new(Size::ZERO, self.viewport.size()),
        )
    }

    /// Dispatch a vertical wheel gesture over the ribbon's own band.
    fn wheel_over_ribbon(&mut self) {
        let node = self.layout();
        let bounds = Layout::new(&node).bounds();
        // The ribbon is the first row of the outer scroller's content.
        let point = Point::new(bounds.x + bounds.width * 0.5, bounds.y + 8.0);
        let mut messages = Vec::new();
        {
            let mut shell = Shell::new(&mut messages);
            let mut clipboard = iced::advanced::clipboard::Null;
            self.view.as_widget_mut().update(
                &mut self.tree,
                &Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0.0, y: 3.0 },
                }),
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &self.renderer,
                &mut clipboard,
                &mut shell,
                &self.viewport,
            );
        }
        self.published = messages;
    }

    /// How many times the list underneath reported a scroll.
    fn list_scrolls(&self) -> usize {
        self.published
            .iter()
            .filter(|message| matches!(message, Message::Tick))
            .count()
    }
}

/// Without the guard, a wheel gesture over the ribbon scrolls the list
/// underneath: this is the defect the guard exists for, and it proves the
/// harness really drives the scroll behavior.
#[test]
fn an_unguarded_ribbon_lets_the_wheel_reach_the_list_underneath() {
    let mut harness = Harness::new(false);
    harness.wheel_over_ribbon();
    assert!(
        harness.list_scrolls() > 0,
        "without the guard the gesture scrolls the list underneath"
    );
}

/// With the guard, the same gesture never reaches the list underneath: the
/// table's view stays still while the pointer is over the ribbon.
#[test]
fn the_guarded_ribbon_keeps_the_wheel_off_the_list_underneath() {
    let mut harness = Harness::new(true);
    harness.wheel_over_ribbon();
    assert_eq!(
        harness.list_scrolls(),
        0,
        "the guarded ribbon must not let the gesture scroll the list underneath"
    );
}
