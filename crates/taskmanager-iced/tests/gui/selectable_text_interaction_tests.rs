// test-intent: behavior
//! Real Iced widget-event evidence for selectable text. The live paragraph
//! supplies pointer coordinates; assertions cover selection state and
//! clipboard contents, not pixel-level hit-test geometry.

use super::{SelectableText, State};
use crate::app::{IcedApp, Message};
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;

use iced::advanced::widget::Tree;
use iced::advanced::{Clipboard, Shell};
use iced::keyboard::key::{NativeCode, Physical};
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use taskmanager_assets::embedded_fonts;

type TestElement = Element<'static, Message, iced::Theme, iced::Renderer>;

#[derive(Default)]
struct RecordingClipboard {
    writes: Vec<(iced::advanced::clipboard::Kind, String)>,
}

impl Clipboard for RecordingClipboard {
    fn read(&self, kind: iced::advanced::clipboard::Kind) -> Option<String> {
        self.writes
            .iter()
            .rev()
            .find(|(written_kind, _)| *written_kind == kind)
            .map(|(_, contents)| contents.clone())
    }

    fn write(&mut self, kind: iced::advanced::clipboard::Kind, contents: String) {
        self.writes.push((kind, contents));
    }
}

struct Harness {
    app: IcedApp,
    id: iced::advanced::widget::Id,
    content: String,
    view: TestElement,
    tree: Tree,
    renderer: iced::Renderer,
    viewport: Rectangle,
    clipboard: RecordingClipboard,
}

impl Harness {
    fn new(content: &str) -> Self {
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
        let app = IcedApp::demo();
        let id = iced::advanced::widget::Id::new("selectable-text-interaction");
        let content = content.to_owned();
        let view = selectable_element(&app, id.clone(), &content);
        let tree = Tree::new(&view);

        Self {
            app,
            id,
            content,
            view,
            tree,
            renderer,
            viewport: Rectangle::new(Point::ORIGIN, Size::new(640.0, 80.0)),
            clipboard: RecordingClipboard::default(),
        }
    }

    fn layout(&mut self) -> Node {
        self.view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &Limits::new(Size::ZERO, self.viewport.size()),
        )
    }

    /// Resolve a text-engine caret position to a pointer event coordinate.
    /// Test strings use ASCII so byte and grapheme offsets coincide. This
    /// keeps the input tied to Iced's live paragraph without making a claim
    /// about any particular pixel metric.
    fn point_at_caret(&mut self, byte_offset: usize) -> Point {
        let node = self.layout();
        let bounds = Layout::new(&node).bounds();
        let state = self.tree.state.downcast_ref::<State>();
        let caret = state
            .measured
            .lines()
            .first()
            .and_then(|line| line.carets.get(byte_offset))
            .copied()
            .expect("the measured line has the requested caret");

        Point::new(bounds.x + caret, bounds.y + bounds.height * 0.5)
    }

    /// Resolve an absolute byte caret on its measured line (multi-line
    /// readouts), using the line's own grapheme-indexed caret positions.
    fn point_at_absolute_caret(&mut self, byte_offset: usize) -> Point {
        let node = self.layout();
        let bounds = Layout::new(&node).bounds();
        let state = self.tree.state.downcast_ref::<State>();
        let line = state
            .measured
            .lines()
            .iter()
            .find(|line| line.start_byte <= byte_offset && byte_offset <= line.end_byte)
            .expect("a measured line contains the requested byte");
        let index = line.grapheme_index_at(&state.content, byte_offset);
        let caret = line
            .carets
            .get(index)
            .copied()
            .expect("the measured line has the requested caret");
        Point::new(bounds.x + caret, bounds.y + line.top + line.height * 0.5)
    }

    /// Put the click within the requested ASCII grapheme using the live
    /// paragraph's adjacent caret positions.
    fn point_inside_byte(&mut self, byte_offset: usize) -> Point {
        let node = self.layout();
        let bounds = Layout::new(&node).bounds();
        let state = self.tree.state.downcast_ref::<State>();
        let carets = &state
            .measured
            .lines()
            .first()
            .expect("the measured run has a line")
            .carets;
        let start = carets
            .get(byte_offset)
            .copied()
            .expect("the measured line has the requested grapheme");
        let end = carets
            .get(byte_offset + 1)
            .copied()
            .expect("the measured line has the next caret");

        Point::new(
            bounds.x + start + (end - start) * 0.5,
            bounds.y + bounds.height * 0.5,
        )
    }

    fn dispatch(&mut self, event: Event, cursor: mouse::Cursor) {
        let node = self.layout();
        let mut messages = Vec::new();
        {
            let mut shell = Shell::new(&mut messages);
            self.view.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&node),
                cursor,
                &self.renderer,
                &mut self.clipboard,
                &mut shell,
                &self.viewport,
            );
        }

        for message in messages {
            let _ = self.app.update(message);
        }

        // Rebuild the production element with the app's owner projection and
        // reconcile it through Iced's real state tree, preserving the widget
        // state between event frames.
        let next = selectable_element(&self.app, self.id.clone(), &self.content);
        self.tree.diff(&next);
        self.view = next;
    }

    fn click(&mut self, point: Point) {
        self.dispatch(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            mouse::Cursor::Available(point),
        );
        self.dispatch(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Available(point),
        );
    }

    fn selection(&self) -> std::ops::Range<usize> {
        let state = self.tree.state.downcast_ref::<State>();
        state.edit.selection(&state.content).unwrap_or(0..0)
    }

    fn selected_text(&self) -> Option<String> {
        self.tree.state.downcast_ref::<State>().selected_text()
    }
}

