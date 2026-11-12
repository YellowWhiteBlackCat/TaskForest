use super::*;
use crate::app::Message;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use taskforest_core::core::session::{
    SessionControlAction, SessionFamilyFact, SessionIncarnation, SessionItem,
};
use taskmanager_application::AppPage;
use taskmanager_application::i18n::{Language, session_family_label, set_language};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::ProjectionSeedFact;
use taskmanager_shell::fixture::seed_projection_fact;
use taskmanager_ui_contract::presentation::sizes::ControlRole;
use taskmanager_ui_contract::{SemanticNodeId, SemanticRole};

#[test]
fn user_projection_preserves_session_identity_and_family_facts() {
    // Family labels use the shared catalog; pin English so assertions are
    // deterministic and independent of the host language.
    set_language(Language::En);

    let shell = demo_app();
    assert_eq!(user_list_state(&shell), ListState::Ready);

    let rows = user_rows(&shell);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "2");
    assert_eq!(rows[0].uid, 1000);
    assert_eq!(rows[0].user, "devuser");
    assert_eq!(rows[0].seat.as_deref(), Some("seat0"));
    assert_eq!(rows[0].tty.as_deref(), Some("tty2"));
    assert_eq!(session_family_label(rows[0].family), "Console");
    assert_eq!(session_family_label(rows[1].family), "Remote");
    assert_eq!(rows[1].timestamp.as_deref(), Some("2026-07-29 11:20"));
}

#[test]
fn user_projection_distinguishes_loading_empty_and_missing_fields() {
    // Localized copy: pin English so the assertion is identical on
    // every runner regardless of the host locale.
    set_language(Language::En);
    let shell = ShellApp::new();
    assert_eq!(user_list_state(&shell), ListState::Loading);
    assert_eq!(
        user_heading(ListState::Loading, 0),
        "Users · waiting for inventory…"
    );

    let mut shell = ShellApp::new();
    seed_projection_fact(&mut shell, ProjectionSeedFact::Sessions(Some(Vec::new())));
    assert_eq!(user_list_state(&shell), ListState::Empty);
    assert_eq!(user_heading(ListState::Empty, 0), "Users · 0 reported");

    seed_projection_fact(
        &mut shell,
        ProjectionSeedFact::Sessions(Some(vec![SessionItem {
            id: "local".into(),
            incarnation: None,
            uid: 0,
            user: "unknown".into(),
            seat: Some(String::new()),
            tty: None,
            timestamp: None,
            family: SessionFamilyFact::default(),
        }])),
    );
    let rows = user_rows(&shell);
    assert_eq!(user_list_state(&shell), ListState::Ready);
    assert_eq!(optional_text(rows[0].seat.as_deref()), "—");
    assert_eq!(optional_text(rows[0].tty.as_deref()), "—");
    assert_eq!(optional_text(rows[0].timestamp.as_deref()), "—");
    assert_eq!(session_family_label(rows[0].family), "Not observed");
}

#[test]
fn user_rows_stay_complete_and_name_cells_highlight_the_shared_query() {
    let mut shell = demo_app();
    shell.query = "DEV".into();
    shell.open_search();

    let rows = user_rows(&shell);
    assert_eq!(
        rows.len(),
        2,
        "the Users page highlights the shared query without filtering sessions"
    );
    assert!(
        rows.iter().any(|row| row.user == "devuser"),
        "the matching session remains in the canonical row set"
    );
    assert_eq!(
        crate::ui::components::highlight::cell_runs("devuser", shell.query.as_str())
            .into_iter()
            .map(|run| (run.text, run.highlighted))
            .collect::<Vec<_>>(),
        vec![("dev".to_owned(), true), ("user".to_owned(), false)],
        "the Users name cell uses the shared ASCII case-insensitive highlight ranges"
    );
    shell.query = "no-such-user".into();
    assert_eq!(
        user_rows(&shell).len(),
        2,
        "a nonmatching shared query still leaves the Users inventory unfiltered"
    );

    // The shared query remains a highlight-only concern on this page.
    let mut app = crate::IcedApp::demo();
    app.shell.query = "DEV".into();
    app.shell.open_search();
    let _ = render(&app);
}

#[derive(Default)]
struct RenderedUserWidgets {
    text: Vec<(String, Rectangle)>,
    focus_ids: Vec<Id>,
}

impl Operation for RenderedUserWidgets {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
        self.text.push((fragment.to_owned(), bounds));
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, _state: &mut dyn Focusable) {
        if let Some(id) = id {
            self.focus_ids.push(id.clone());
        }
    }
}

