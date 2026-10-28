// test-intent: behavior
//! Interaction evidence for the Iced-owned overlay lifecycle.

use crate::app::{ContextMenuKind, FocusTarget, IcedApp, LocalSurfaceKind, Message};
use crate::focus;
use crate::ui;
use crate::ui::smart_self_test_confirmation::confirmation;
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::overlay::Nested;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Element, Event, Point, Rectangle, Size, Vector, mouse};
use taskmanager_application::AppPage;
use taskmanager_assets::embedded_fonts;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_shell::SortCol;
use taskmanager_shell::fixture::health::seed_smart_self_test_capabilities;
use taskmanager_shell::presentation::smart::self_test_intent;

type TestOverlay<'a> = Nested<'a, Message, iced::Theme, iced::Renderer>;

struct FocusTargetBounds {
    target: Id,
    bounds: Option<Rectangle>,
}

impl Operation<()> for FocusTargetBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            state.focus();
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

fn build_view<'a>(
    app: &'a IcedApp,
    renderer: &iced::Renderer,
    size: Size,
) -> (
    Element<'a, Message, iced::Theme, iced::Renderer>,
    Tree,
    Node,
) {
    let mut view = ui::view(app);
    let mut tree = Tree::new(&view);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));
    (view, tree, node)
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

fn focus_target_bounds(
    view: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: FocusTarget,
) -> Option<Rectangle> {
    let mut operation = FocusTargetBounds {
        target: Id::from(focus::focus_id(target)),
        bounds: None,
    };
    view.as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.bounds
}

fn focus_overlay_target_bounds(
    overlay: &mut TestOverlay<'_>,
    node: &Node,
    renderer: &iced::Renderer,
    target: FocusTarget,
) -> Option<Rectangle> {
    let mut operation = FocusTargetBounds {
        target: Id::from(focus::focus_id(target)),
        bounds: None,
    };
    overlay.operate(Layout::new(node), renderer, &mut operation);
    operation.bounds
}

fn dispatch(
    view: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
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
        view.as_widget_mut().update(
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

fn dispatch_overlay(
    overlay: &mut TestOverlay<'_>,
    node: &Node,
    renderer: &iced::Renderer,
    event: Event,
    cursor: mouse::Cursor,
) -> Vec<Message> {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        overlay.update(
            &event,
            Layout::new(node),
            cursor,
            renderer,
            &mut clipboard,
            &mut shell,
        );
    }
    messages
}

fn apply_messages(app: &mut IcedApp, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

fn key_pressed(named: Named, code: Code) -> iced::keyboard::Event {
    let key = Key::Named(named);
    iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: Physical::Code(code),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    }
}

#[test]
fn window_deactivation_removes_the_real_user_context_menu_without_submitting_an_action() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Users));
    let _ = app.update(Message::OpenUserRowMenu(1));
    assert_eq!(app.context_menu_kind(), Some(ContextMenuKind::User));
    assert!(app.user_menu_session().is_some());
    let feedback_before_deactivation = app.shell.feedback_text().to_owned();

    let renderer = renderer();
    let menu_action_was_mounted = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        let overlay = view
            .as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &Rectangle::with_size(size),
                Vector::ZERO,
            )
            .expect("the open Users row menu mounts through the overlay channel");
        let mut overlay = Nested::new(overlay);
        let overlay_node = overlay.layout(&renderer, size);
        focus_overlay_target_bounds(
            &mut overlay,
            &overlay_node,
            &renderer,
            FocusTarget::UserRowMenuDisconnect,
        )
        .is_some()
    };
    assert!(
        menu_action_was_mounted,
        "the open row menu exposes its action"
    );

    let message = super::message_for_window_event(iced::window::Event::Unfocused)
        .expect("Iced window deactivation must produce the dismiss message");
    assert!(matches!(&message, Message::WindowUnfocused));
    let _ = app.update(message);

    assert_eq!(app.context_menu_kind(), None);
    assert!(app.user_menu_session().is_none());
    assert!(app.shell.pending_session().is_none());
    assert_eq!(
        app.shell.feedback_text(),
        feedback_before_deactivation.as_str(),
        "window deactivation closes the menu without submitting its session action"
    );
    let menu_action_remains_mounted = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        view.as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &Rectangle::with_size(size),
                Vector::ZERO,
            )
            .is_some()
    };
    assert!(
        !menu_action_remains_mounted,
        "deactivation removes the menu action from the actual view"
    );
    assert!(
        super::message_for_window_event(iced::window::Event::Focused).is_none(),
        "window activation alone does not dismiss or submit a menu action"
    );
}

