// test-intent: behavior
//! The renderer-local input-modality tracker: keyboard input is the only
//! focus-visible source (the strict GPUI-parity policy), pointer presses
//! clear it, programmatic focus inherits the previous origin, and the shared
//! ring seam (`theme::focus_ring_color`) consumes the decision. Lifecycle
//! cases drive the production Observer and IcedApp reducers with real Iced
//! events over a software-rendered widget tree; no native window is required.

use crate::app::{FocusTarget, IcedApp, Message};
use crate::input_modality::InputModality;
use crate::keys::IcedKey;
use crate::theme::focus_ring_color;
use crate::theme_binding::color;
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::{Element, Event, Size, mouse};
use taskmanager_application::AppPage;
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::Theme;

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
    view: &mut TestElement<'_>,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    size: Size,
) -> Node {
    view.as_widget_mut()
        .layout(tree, renderer, &Limits::new(Size::ZERO, size))
}

struct TargetPresence {
    target: Id,
    found: bool,
    focus_target: bool,
    focused: bool,
}

impl Operation<()> for TargetPresence {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        if !self.found {
            visit(self);
        }
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: iced::Rectangle, state: &mut dyn Focusable) {
        if id.is_some_and(|id| id == &self.target) {
            self.found = true;
            if self.focus_target {
                state.focus();
                self.focused = state.is_focused();
            }
        }
    }
}

fn target_is_present(
    view: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: FocusTarget,
) -> bool {
    let mut operation = TargetPresence {
        target: Id::from(crate::focus::focus_id(target)),
        found: false,
        focus_target: false,
        focused: false,
    };
    view.as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.found
}

fn focus_target(
    view: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: FocusTarget,
) -> bool {
    let mut operation = TargetPresence {
        target: Id::from(crate::focus::focus_id(target)),
        found: false,
        focus_target: true,
        focused: false,
    };
    view.as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.found && operation.focused
}

fn dispatch_window_event(
    view: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    event: Event,
) -> Vec<Message> {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard,
            &mut shell,
            &iced::Rectangle::with_size(node.size()),
        );
    }
    messages
}

#[test]
fn pointer_input_clears_the_keyboard_ring_source() {
    assert!(InputModality::Keyboard.shows_focus_ring());
    assert!(!InputModality::Pointer.shows_focus_ring());
    // Programmatic focus inherits the previous origin: it never rings by
    // itself while the window's modality is programmatic.
    assert!(!InputModality::Programmatic.shows_focus_ring());
}

#[test]
fn ring_color_encodes_the_visibility_decision() {
    let theme = Theme::dark();
    let ring = color(theme.palette().ring);

    let keyboard_theme = theme.with_focus_visible(true);
    let stroke = focus_ring_color(&keyboard_theme, false);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (ring.r, ring.g, ring.b),
        "the ring hue comes from palette().ring"
    );
    assert_eq!(stroke.a, 1.0, "keyboard focus ⇒ ring drawn opaquely");

    let pointer_theme = theme.with_focus_visible(false);
    let pointer_stroke = focus_ring_color(&pointer_theme, false);
    assert_eq!(
        (pointer_stroke.r, pointer_stroke.g, pointer_stroke.b),
        (ring.r, ring.g, ring.b)
    );
    assert_eq!(pointer_stroke.a, 0.0, "pointer focus ⇒ no ring");
}

#[test]
fn destructive_rings_follow_the_same_visibility_rule() {
    let theme = Theme::dark();
    let danger = color(theme.palette().danger);

    let keyboard_theme = theme.with_focus_visible(true);
    let stroke = focus_ring_color(&keyboard_theme, true);
    assert_eq!(
        (stroke.r, stroke.g, stroke.b),
        (danger.r, danger.g, danger.b),
        "destructive controls keep the danger ring hue"
    );
    assert_eq!(stroke.a, 1.0);

    let pointer_theme = theme.with_focus_visible(false);
    assert_eq!(focus_ring_color(&pointer_theme, true).a, 0.0);
}

