// test-intent: behavior
//
// Real widget-tree evidence for the one native-button adapter: the caller's
// style resolver, selected state, pointer activation, keyboard focus and
// disabled path are all observable without source-text assertions.

use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::widget::{button, container, text};
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use std::rc::Rc;
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};

use crate::app::FocusTarget;
use crate::focus::focus_id;
use crate::theme;
use crate::ui::components::{ContentButtonOptions, content_button};

type TestElement<'a> = Element<'a, crate::Message, iced::Theme, iced::Renderer>;

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

fn theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
    .with_focus_visible(true)
}

fn styled_button<'a>(
    theme_snapshot: Theme,
    selected: bool,
    target: Option<FocusTarget>,
    on_press: Option<crate::Message>,
    enabled: bool,
) -> TestElement<'a> {
    let style_theme = theme_snapshot;
    content_button(ContentButtonOptions {
        content: container(text("content")).into(),
        style: Rc::new(move |_, status| theme::header_button_style(&style_theme, status, selected)),
        padding: iced::Padding::ZERO,
        width: Some(iced::Length::Fill),
        height: None,
        on_press,
        target,
        focus_color: theme::focus_ring_color(&theme_snapshot, false),
        focus_radius: f32::from(theme_snapshot.palette().control_radius),
        activate_on_pointer: false,
        enabled,
        clip: false,
    })
}

fn layout(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    size: Size,
) -> Node {
    element
        .as_widget_mut()
        .layout(tree, renderer, &Limits::new(Size::ZERO, size))
}

struct FocusById {
    target: Id,
    matched: bool,
}

impl iced::advanced::widget::Operation<()> for FocusById {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<()>)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        id: Option<&Id>,
        _bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        if id == Some(&self.target) {
            state.focus();
            self.matched = true;
        }
    }
}

fn focus(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: Id,
) -> bool {
    let mut operation = FocusById {
        target,
        matched: false,
    };
    element
        .as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.matched
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

fn dispatch(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    event: Event,
    cursor: mouse::Cursor,
) -> Vec<crate::Message> {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        element.as_widget_mut().update(
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

fn active_style(theme_snapshot: &Theme, selected: bool) -> button::Style {
    theme::header_button_style(theme_snapshot, button::Status::Active, selected)
}

#[test]
fn content_button_keeps_the_callers_typed_style_and_selected_state() {
    let theme_snapshot = theme();
    let selected_style = active_style(&theme_snapshot, true);
    let idle_style = active_style(&theme_snapshot, false);
    assert_ne!(
        selected_style.text_color, idle_style.text_color,
        "the selected semantic changes the typed ink"
    );

    let headless_renderer = renderer();
    let size = Size::new(320.0, 80.0);
    let observed_styles = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed_styles_for_style = std::rc::Rc::clone(&observed_styles);
    let mut selected: TestElement<'_> = content_button(ContentButtonOptions {
        content: container(text("content")).into(),
        style: Rc::new(move |_, status| {
            let style = theme::header_button_style(&theme_snapshot, status, true);
            observed_styles_for_style
                .borrow_mut()
                .push(style.text_color);
            style
        }),
        padding: iced::Padding::ZERO,
        width: Some(iced::Length::Fill),
        height: None,
        on_press: Some(crate::Message::Tick),
        target: Some(FocusTarget::EndTask),
        focus_color: theme::focus_ring_color(&theme_snapshot, false),
        focus_radius: f32::from(theme_snapshot.palette().control_radius),
        activate_on_pointer: false,
        enabled: true,
        clip: false,
    });
    let mut tree = Tree::new(&selected);
    let node = layout(&mut selected, &mut tree, &headless_renderer, size);
    assert_eq!(node.bounds().width, size.width);
    assert!(node.bounds().height > 0.0);
    let _ = dispatch(
        &mut selected,
        &mut tree,
        &node,
        &headless_renderer,
        &Rectangle::with_size(size),
        Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        )),
        mouse::Cursor::Unavailable,
    );
    let mut draw_renderer = renderer();
    selected.as_widget().draw(
        &tree,
        &mut draw_renderer,
        &iced::Theme::Dark,
        &iced::advanced::renderer::Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &Rectangle::with_size(size),
    );
    assert_eq!(
        observed_styles.borrow().first().copied(),
        Some(selected_style.text_color)
    );
    assert_ne!(selected_style.text_color, idle_style.text_color);
}

#[test]
fn content_button_preserves_pointer_keyboard_and_disabled_activation_paths() {
    let theme_snapshot = theme();
    let renderer = renderer();
    let size = Size::new(320.0, 80.0);
    let viewport = Rectangle::with_size(size);
    let target = FocusTarget::EndTask;
    let activation = crate::Message::CloseSearch;

    let mut enabled: TestElement<'_> = styled_button(
        theme_snapshot,
        false,
        Some(target),
        Some(activation.clone()),
        true,
    );
    let mut enabled_tree = Tree::new(&enabled);
    let enabled_node = layout(&mut enabled, &mut enabled_tree, &renderer, size);
    let center = Point::new(
        enabled_node.bounds().center_x(),
        enabled_node.bounds().center_y(),
    );
    let press = dispatch(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(center),
    );
    let release = dispatch(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(center),
    );
    assert!(
        press
            .iter()
            .chain(&release)
            .any(|message| matches!(message, crate::Message::CloseSearch))
    );
    assert!(
        press
            .iter()
            .any(|message| matches!(message, crate::Message::Focus(FocusTarget::EndTask)))
    );

    assert!(focus(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        Id::from(focus_id(target)),
    ));
    let keyboard = dispatch(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        &viewport,
        key_pressed(Named::Space, Code::Space),
        mouse::Cursor::Unavailable,
    );
    assert!(
        keyboard
            .iter()
            .any(|message| matches!(message, crate::Message::CloseSearch))
    );

    let mut disabled: TestElement<'_> = styled_button(
        theme_snapshot,
        false,
        Some(target),
        Some(crate::Message::CloseSearch),
        false,
    );
    let mut disabled_tree = Tree::new(&disabled);
    let disabled_node = layout(&mut disabled, &mut disabled_tree, &renderer, size);
    assert!(!focus(
        &mut disabled,
        &mut disabled_tree,
        &disabled_node,
        &renderer,
        Id::from(focus_id(target)),
    ));
    let silent = dispatch(
        &mut disabled,
        &mut disabled_tree,
        &disabled_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(center),
    );
    assert!(
        !silent
            .iter()
            .any(|message| matches!(message, crate::Message::CloseSearch))
    );
}

#[test]
fn content_button_without_a_focus_target_keeps_native_pointer_activation() {
    let theme_snapshot = theme();
    let renderer = renderer();
    let size = Size::new(320.0, 80.0);
    let viewport = Rectangle::with_size(size);
    let mut element: TestElement<'_> = styled_button(
        theme_snapshot,
        false,
        None,
        Some(crate::Message::CloseSearch),
        true,
    );
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    let center = Point::new(node.bounds().center_x(), node.bounds().center_y());
    let _ = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(center),
    );
    let release = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(center),
    );
    assert!(
        release
            .iter()
            .any(|message| matches!(message, crate::Message::CloseSearch))
    );
}