#[test]
fn process_context_menu_mounts_its_production_popover_action() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let identity = app
        .shell
        .visible_processes()
        .iter()
        .find_map(|process| ProcessLiveKey::from_process(process))
        .expect("the demo process fixture has a live row identity");
    let _ = app.update(Message::OpenProcessRowMenu { identity });
    assert_eq!(app.process_menu_identity(), Some(identity));

    let renderer = renderer();
    let (mut view, mut tree, node) = build_view(&app, &renderer, size);
    let overlay = view
        .as_widget_mut()
        .overlay(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &Rectangle::with_size(size),
            Vector::ZERO,
        )
        .expect("the selected process mounts its production Popover overlay");
    let mut overlay = Nested::new(overlay);
    let overlay_node = overlay.layout(&renderer, size);
    let copy_pid = focus_overlay_target_bounds(
        &mut overlay,
        &overlay_node,
        &renderer,
        FocusTarget::ProcessMenuCopyPid,
    );
    assert!(
        copy_pid.is_some_and(|bounds| bounds.width > 0.0 && bounds.height > 0.0),
        "the mounted Process Popover exposes its real Copy PID action"
    );
}

#[test]
fn smart_self_test_confirmation_buttons_fit_the_shared_component_at_standard_and_compact_rail_widths()
 {
    let mut app = IcedApp::demo();
    seed_smart_self_test_capabilities(&mut app.shell);
    let intent = self_test_intent(
        &app.shell
            .projection()
            .snapshot
            .as_ref()
            .expect("demo snapshot")
            .disks[0],
        SmartSelfTestKind::Short,
    )
    .expect("demo disk reports explicit Short support");
    let renderer = renderer();
    for width in [340.0, 280.0] {
        let max_size = Size::new(width, 120.0);
        let mut confirmation_view = confirmation(app.theme(), &intent);
        let mut tree = Tree::new(&confirmation_view);
        let node = confirmation_view.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &Limits::new(Size::ZERO, max_size),
        );
        let panel = node.bounds();
        let confirm = focus_target_bounds(
            &mut confirmation_view,
            &mut tree,
            &node,
            &renderer,
            FocusTarget::ConfirmSmartSelfTest,
        )
        .expect("confirm action is mounted");
        let cancel = focus_target_bounds(
            &mut confirmation_view,
            &mut tree,
            &node,
            &renderer,
            FocusTarget::CancelSmartSelfTest,
        )
        .expect("cancel action is mounted");

        for (name, bounds) in [("confirm", confirm), ("cancel", cancel)] {
            assert!(bounds.width > 0.0 && bounds.height > 0.0, "{name} has size");
            assert!(
                bounds.x >= panel.x,
                "{name} stays inside the panel: {bounds:?}"
            );
            assert!(
                bounds.y >= panel.y,
                "{name} stays inside the panel: {bounds:?}"
            );
            assert!(
                bounds.x + bounds.width <= panel.x + panel.width + 1.0,
                "{name} right edge stays inside the panel: {bounds:?} / {panel:?}"
            );
            assert!(
                bounds.y + bounds.height <= panel.y + panel.height + 1.0,
                "{name} bottom edge stays inside the panel: {bounds:?} / {panel:?}"
            );
        }
        let overlap_x = confirm.x < cancel.x + cancel.width && cancel.x < confirm.x + confirm.width;
        let overlap_y =
            confirm.y < cancel.y + cancel.height && cancel.y < confirm.y + confirm.height;
        assert!(
            !(overlap_x && overlap_y),
            "confirm and cancel do not overlap"
        );
    }
}