#[test]
fn root_messages_feed_the_modality_tracker() {
    let mut app = crate::IcedApp::demo();
    assert!(!app.theme().focus_visible());

    // Any key press counts as keyboard input, even an unmappable one (the
    // `Other` bucket covers bare modifiers too).
    let _ = app.update(Message::Key(IcedKey::Other));
    assert!(
        app.theme().focus_visible(),
        "a key press marks keyboard input"
    );

    let _ = app.update(Message::PointerPressed);
    assert!(
        !app.theme().focus_visible(),
        "a pointer press over the root clears it"
    );
}

#[test]
fn modality_is_isolated_between_app_instances() {
    let mut keyboard = crate::IcedApp::demo();
    let mut pointer = crate::IcedApp::demo();

    let _ = keyboard.update(Message::Key(IcedKey::Other));
    let _ = pointer.update(Message::PointerPressed);

    assert!(keyboard.theme().focus_visible());
    assert!(!pointer.theme().focus_visible());
}

#[test]
fn window_deactivation_from_the_subscription_clears_the_keyboard_ring() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let target = FocusTarget::AboutTrigger;
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::Key(IcedKey::Other));
    let _ = app.update(Message::Focus(target));
    assert!(app.theme().focus_visible());
    assert_eq!(app.input.focused_control, Some(target));

    let renderer = renderer();
    {
        let mut view = crate::ui::view(&app);
        let mut tree = Tree::new(&view);
        let node = layout(&mut view, &mut tree, &renderer, size);
        assert!(
            focus_target(&mut view, &mut tree, &node, &renderer, target),
            "the tracked toolbar target is mounted and focusable before window blur"
        );
    }

    let message =
        crate::app::subscription::message_for_window_event(iced::window::Event::Unfocused)
            .expect("the window subscription publishes deactivation");
    let _ = app.update(message);

    assert_eq!(app.input.modality, InputModality::Programmatic);
    assert!(!app.theme().focus_visible());
    assert_eq!(focus_ring_color(app.theme(), false).a, 0.0);
    assert_eq!(
        app.input.focused_control,
        Some(target),
        "window blur clears the keyboard ring without discarding the still-mounted target"
    );
}

#[test]
fn an_unmounted_focused_target_clears_the_ring_on_the_next_real_event() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let target = FocusTarget::TableRow {
        page: AppPage::Applications,
        index: 0,
    };
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let _ = app.update(Message::Key(IcedKey::Other));
    let _ = app.update(Message::Focus(target));
    assert!(app.theme().focus_visible());
    assert_eq!(app.input.focused_control, Some(target));

    let renderer = renderer();
    let mut tree = {
        let mut view = crate::ui::view(&app);
        let mut tree = Tree::new(&view);
        let node = layout(&mut view, &mut tree, &renderer, size);
        assert!(
            focus_target(&mut view, &mut tree, &node, &renderer, target),
            "the table row is mounted and can receive real Iced focus before filtering"
        );
        tree
    };

    let stale_target = FocusTarget::PageTab(AppPage::Performance);
    let _ = app.update(Message::FocusedTargetUnmounted(stale_target));
    assert_eq!(app.input.focused_control, Some(target));
    assert!(
        app.theme().focus_visible(),
        "a stale unmount signal is ignored"
    );
    assert_eq!(focus_ring_color(app.theme(), false).a, 1.0);

    let _ = app.update(Message::SearchChanged(
        "__taskforest_no_such_process__".to_owned(),
    ));
    assert_eq!(app.input.focused_control, Some(target));
    assert!(app.theme().focus_visible());

    let messages = {
        let mut view = crate::ui::view(&app);
        tree.diff(&view);
        let node = layout(&mut view, &mut tree, &renderer, size);
        assert!(
            !target_is_present(&mut view, &mut tree, &node, &renderer, target),
            "filtering removes the focused row from the reconciled widget tree"
        );
        dispatch_window_event(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            Event::Window(iced::window::Event::Focused),
        )
    };
    assert!(
        messages.iter().any(
            |message| matches!(message, Message::FocusedTargetUnmounted(candidate) if *candidate == target)
        ),
        "the production observer reports the missing tracked target before forwarding the event"
    );
    for message in messages {
        let _ = app.update(message);
    }

    assert_eq!(app.input.focused_control, None);
    assert_eq!(app.input.modality, InputModality::Programmatic);
    assert!(!app.theme().focus_visible());
    assert_eq!(focus_ring_color(app.theme(), false).a, 0.0);
}
