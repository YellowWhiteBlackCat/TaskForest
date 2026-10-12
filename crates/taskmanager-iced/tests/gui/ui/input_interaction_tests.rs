// test-intent: behavior
//! Production-path interaction evidence for Iced's text and choice inputs.

use super::*;
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation, TextInput};
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, NativeCode, Physical};
use iced::{Element, Event, Point, Rectangle, Size, Vector, mouse};
use taskmanager_application::AppPage;
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

fn viewport() -> Rectangle {
    Rectangle::new(Point::new(0.0, 0.0), Size::new(1280.0, 1000.0))
}

fn layout(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    renderer: &iced::Renderer,
    viewport: Rectangle,
) -> Node {
    element
        .as_widget_mut()
        .layout(tree, renderer, &Limits::new(Size::ZERO, viewport.size()))
}

fn first_overlay_panel_bounds(node: &Node, viewport: Size) -> Option<Rectangle> {
    let bounds = node.bounds();
    if bounds.width < viewport.width || bounds.height < viewport.height {
        return Some(bounds);
    }
    node.children()
        .iter()
        .find_map(|child| first_overlay_panel_bounds(child, viewport))
}

fn dispatch(
    element: &mut TestElement<'_>,
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

#[derive(Default)]
struct InputBounds(Vec<Rectangle>);

impl Operation<()> for InputBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn text_input(&mut self, _id: Option<&Id>, bounds: Rectangle, _state: &mut dyn TextInput) {
        self.0.push(bounds);
    }
}

fn input_bounds(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
) -> Rectangle {
    let mut operation = InputBounds::default();
    element
        .as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    assert_eq!(
        operation.0.len(),
        1,
        "the production surface has one text input"
    );
    operation.0[0]
}

struct FocusableBounds {
    target: Id,
    bounds: Option<Rectangle>,
}

impl Operation<()> for FocusableBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            self.bounds = Some(bounds);
        }
    }
}

struct FocusById {
    target: Id,
    matched: bool,
}

impl Operation<()> for FocusById {
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

fn focus_bounds(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: Id,
) -> Rectangle {
    let mut operation = FocusableBounds {
        target,
        bounds: None,
    };
    element
        .as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation
        .bounds
        .expect("the production control is focusable")
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

fn target(section: &'static str) -> Id {
    Id::from(crate::focus::focus_id(FocusTarget::SettingsChoice {
        section,
        index: 0,
    }))
}

fn center(bounds: Rectangle) -> Point {
    Point::new(bounds.center_x(), bounds.center_y())
}

fn click_messages(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    point: Point,
) -> Vec<Message> {
    let mut messages = dispatch(
        element,
        tree,
        node,
        renderer,
        viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    messages.extend(dispatch(
        element,
        tree,
        node,
        renderer,
        viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    ));
    messages
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

fn character_pressed(character: char) -> Event {
    let text = character.to_string();
    let key = Key::Character(text.clone().into());
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: Some(text.into()),
        repeat: false,
    })
}

fn apply(app: &mut crate::IcedApp, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

fn run_task_view(app: &crate::IcedApp) -> TestElement<'_> {
    crate::ui::overlays::run_task::run_task_overlay(app.theme(), &app.run_task, 1.0)
}

fn type_run_task_x(app: &mut crate::IcedApp, renderer: &iced::Renderer, viewport: Rectangle) {
    let mut element = run_task_view(app);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, renderer, viewport);
    let bounds = input_bounds(&mut element, &mut tree, &node, renderer);
    let point = Point::new(bounds.x + bounds.width - 4.0, bounds.center_y());
    let _ = click_messages(&mut element, &mut tree, &node, renderer, &viewport, point);
    let edited = dispatch(
        &mut element,
        &mut tree,
        &node,
        renderer,
        &viewport,
        character_pressed('x'),
        mouse::Cursor::Available(point),
    );
    assert!(
        edited.iter().any(
            |message| matches!(message, Message::UpdateRunTaskCommand(command) if command == "x")
        ),
        "typing in the mounted Run Task TextInput publishes its edit"
    );
    drop(element);
    apply(app, edited);
    assert_eq!(
        app.run_task.command, "x",
        "the consumer stores the edited buffer"
    );
}

/// Run Task's production TextInput edits the command, Enter submits it, and
/// Escape dismisses the local surface without sending a launch request.
#[test]
fn run_task_text_input_edits_submits_and_escape_cancels() {
    let renderer = renderer();
    let viewport = viewport();
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::OpenRunTask);
    type_run_task_x(&mut app, &renderer, viewport);