fn selectable_element(app: &IcedApp, id: iced::advanced::widget::Id, content: &str) -> TestElement {
    let is_owner = app.text_selection_owner().as_ref() == Some(&id);
    SelectableText::new(id, content.to_owned(), 16.0, iced::Color::BLACK)
        .selection_owner(is_owner)
        .into()
}

fn ctrl_c_event() -> Event {
    command_event('c')
}

fn ctrl_a_event() -> Event {
    command_event('a')
}

fn command_event(character: char) -> Event {
    let text = character.to_string();
    let key = iced::keyboard::Key::Character(text.clone().into());
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::CTRL,
        text: Some(text.into()),
        repeat: false,
    })
}

#[test]
fn pointer_drag_selects_the_live_text_range_and_copies_that_fragment() {
    let content = "prefix chosen suffix";
    let mut harness = Harness::new(content);
    let start = content.find("chosen").expect("selected word");
    let range = start..start + "chosen".len();
    let start_point = harness.point_at_caret(range.start);
    let end_point = harness.point_at_caret(range.end);

    harness.dispatch(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(start_point),
    );
    assert_eq!(
        harness.app.text_selection_owner(),
        Some(harness.id.clone()),
        "the real pointer press publishes selection ownership into IcedApp"
    );
    harness.dispatch(
        Event::Mouse(mouse::Event::CursorMoved {
            position: end_point,
        }),
        mouse::Cursor::Available(end_point),
    );
    assert_eq!(
        harness.selection(),
        range,
        "the drag exposes exactly the selected byte range"
    );
    assert_eq!(
        harness.selected_text().as_deref(),
        Some("chosen"),
        "the visible selection model contains only the dragged fragment"
    );
    harness.dispatch(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(end_point),
    );

    assert_eq!(
        harness.clipboard.writes,
        vec![(
            iced::advanced::clipboard::Kind::Primary,
            "chosen".to_owned()
        )],
        "finishing a pointer selection writes its exact fragment to PRIMARY"
    );

    harness.dispatch(ctrl_c_event(), mouse::Cursor::Unavailable);
    assert_eq!(
        harness.clipboard.writes,
        vec![
            (
                iced::advanced::clipboard::Kind::Primary,
                "chosen".to_owned()
            ),
            (
                iced::advanced::clipboard::Kind::Standard,
                "chosen".to_owned()
            ),
        ],
        "Ctrl-C writes the same selected fragment to the standard clipboard"
    );
}