#[test]
fn modal_scrim_and_cancel_events_never_activate_the_page_underneath() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let original_page = app.shell.page();

    let renderer = renderer();
    let underlying_page_tab = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        focus_target_bounds(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            FocusTarget::PageTab(AppPage::Performance),
        )
        .expect("the underlying page tab is present before the modal opens")
    };
    let scrim_point = Point::new(
        underlying_page_tab.x + underlying_page_tab.width / 2.0,
        underlying_page_tab.y + underlying_page_tab.height / 2.0,
    );
    assert!(
        scrim_point.y < size.height / 4.0,
        "the old navigation target lies on the modal scrim, outside its centered panel"
    );

    let _ = app.update(Message::OpenRunTask);
    assert_eq!(app.local_surface_kind(), Some(LocalSurfaceKind::RunTask));
    let (scrim_messages, cancel_messages) = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        let viewport = Rectangle::with_size(size);
        assert!(
            focus_target_bounds(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                FocusTarget::RunTaskCommandInput,
            )
            .is_some(),
            "the opened Run Task surface exposes its tracked command input"
        );
        let press = dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            mouse::Cursor::Available(scrim_point),
        );
        let release = dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Available(scrim_point),
        );
        assert!(
            press.iter().chain(&release).all(|message| {
                !matches!(message, Message::SelectPage(_))
                    && !matches!(
                        message,
                        Message::FocusedTargetUnmounted(FocusTarget::RunTaskCommandInput)
                    )
            }),
            "a click on the scrim cannot reach the removed page navigation: {press:?} / {release:?}"
        );
        let scrim_messages = press.into_iter().chain(release).collect::<Vec<_>>();

        let cancel_bounds = focus_target_bounds(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            FocusTarget::RunTaskCancel,
        )
        .expect("the open task dialog renders its Cancel control");
        let cancel_point = Point::new(
            cancel_bounds.x + cancel_bounds.width / 2.0,
            cancel_bounds.y + cancel_bounds.height / 2.0,
        );
        let _ = dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            mouse::Cursor::Available(cancel_point),
        );
        let cancel_messages = dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Available(cancel_point),
        );
        (scrim_messages, cancel_messages)
    };
    apply_messages(&mut app, scrim_messages);
    assert_eq!(
        app.shell.page(),
        original_page,
        "scrim input leaves the underlying page selection unchanged"
    );
    assert!(
        matches!(cancel_messages.as_slice(), [Message::CloseRunTask]),
        "the actual Cancel click emits only the dialog dismissal"
    );
    let _ = app.update(cancel_messages.into_iter().next().expect("cancel message"));
    assert!(app.local_surface_kind().is_none());
    assert_eq!(app.shell.page(), original_page);
}