    let mut element = run_task_view(&app);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let bounds = input_bounds(&mut element, &mut tree, &node, &renderer);
    let point = Point::new(bounds.x + bounds.width - 4.0, bounds.center_y());
    let _ = click_messages(&mut element, &mut tree, &node, &renderer, &viewport, point);
    let submitted = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Available(point),
    );
    assert!(
        submitted
            .iter()
            .any(|message| matches!(message, Message::SubmitRunTask)),
        "Enter in the production TextInput reaches its submit message"
    );
    drop(element);
    apply(&mut app, submitted);
    assert!(
        !app.run_task_open(),
        "committing closes the Run Task surface"
    );
    assert!(
        app.run_task.command.is_empty(),
        "committing clears the draft"
    );

    let mut canceled = crate::IcedApp::demo();
    let _ = canceled.update(Message::OpenRunTask);
    type_run_task_x(&mut canceled, &renderer, viewport);
    let feedback_before_cancel = canceled.shell.feedback_notice().cloned();
    let mut element = run_task_view(&canceled);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let bounds = input_bounds(&mut element, &mut tree, &node, &renderer);
    let point = Point::new(bounds.x + bounds.width - 4.0, bounds.center_y());
    let _ = click_messages(&mut element, &mut tree, &node, &renderer, &viewport, point);
    let _ = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Escape, Code::Escape),
        mouse::Cursor::Available(point),
    );
    drop(element);
    let _ = canceled.update(Message::Key(crate::keys::map_key(
        &Key::Named(Named::Escape),
        iced::keyboard::Modifiers::default(),
    )));
    assert!(!canceled.run_task_open(), "Escape cancels the open surface");
    assert!(
        canceled.shell.feedback_notice() == feedback_before_cancel.as_ref(),
        "canceling does not submit a platform launch request"
    );
    assert!(
        canceled.shell.pending_session().is_none(),
        "canceling does not create a launch session"
    );
    assert_eq!(
        canceled.run_task.command, "x",
        "canceling preserves the unsubmitted command draft"
    );
}

/// The Services SearchInput's native character events filter the actual
/// memoized service projection; Escape clears the query and returns focus to
/// the restored list.
#[test]
fn services_search_input_filters_and_escape_clears_restores_rows() {
    let renderer = renderer();
    let viewport = viewport();
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Services));
    let (all_rows, all_indices, _) = app.services_projection("");
    assert!(
        !all_indices.is_empty(),
        "the demo consumer has services to filter"
    );
    let all_count = all_indices.len();

    for character in "timesync".chars() {
        let mut element = crate::ui::components::search_input(
            app.theme(),
            FocusTarget::ServicesSearch,
            "Search services",
            app.services_query(),
            Message::ServicesSearchChanged,
        );
        let mut tree = Tree::new(&element);
        let node = layout(&mut element, &mut tree, &renderer, viewport);
        let bounds = input_bounds(&mut element, &mut tree, &node, &renderer);
        let point = Point::new(bounds.x + bounds.width - 4.0, bounds.center_y());
        let _ = click_messages(&mut element, &mut tree, &node, &renderer, &viewport, point);
        let messages = dispatch(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            character_pressed(character),
            mouse::Cursor::Available(point),
        );
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::ServicesSearchChanged(_))),
            "each native character edit reaches the Services filter reducer"
        );
        drop(element);
        apply(&mut app, messages);
    }

    assert_eq!(app.services_query(), "timesync");
    let (filtered_rows, filtered_indices, _) = app.services_projection(app.services_query());
    assert_eq!(
        filtered_indices.len(),
        1,
        "the query narrows the service list"
    );
    assert_eq!(
        filtered_rows[filtered_indices[0]].name, "systemd-timesyncd.service",
        "the retained projection row is the real query match"
    );

    let mut element = crate::ui::components::search_input(
        app.theme(),
        FocusTarget::ServicesSearch,
        "Search services",
        app.services_query(),
        Message::ServicesSearchChanged,
    );
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let bounds = input_bounds(&mut element, &mut tree, &node, &renderer);
    let point = Point::new(bounds.x + bounds.width - 4.0, bounds.center_y());
    let _ = click_messages(&mut element, &mut tree, &node, &renderer, &viewport, point);
    let _ = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Escape, Code::Escape),
        mouse::Cursor::Available(point),
    );
    drop(element);
    let _ = app.update(Message::Key(crate::keys::map_key(
        &Key::Named(Named::Escape),
        iced::keyboard::Modifiers::default(),
    )));

    assert!(
        app.services_query().is_empty(),
        "Escape clears the active query"
    );
    let (_, restored_indices, _) = app.services_projection(app.services_query());
    assert_eq!(
        restored_indices.len(),
        all_count,
        "the full list is restored"
    );
    assert!(
        app.input.focused_control
            == Some(FocusTarget::TableRow {
                page: AppPage::Services,
                index: 0,
            }),
        "Escape returns focus to the first row in the restored list"
    );
    assert_eq!(all_rows.len(), all_count);
}

