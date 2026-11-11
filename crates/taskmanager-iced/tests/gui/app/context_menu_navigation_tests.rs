// test-intent: behavior
//! Context-menu focus traversal and Select arrow-key ownership.

use crate::app::subscription::overlay_interaction_tests::{
    TestOverlay, build_view, first_overlay_panel_bounds, focus_overlay_target_bounds, renderer,
};
use crate::app::{FocusTarget, IcedApp, Message};
use iced::advanced::Shell;
use iced::advanced::layout::Layout;
use iced::advanced::overlay::Nested;
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Event, Point, Rectangle, Size, Vector, mouse};
use taskforest_core::core::process::ProcessLiveKey;
use taskmanager_application::{AppPage, KeyCode, Modifiers};
use taskmanager_shell::ShellKeyEvent;

#[test]
fn process_context_menu_mounts_inside_standard_and_compact_viewports() {
    let renderer = renderer();
    for size in [Size::new(1180.0, 780.0), Size::new(720.0, 480.0)] {
        let mut app = IcedApp::demo();
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
        let panel = first_overlay_panel_bounds(&overlay_node, size)
            .expect("the production menu has a bounded floating panel");
        assert!(
            panel.x >= 0.0
                && panel.y >= 0.0
                && panel.x + panel.width <= size.width + 1.0
                && panel.y + panel.height <= size.height + 1.0,
            "the menu panel stays inside {size:?}: {panel:?}"
        );
        let copy_pid = focus_overlay_target_bounds(
            &mut overlay,
            &overlay_node,
            &renderer,
            FocusTarget::ProcessMenuCopyPid,
        );
        assert!(
            copy_pid.is_some_and(|bounds| bounds.width > 0.0 && bounds.height > 0.0),
            "the mounted Process Popover exposes Copy PID at {size:?}"
        );
    }
}

#[test]
fn process_context_menu_arrows_advance_and_wrap_without_moving_the_row() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let identity = app
        .shell
        .visible_processes()
        .iter()
        .find_map(|process| ProcessLiveKey::from_process(process))
        .expect("the demo process fixture has a live row identity");
    let _ = app.update(Message::OpenProcessRowMenu { identity });
    let selected_row = app.shell.selected_row;
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessMenuEndTask)
    );

    let down = app.update(Message::Key(crate::keys::IcedKey::Fixed(
        ShellKeyEvent::new(KeyCode::ArrowDown, Modifiers::NONE),
    )));
    assert!(down.units() > 0);
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessMenuEndTree)
    );
    assert_eq!(app.shell.selected_row, selected_row);

    app.input.focused_control = Some(FocusTarget::ProcessMenuClose);
    let down = app.update(Message::Key(crate::keys::IcedKey::Fixed(
        ShellKeyEvent::new(KeyCode::ArrowDown, Modifiers::NONE),
    )));
    assert!(down.units() > 0);
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessMenuEndTask)
    );
    assert_eq!(app.shell.selected_row, selected_row);
}

#[test]
fn process_actions_arrows_leave_the_initially_focused_select_alone() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let _ = app.update(Message::OpenProcessActionsMenu);
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::ProcessPrioritySelect),
        "opening the action menu initially focuses its Select"
    );
    let selected_row = app.shell.selected_row;

    for key in [KeyCode::ArrowUp, KeyCode::ArrowDown] {
        let task = app.update(Message::Key(crate::keys::IcedKey::Fixed(
            ShellKeyEvent::new(key, Modifiers::NONE),
        )));
        assert_eq!(task.units(), 0, "ProcessActions {key:?} schedules no focus");
        assert_eq!(
            app.input.focused_control,
            Some(FocusTarget::ProcessPrioritySelect)
        );
        assert_eq!(
            app.shell.selected_row, selected_row,
            "the Select {key:?} is not routed to process-table navigation"
        );
    }
    assert!(app.process_actions_menu_open());
}

#[test]
fn process_actions_popover_forwards_the_raw_arrow_to_its_select_widget() {
    let renderer = renderer();
    let size = Size::new(720.0, 480.0);
    let mut app = IcedApp::demo();
    let _ = app.update(Message::WindowResized(size));
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let _ = app.update(Message::OpenProcessActionsMenu);

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
        .expect("the Actions menu mounts its production Popover");
    let mut overlay: TestOverlay<'_> = Nested::new(overlay);
    let overlay_node = overlay.layout(&renderer, size);
    let select = focus_overlay_target_bounds(
        &mut overlay,
        &overlay_node,
        &renderer,
        FocusTarget::ProcessPrioritySelect,
    )
    .expect("the live priority picker is a focusable Select");
    let point = Point::new(select.center_x(), select.center_y());
    let (opened, captured) = dispatch_overlay(
        &mut overlay,
        &overlay_node,
        &renderer,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    assert!(captured, "the PickList trigger owns its opening press");
    assert!(
        opened
            .iter()
            .any(|message| matches!(message, Message::Focus(FocusTarget::ProcessPrioritySelect)))
    );

    for (named, code) in [
        (Named::ArrowUp, Code::ArrowUp),
        (Named::ArrowDown, Code::ArrowDown),
    ] {
        let (messages, captured) = dispatch_overlay(
            &mut overlay,
            &overlay_node,
            &renderer,
            Event::Keyboard(key_pressed(named, code)),
            mouse::Cursor::Unavailable,
        );
        assert!(
            messages.is_empty(),
            "raw Select {named:?} does not synthesize a table or focus message"
        );
        assert!(
            !captured,
            "native widget {named:?} remains available to the focused Select"
        );
    }
}

fn dispatch_overlay(
    overlay: &mut TestOverlay<'_>,
    node: &iced::advanced::layout::Node,
    renderer: &iced::Renderer,
    event: Event,
    cursor: mouse::Cursor,
) -> (Vec<Message>, bool) {
    let mut messages = Vec::new();
    let captured = {
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
        shell.is_event_captured()
    };
    (messages, captured)
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