#[test]
fn dropdown_escape_restores_the_trigger_and_enter_reopens_the_menu() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let renderer = renderer();
    let open_messages = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        assert!(
            focus_target_bounds(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                FocusTarget::ProcessColumnsTrigger,
            )
            .is_some(),
            "the Applications view mounts the dropdown trigger"
        );
        dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &Rectangle::with_size(size),
            Event::Keyboard(key_pressed(Named::Enter, Code::Enter)),
            mouse::Cursor::Unavailable,
        )
    };
    assert!(
        open_messages
            .iter()
            .any(|message| matches!(message, Message::OpenProcessColumnsMenu)),
        "the actual dropdown trigger should open the menu: {open_messages:?}"
    );
    apply_messages(&mut app, open_messages);
    assert!(app.process_columns_menu_open());
    let hidden_columns_before_escape = app.process_presentation.hidden_columns.clone();

    let escape = super::message_for_keyboard_event(key_pressed(Named::Escape, Code::Escape));
    assert!(matches!(&escape, Message::Key(_)));
    let _ = app.update(escape);
    assert!(!app.process_columns_menu_open());
    assert_eq!(
        app.process_presentation.hidden_columns, hidden_columns_before_escape,
        "Escape dismisses without toggling a menu item"
    );
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessColumnsTrigger),
        "Escape restores the renderer's focus target to the menu trigger"
    );

    let reopen_messages = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        assert!(
            focus_target_bounds(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                FocusTarget::ProcessColumnsTrigger,
            )
            .is_some(),
            "the closed dropdown keeps its real trigger mounted"
        );
        dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &Rectangle::with_size(size),
            Event::Keyboard(key_pressed(Named::Enter, Code::Enter)),
            mouse::Cursor::Unavailable,
        )
    };
    assert!(
        reopen_messages
            .iter()
            .any(|message| matches!(message, Message::OpenProcessColumnsMenu)),
        "Enter on the restored trigger publishes the normal open message: {reopen_messages:?}"
    );
    apply_messages(&mut app, reopen_messages);
    assert!(app.process_columns_menu_open());
}

#[test]
fn dropdown_checkbox_toggles_keep_the_real_process_columns_menu_open() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let renderer = renderer();

    let open_messages = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        assert!(
            focus_target_bounds(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                FocusTarget::ProcessColumnsTrigger,
            )
            .is_some()
        );
        dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &Rectangle::with_size(size),
            Event::Keyboard(key_pressed(Named::Enter, Code::Enter)),
            mouse::Cursor::Unavailable,
        )
    };
    assert!(
        open_messages
            .iter()
            .any(|message| matches!(message, Message::OpenProcessColumnsMenu)),
        "the actual dropdown trigger should open the menu: {open_messages:?}"
    );
    apply_messages(&mut app, open_messages);
    assert!(app.process_columns_menu_open());
    assert!(
        !app.process_presentation
            .hidden_columns
            .contains(&SortCol::Cpu)
    );
    assert!(
        !app.process_presentation
            .hidden_columns
            .contains(&SortCol::Memory)
    );

    for (column, named, code) in [
        (SortCol::Memory, Named::Space, Code::Space),
        (SortCol::Cpu, Named::Enter, Code::Enter),
    ] {
        let messages = {
            let mut view = ui::view(&app);
            let mut tree = Tree::new(&view);
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
            let overlay = view
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &Rectangle::with_size(size),
                    Vector::ZERO,
                )
                .expect("the open ProcessColumns menu mounts as a real overlay");
            let mut overlay = Nested::new(overlay);
            let overlay_node = overlay.layout(&renderer, size);
            assert!(
                focus_overlay_target_bounds(
                    &mut overlay,
                    &overlay_node,
                    &renderer,
                    FocusTarget::ProcessColumnToggle(column),
                )
                .is_some()
            );
            dispatch_overlay(
                &mut overlay,
                &overlay_node,
                &renderer,
                Event::Keyboard(key_pressed(named, code)),
                mouse::Cursor::Unavailable,
            )
        };
        assert!(messages.iter().any(
            |message| matches!(message, Message::ToggleProcessColumn(found) if *found == column)
        ));
        apply_messages(&mut app, messages);
        assert!(app.process_presentation.hidden_columns.contains(&column));
        assert!(
            app.process_columns_menu_open(),
            "the {column:?} checkbox can be toggled without dismissing the multi-select menu"
        );

        let item_remains_mounted = {
            let mut view = ui::view(&app);
            let mut tree = Tree::new(&view);
            let node =
                view.as_widget_mut()
                    .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
            let overlay = view
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &Rectangle::with_size(size),
                    Vector::ZERO,
                )
                .expect("the dropdown remains mounted after each checkbox toggle");
            let mut overlay = Nested::new(overlay);
            let overlay_node = overlay.layout(&renderer, size);
            focus_overlay_target_bounds(
                &mut overlay,
                &overlay_node,
                &renderer,
                FocusTarget::ProcessColumnToggle(column),
            )
            .is_some()
        };
        assert!(
            item_remains_mounted,
            "the toggled checkbox remains available"
        );
    }
}