/// The real System settings CPU switch updates its preference through both
/// pointer and Space paths. A disabled instance emits no action.
#[test]
fn settings_switch_routes_pointer_and_space_and_disabled_is_inert() {
    let renderer = renderer();
    let viewport = viewport();
    let mut app = crate::IcedApp::demo();
    let mut element = system_group(app.theme(), app.language(), app.preferences());
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let bounds = focus_bounds(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        target(DeviceKind::Cpu.key()),
    );
    let pointer_messages = click_messages(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        center(bounds),
    );
    assert!(
        pointer_messages.iter().any(|message| matches!(
            message,
            Message::SettingsChanged(SettingsChange::ShowDevice(DeviceKind::Cpu, false))
        )),
        "pointer activation emits the CPU visibility preference change"
    );
    drop(element);
    apply(&mut app, pointer_messages);
    assert!(
        !app.preferences().device_visible(DeviceKind::Cpu),
        "the IcedApp consumer hides CPU after the pointer action"
    );

    let mut element = system_group(app.theme(), app.language(), app.preferences());
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let id = target(DeviceKind::Cpu.key());
    assert!(focus(&mut element, &mut tree, &node, &renderer, id));
    let keyboard_messages = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Space, Code::Space),
        mouse::Cursor::Unavailable,
    );
    assert!(
        keyboard_messages.iter().any(|message| matches!(
            message,
            Message::SettingsChanged(SettingsChange::ShowDevice(DeviceKind::Cpu, true))
        )),
        "Space activation emits the inverse CPU visibility preference"
    );
    drop(element);
    apply(&mut app, keyboard_messages);
    assert!(
        app.preferences().device_visible(DeviceKind::Cpu),
        "the IcedApp consumer restores CPU after Space activation"
    );

    let mut disabled: TestElement<'_> =
        crate::ui::components::switch(app.theme(), FocusTarget::StartupControl, true, true, |_| {
            Message::Tick
        });
    let mut tree = Tree::new(&disabled);
    let node = layout(&mut disabled, &mut tree, &renderer, viewport);
    assert!(
        !focus(
            &mut disabled,
            &mut tree,
            &node,
            &renderer,
            Id::from(crate::focus::focus_id(FocusTarget::StartupControl)),
        ),
        "the disabled switch is not registered as a focus stop"
    );
    let messages = click_messages(
        &mut disabled,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        center(node.bounds()),
    );
    assert!(
        messages.is_empty(),
        "a disabled switch ignores pointer activation"
    );
}

fn drag_refresh_to(
    app: &mut crate::IcedApp,
    renderer: &iced::Renderer,
    viewport: Rectangle,
    fraction: f32,
) -> Vec<Message> {
    let mut element = system_group(app.theme(), app.language(), app.preferences());
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, renderer, viewport);
    let bounds = focus_bounds(&mut element, &mut tree, &node, renderer, target("refresh"));
    let start = Point::new(bounds.x + bounds.width * 0.5, bounds.center_y());
    let end = Point::new(
        bounds.x + bounds.width * fraction.clamp(0.001, 0.999),
        bounds.center_y(),
    );
    let mut messages = dispatch(
        &mut element,
        &mut tree,
        &node,
        renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(start),
    );
    messages.extend(dispatch(
        &mut element,
        &mut tree,
        &node,
        renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: end }),
        mouse::Cursor::Available(end),
    ));
    messages.extend(dispatch(
        &mut element,
        &mut tree,
        &node,
        renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(end),
    ));
    drop(element);
    messages
}

fn last_refresh_value(messages: &[Message]) -> u64 {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::SettingsChanged(SettingsChange::RefreshInterval(value)) => Some(*value),
            _ => None,
        })
        .last()
        .expect("dragging the native slider emits a refresh interval")
}

/// Dragging the Settings refresh slider reaches both endpoints, snaps its
/// interior value to 100 ms steps, and writes each selected value back into
/// the IcedApp preference projection.
#[test]
fn settings_refresh_slider_drag_snaps_clamps_and_writes_back() {
    let renderer = renderer();
    let viewport = viewport();
    let mut app = crate::IcedApp::demo();

    let messages = drag_refresh_to(&mut app, &renderer, viewport, 1.0);
    assert_eq!(
        last_refresh_value(&messages),
        5_000,
        "the right drag clamps to max"
    );
    apply(&mut app, messages);
    assert_eq!(
        app.preferences().refresh_ms,
        5_000,
        "max writes to settings"
    );

    let messages = drag_refresh_to(&mut app, &renderer, viewport, 0.0);
    assert_eq!(
        last_refresh_value(&messages),
        500,
        "the left drag clamps to min"
    );
    apply(&mut app, messages);
    assert_eq!(app.preferences().refresh_ms, 500, "min writes to settings");

    let messages = drag_refresh_to(&mut app, &renderer, viewport, 0.67);
    let stepped = last_refresh_value(&messages);
    assert!(
        (500..=5_000).contains(&stepped),
        "the interior value stays bounded"
    );
    assert_eq!(
        stepped % 100,
        0,
        "the 0.1-second step is preserved as 100 ms"
    );
    apply(&mut app, messages);
    assert_eq!(
        app.preferences().refresh_ms,
        stepped,
        "the consumer commits the snapped value"
    );
}