fn render_widget_text(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
) -> RenderedUserWidgets {
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
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &Limits::new(Size::ZERO, Size::new(1180.0, 780.0)),
    );
    let mut operation = RenderedUserWidgets::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut operation);
    operation
}

/// A headless software renderer for the real widget trees.
fn headless_renderer() -> iced::Renderer {
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

#[derive(Default)]
struct FocusById {
    target: Option<Id>,
    matched: bool,
}

impl Operation for FocusById {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if id.is_some() && id == self.target.as_ref() {
            state.focus();
            self.matched = true;
        }
    }
}

/// Focus the control registered under `target`, reporting whether it is a
/// real focus stop at all.
fn focus_target(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    node: &iced::advanced::layout::Node,
    renderer: &iced::Renderer,
    target: crate::app::FocusTarget,
) -> bool {
    let mut operation = FocusById {
        target: Some(Id::from(crate::focus::focus_id(target))),
        matched: false,
    };
    element
        .as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.matched
}

/// Deliver one Enter keypress to the widget tree and collect what it
/// published.
fn press_enter(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    node: &iced::advanced::layout::Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
) -> Vec<Message> {
    let key = iced::keyboard::key::Key::Named(iced::keyboard::key::Named::Enter);
    let event = Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    });
    let mut messages = Vec::new();
    {
        let mut shell = iced::advanced::Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        element.as_widget_mut().update(
            tree,
            &event,
            Layout::new(node),
            iced::mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard,
            &mut shell,
            viewport,
        );
    }
    messages
}

/// Deliver one pointer click to the widget tree and collect what it published.
fn click(
    element: &mut Element<'_, Message, iced::Theme, iced::Renderer>,
    tree: &mut Tree,
    node: &iced::advanced::layout::Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    point: Point,
) -> Vec<Message> {
    let mut messages = Vec::new();
    {
        let mut shell = iced::advanced::Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            element.as_widget_mut().update(
                tree,
                &event,
                Layout::new(node),
                mouse::Cursor::Available(point),
                renderer,
                &mut clipboard,
                &mut shell,
                viewport,
            );
        }
    }
    messages
}

fn menu_button(
    actions_enabled: bool,
    target: crate::app::FocusTarget,
    action: SessionControlAction,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    crate::focus::dynamic_button_owned_enabled(
        Theme::dark(),
        target,
        action_label(action).to_owned(),
        Message::RequestUserRowSessionControl(action),
        false,
        actions_enabled,
    )
}

fn action_label(action: SessionControlAction) -> &'static str {
    match action {
        SessionControlAction::Disconnect => "Disconnect",
        SessionControlAction::Lock => "Lock",
    }
}

#[test]
fn users_row_menu_actions_are_real_disabled_controls_when_the_frozen_target_is_stale() {
    set_language(Language::En);
    let renderer = headless_renderer();
    let size = Size::new(240.0, 64.0);
    let viewport = Rectangle::with_size(size);
    let action = SessionControlAction::Disconnect;
    let target = crate::app::FocusTarget::UserRowMenuDisconnect;

    for actions_enabled in [true, false] {
        let mut element = menu_button(actions_enabled, target, action);
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        let focusable = focus_target(&mut element, &mut tree, &node, &renderer, target);
        let keyboard = press_enter(&mut element, &mut tree, &node, &renderer, &viewport);
        let pointer = click(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            &viewport,
            Point::new(node.bounds().center_x(), node.bounds().center_y()),
        );
        let mut published = keyboard.iter().chain(&pointer);
        if actions_enabled {
            assert!(
                focusable,
                "a controllable session action is a real focus stop"
            );
            assert!(
                published
                    .clone()
                    .any(|message| matches!(message, Message::RequestUserRowSessionControl(observed) if *observed == action)),
                "the enabled menu action publishes its typed request"
            );
        } else {
            assert!(
                !focusable,
                "a stale session action must not be a keyboard focus stop"
            );
            assert!(
                !published
                    .any(|message| matches!(message, Message::RequestUserRowSessionControl(_))),
                "a disabled menu action publishes no keyboard or pointer request"
            );
        }
    }
}