#[test]
fn dropdown_outside_press_dismisses_and_forwards_the_page_target_beneath() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let renderer = renderer();
    let viewport = Rectangle::with_size(size);

    let open_messages = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        assert!(
            focus_target_bounds(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                FocusTarget::ProcessColumnsTrigger,
            )
            .is_some(),
            "the Applications view mounts its actual dropdown trigger"
        );
        dispatch(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Event::Keyboard(key_pressed(Named::Enter, Code::Enter)),
            mouse::Cursor::Unavailable,
        )
    };
    assert!(
        open_messages
            .iter()
            .any(|message| matches!(message, Message::OpenProcessColumnsMenu)),
        "dropdown trigger output was {open_messages:?}"
    );
    apply_messages(&mut app, open_messages);
    assert!(app.process_columns_menu_open());

    let (press_messages, captured, outside_point) = {
        let mut view = ui::view(&app);
        let mut tree = Tree::new(&view);
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        let page_tab = focus_target_bounds(
            &mut view,
            &mut tree,
            &node,
            &renderer,
            FocusTarget::PageTab(AppPage::Performance),
        )
        .expect("the underlying Performance tab is mounted before dismissal");
        let outside_point = Point::new(
            page_tab.x + page_tab.width / 2.0,
            page_tab.y + page_tab.height / 2.0,
        );
        let overlay = view
            .as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &viewport,
                Vector::ZERO,
            )
            .expect("the open ProcessColumns dropdown mounts as an overlay");
        let mut overlay = Nested::new(overlay);
        let overlay_node = overlay.layout(&renderer, size);
        let toggle_bounds = focus_overlay_target_bounds(
            &mut overlay,
            &overlay_node,
            &renderer,
            FocusTarget::ProcessColumnToggle(SortCol::Cpu),
        )
        .expect("the open dropdown mounts its CPU choice");
        assert!(
            outside_point.y + 16.0 < toggle_bounds.y,
            "the Performance tab {:?} must lie above the dropdown choice {:?}",
            outside_point,
            toggle_bounds
        );

        let mut messages = Vec::new();
        let captured;
        {
            let mut shell = Shell::new(&mut messages);
            let mut clipboard = iced::advanced::clipboard::Null;
            overlay.update(
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Layout::new(&overlay_node),
                mouse::Cursor::Available(outside_point),
                &renderer,
                &mut clipboard,
                &mut shell,
            );
            captured = shell.is_event_captured();
        }
        drop(overlay);
        drop(view);
        (messages, captured, outside_point)
    };
    assert!(
        !captured,
        "the shared policy forwards the dismissal press to the background target"
    );
    assert!(matches!(
        press_messages.as_slice(),
        [Message::CloseProcessColumnsMenu]
    ));
    apply_messages(&mut app, press_messages);
    assert!(!app.process_columns_menu_open());
    assert_eq!(
        app.shell.page(),
        AppPage::Applications,
        "the overlay still owns the page until the base tree receives the press"
    );
    assert!(
        outside_point.y < size.height / 4.0,
        "the tested background target is the visible navigation tab"
    );

    // The forwarded press reaches the base tree; the page tab activates on the
    // press/release pair just as it does without an overlay.
    let messages = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        let mut messages = Vec::new();
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            messages.extend(dispatch(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                &viewport,
                event,
                mouse::Cursor::Available(outside_point),
            ));
        }
        messages
    };
    apply_messages(&mut app, messages);
    assert_eq!(
        app.shell.page(),
        AppPage::Performance,
        "the forwarded press activates the underlying Performance tab"
    );
}