fn quiet_hours_view(app: &crate::IcedApp) -> TestElement<'_> {
    notifications_group(app.theme(), app.preferences())
}

/// The Notifications select opens its real PickList overlay, commits the
/// chosen hour, leaves the committed value unchanged when canceled by
/// clicking its trigger, and re-affirming the current value is idempotent.
#[test]
fn settings_quiet_hours_select_commits_cancels_and_reaffirms() {
    let renderer = renderer();
    let viewport = viewport();
    let mut app = crate::IcedApp::demo();

    let mut element = quiet_hours_view(&app);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let bounds = focus_bounds(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        target("quiet-hours-start"),
    );
    let point = center(bounds);
    let _ = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );

    let mut overlay = element
        .as_widget_mut()
        .overlay(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &viewport,
            Vector::ZERO,
        )
        .expect("the pointer press opens the native pick-list overlay");
    let overlay_node = overlay.as_overlay_mut().layout(&renderer, viewport.size());
    let menu_bounds = first_overlay_panel_bounds(&overlay_node, viewport.size())
        .expect("the native pick-list overlay has a bounded menu inside the viewport");
    assert!(
        !menu_bounds.intersects(&bounds),
        "the open option menu is separately hit-tested from its trigger"
    );
    let option_height = menu_bounds.height / 24.0;
    let option_five = Point::new(menu_bounds.center_x(), menu_bounds.y + option_height * 5.5);
    let mut selected = Vec::new();
    {
        let mut shell = Shell::new(&mut selected);
        let mut clipboard = iced::advanced::clipboard::Null;
        overlay.as_overlay_mut().update(
            &Event::Mouse(mouse::Event::CursorMoved {
                position: option_five,
            }),
            Layout::new(&overlay_node),
            mouse::Cursor::Available(option_five),
            &renderer,
            &mut clipboard,
            &mut shell,
        );
        overlay.as_overlay_mut().update(
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(&overlay_node),
            mouse::Cursor::Available(option_five),
            &renderer,
            &mut clipboard,
            &mut shell,
        );
    }
    assert!(
        selected.iter().any(|message| matches!(
            message,
            Message::SettingsChanged(SettingsChange::QuietHoursStart(5))
        )),
        "selecting the fifth menu row publishes the typed quiet-hours value"
    );
    drop(overlay);
    drop(element);
    apply(&mut app, selected);
    assert_eq!(
        app.preferences().quiet_start,
        5,
        "the consumer commits hour 05"
    );

    let committed = app.preferences().quiet_start;
    let mut element = quiet_hours_view(&app);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let bounds = focus_bounds(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        target("quiet-hours-start"),
    );
    let point = center(bounds);
    let _ = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    let open = element.as_widget_mut().overlay(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &viewport,
        Vector::ZERO,
    );
    assert!(
        open.is_some(),
        "the second pointer press stages a fresh choice list"
    );
    drop(open);
    let canceled = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    assert!(
        !canceled.iter().any(|message| matches!(
            message,
            Message::SettingsChanged(SettingsChange::QuietHoursStart(_))
        )),
        "clicking the open trigger closes the menu without choosing an option"
    );
    let closed = element.as_widget_mut().overlay(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &viewport,
        Vector::ZERO,
    );
    assert!(closed.is_none(), "the cancellation closes the staged menu");
    drop(closed);
    drop(tree);
    drop(element);
    apply(&mut app, canceled);
    assert_eq!(
        app.preferences().quiet_start,
        committed,
        "cancel leaves the previously committed value untouched"
    );

    let mut element = quiet_hours_view(&app);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, viewport);
    let id = target("quiet-hours-start");
    assert!(focus(&mut element, &mut tree, &node, &renderer, id));
    let reaffirmed = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(
        reaffirmed.iter().any(|message| matches!(
            message,
            Message::SettingsChanged(SettingsChange::QuietHoursStart(5))
        )),
        "Enter on the focused current selection reaffirms its typed value"
    );
    drop(tree);
    drop(element);
    apply(&mut app, reaffirmed);
    assert_eq!(
        app.preferences().quiet_start,
        committed,
        "reaffirmation is idempotent"
    );
}