/// The session action buttons run through ONE shared control in both
/// availability states: the available state publishes the typed
/// session-control request, the unavailable state keeps the same measured
/// extent, drops the focus stop, and publishes nothing.
#[test]
fn session_action_button_publishes_the_typed_request_only_when_enabled() {
    set_language(Language::En);
    let theme = Theme::dark();
    let renderer = headless_renderer();
    let size = Size::new(320.0, 64.0);
    let viewport = Rectangle::with_size(size);
    let target = crate::app::FocusTarget::SessionDisconnect;

    let mut enabled: Element<'_, Message, iced::Theme, iced::Renderer> = session_action_button(
        &theme,
        "Disconnect",
        SessionControlAction::Disconnect,
        target,
        true,
    );
    let mut enabled_tree = Tree::new(&enabled);
    let enabled_node = enabled.as_widget_mut().layout(
        &mut enabled_tree,
        &renderer,
        &Limits::new(Size::ZERO, size),
    );

    let mut disabled: Element<'_, Message, iced::Theme, iced::Renderer> = session_action_button(
        &theme,
        "Disconnect",
        SessionControlAction::Disconnect,
        target,
        false,
    );
    let mut disabled_tree = Tree::new(&disabled);
    let disabled_node = disabled.as_widget_mut().layout(
        &mut disabled_tree,
        &renderer,
        &Limits::new(Size::ZERO, size),
    );

    // The shared role matrix owns the extent: availability changes paint and
    // interaction, never geometry.
    assert_eq!(enabled_node.bounds().height, disabled_node.bounds().height);
    assert_eq!(
        disabled_node.bounds().height,
        crate::theme::control_extent(&theme, ControlRole::Button)
    );

    assert!(focus_target(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        target
    ));
    let published = press_enter(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        &viewport,
    );
    assert!(published.iter().any(|message| matches!(
        message,
        Message::RequestSessionControl(SessionControlAction::Disconnect)
    )));

    assert!(!focus_target(
        &mut disabled,
        &mut disabled_tree,
        &disabled_node,
        &renderer,
        target
    ));
    let silent = press_enter(
        &mut disabled,
        &mut disabled_tree,
        &disabled_node,
        &renderer,
        &viewport,
    );
    assert!(
        !silent
            .iter()
            .any(|message| matches!(message, Message::RequestSessionControl(_))),
        "an unavailable session action must publish no request"
    );
}

#[test]
fn session_control_confirmation_is_visible_and_has_dialog_semantics() {
    set_language(Language::En);
    let mut app = crate::IcedApp::demo();
    let mut sessions = app.shell.projection().sessions.clone().unwrap_or_default();
    let session = sessions
        .first_mut()
        .expect("the demo Users projection has a session");
    session.incarnation = Some(SessionIncarnation::LinuxLogind {
        boot_id: "users-confirmation-fixture".into(),
        created_monotonic_usec: 7,
    });
    let session_id = session.id.clone();
    let username = session.user.clone();
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Sessions(Some(sessions)));
    let _ = app.update(Message::SelectPage(AppPage::Users));
    let _ = app.update(Message::SelectRow(0));
    let _ = app.update(Message::RequestSessionControl(
        SessionControlAction::Disconnect,
    ));

    let mut view = crate::ui::view(&app);
    let rendered = render_widget_text(&mut view);
    for expected in [
        format!("Disconnect session {session_id} ({username})?"),
        String::from("Confirm"),
        String::from("Cancel"),
        String::from("Close"),
    ] {
        assert!(
            rendered
                .text
                .iter()
                .any(|(fragment, bounds)| fragment == &expected
                    && bounds.width > 0.0
                    && bounds.height > 0.0),
            "visible Users confirmation must include {expected:?}"
        );
    }
    for target in [
        crate::app::FocusTarget::ConfirmSessionControl,
        crate::app::FocusTarget::CancelSessionControl,
        crate::app::FocusTarget::CloseSessionControl,
    ] {
        let expected = Id::from(crate::focus::focus_id(target));
        assert!(
            rendered.focus_ids.contains(&expected),
            "session confirmation control {target:?} must be a real focus stop"
        );
    }

    let snapshot = crate::a11y::semantic_snapshot_with_local(&app)
        .expect("semantic snapshot should build with the pending session dialog");
    let dialog = snapshot
        .get(&SemanticNodeId::owned("modal:session-control-confirmation"))
        .expect("pending session control should publish a dialog node");
    assert_eq!(dialog.role(), SemanticRole::Dialog);
    assert!(dialog.state().modal);
    assert!(
        dialog
            .description()
            .is_some_and(|description| description.contains(session_id.as_str())
                && description.contains(&username)),
        "dialog semantics should name the frozen session target"
    );
}