#[test]
fn double_click_selects_the_whole_address_through_iced_pointer_events() {
    let content = "gateway 192.168.1.1 up";
    let mut harness = Harness::new(content);
    let start = content.find("192.168.1.1").expect("address");
    let click = harness.point_inside_byte(start + 5);

    harness.click(click);
    harness.click(click);

    let range = start..start + "192.168.1.1".len();
    assert_eq!(harness.selection(), range);
    assert_eq!(
        harness.selected_text().as_deref(),
        Some("192.168.1.1"),
        "two real presses select the full dotted identifier"
    );
    assert_eq!(
        harness.app.text_selection_owner(),
        Some(harness.id),
        "the IcedApp owns the widget that received the double-click"
    );
    assert!(harness.clipboard.writes.is_empty());
}

#[test]
fn triple_click_selects_the_entire_value_through_iced_pointer_events() {
    let content = "read only value";
    let mut harness = Harness::new(content);
    let click = harness.point_inside_byte(content.find("only").expect("word") + 1);

    harness.click(click);
    harness.click(click);
    harness.click(click);

    assert_eq!(harness.selection(), 0..content.len());
    assert_eq!(
        harness.selected_text().as_deref(),
        Some(content),
        "the third real press selects the complete value including spaces"
    );
    assert!(harness.clipboard.writes.is_empty());
}

#[test]
fn ctrl_a_selects_the_whole_multiline_readout_only_after_it_owns_the_selection() {
    let content = "alpha\nbeta gamma\ndelta";
    let mut harness = Harness::new(content);

    // Before this widget owns the window's selection, Ctrl-A keeps the shared
    // row-summary meaning: the readout consumes nothing.
    harness.dispatch(ctrl_a_event(), mouse::Cursor::Unavailable);
    assert_eq!(
        harness.selection(),
        0..0,
        "an unowned readout ignores Ctrl-A"
    );

    let point = harness.point_at_absolute_caret(2);
    harness.click(point);
    assert_eq!(harness.app.text_selection_owner(), Some(harness.id.clone()));

    harness.dispatch(ctrl_a_event(), mouse::Cursor::Unavailable);
    assert_eq!(
        harness.selection(),
        0..content.len(),
        "Ctrl-A selects the complete multi-line readout"
    );
    assert_eq!(harness.selected_text().as_deref(), Some(content));
    assert!(
        harness.clipboard.writes.is_empty(),
        "select-all does not copy by itself"
    );
}

#[test]
fn pointer_drag_selects_across_a_line_break() {
    let content = "alpha\nbeta gamma\ndelta";
    let mut harness = Harness::new(content);
    let start = 2; // inside "alpha"
    let end = content.find(" gamma").expect("line-two word end");
    let start_point = harness.point_at_absolute_caret(start);
    let end_point = harness.point_at_absolute_caret(end);

    harness.dispatch(
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(start_point),
    );
    harness.dispatch(
        Event::Mouse(mouse::Event::CursorMoved {
            position: end_point,
        }),
        mouse::Cursor::Available(end_point),
    );
    assert_eq!(
        harness.selection(),
        start..end,
        "the drag spans the line break in one byte range"
    );
    assert_eq!(harness.selected_text().as_deref(), Some("pha\nbeta"));
    harness.dispatch(
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(end_point),
    );
    assert_eq!(
        harness.clipboard.writes,
        vec![(
            iced::advanced::clipboard::Kind::Primary,
            "pha\nbeta".to_owned()
        )],
        "the multi-line fragment reaches the primary clipboard intact"
    );
}