#[test]
fn context_menu_outside_press_dismisses_and_forwards_the_page_target() {
    let mut app = IcedApp::demo();
    let size = Size::new(1180.0, 780.0);
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Users));
    let _ = app.update(Message::OpenUserRowMenu(1));
    assert_eq!(app.context_menu_kind(), Some(ContextMenuKind::User));
    assert!(app.user_menu_session().is_some());
    let feedback_before_press = app.shell.feedback_text().to_owned();
    let renderer = renderer();
    let viewport = Rectangle::with_size(size);

    let mut view = ui::view(&app);
    let mut tree = Tree::new(&view);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
    let page_tab_bounds = focus_target_bounds(
        &mut view,
        &mut tree,
        &node,
        &renderer,
        FocusTarget::PageTab(AppPage::Performance),
    )
    .expect("the underlying page tab is mounted beneath the open context menu");
    let outside_point = Point::new(
        page_tab_bounds.x + page_tab_bounds.width / 2.0,
        page_tab_bounds.y + page_tab_bounds.height / 2.0,
    );
    let overlay = view
        .as_widget_mut()
        .overlay(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &viewport,
            Vector::ZERO,
        )
        .expect("the actual Users row context menu mounts through the overlay channel");
    let mut overlay = Nested::new(overlay);
    let overlay_node = overlay.layout(&renderer, viewport.size());
    let panel_bounds = first_overlay_panel_bounds(&overlay_node, viewport.size())
        .expect("the floating context menu has a bounded panel inside the viewport");
    assert!(
        !panel_bounds.contains(outside_point),
        "the underlying page tab lies outside the floating context menu"
    );

    let (press_messages, captured) = {
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        overlay.update(
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(&overlay_node),
            mouse::Cursor::Available(outside_point),
            &renderer,
            &mut clipboard,
            &mut shell,
        );
        let captured = shell.is_event_captured();
        (messages, captured)
    };
    assert!(
        !captured,
        "the shared context-menu policy forwards the dismissal press"
    );
    assert!(matches!(
        press_messages.as_slice(),
        [Message::CloseUserRowMenu]
    ));
    let release_messages = dispatch_overlay(
        &mut overlay,
        &overlay_node,
        &renderer,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(outside_point),
    );
    assert!(
        release_messages.is_empty(),
        "the outside release has no action"
    );
    drop(overlay);
    drop(view);
    apply_messages(&mut app, press_messages);

    assert_eq!(app.context_menu_kind(), None);
    assert!(app.user_menu_session().is_none());
    assert!(app.shell.pending_session().is_none());
    assert_eq!(
        app.shell.feedback_text(),
        feedback_before_press.as_str(),
        "outside dismissal cannot submit the session action or change feedback"
    );
    assert_eq!(
        app.shell.page(),
        AppPage::Users,
        "the overlay still owns the page until the base tree receives the press"
    );

    // The forwarded press reaches the base tree; the page tab activates while
    // the dismissed session action stays unsubmitted.
    let messages = {
        let (mut view, mut tree, node) = build_view(&app, &renderer, size);
        let mut messages = Vec::new();
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            messages.extend(dispatch(
                &mut view,
                &mut tree,
                &node,
                &renderer,
                &viewport,
                event,
                mouse::Cursor::Available(outside_point),
            ));
        }
        messages
    };
    apply_messages(&mut app, messages);
    assert_eq!(
        app.shell.page(),
        AppPage::Performance,
        "the forwarded press activates the underlying Performance tab"
    );
    assert!(app.shell.pending_session().is_none());
    assert_eq!(
        app.shell.feedback_text(),
        feedback_before_press.as_str(),
        "the forwarded page change cannot submit the dismissed session action"
    );
}
