use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::operation::Operation;
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use taskmanager_application::AppPage;
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree_with_partial_aggregate;
use taskmanager_shell::{InfoSortCol, InfoTable, SortCol, app_tree_expansion_key_for_identity};

use super::*;

#[test]
fn adapter_state_obeys_iced_focusable_contract() {
    let mut state = widget::State::default();
    assert!(!state.is_focused());

    state.focus();
    assert!(state.is_focused());

    state.unfocus();
    assert!(!state.is_focused());
}

#[test]
fn every_declared_focus_target_has_a_unique_operation_id() {
    let ids: Vec<_> = FocusTarget::ALL.into_iter().map(focus_id).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
    assert!(ids.iter().all(|id| id.starts_with("iced-")));
}

#[test]
fn table_row_operation_ids_are_stable_and_page_bound() {
    let first = focus_id(FocusTarget::TableRow {
        page: AppPage::Applications,
        index: 0,
    });
    let second = focus_id(FocusTarget::TableRow {
        page: AppPage::Applications,
        index: 1,
    });
    let other_page = focus_id(FocusTarget::TableRow {
        page: AppPage::Services,
        index: 0,
    });
    assert_eq!(first, "iced-table-row-applications-0");
    assert_ne!(first, second);
    assert_ne!(first, other_page);
}

type TestElement<'a> = Element<'a, crate::app::Message, iced::Theme, iced::Renderer>;

struct TargetBounds {
    target: Id,
    bounds: Option<Rectangle>,
}

impl Operation<()> for TargetBounds {
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
    app: &'a crate::IcedApp,
    renderer: &iced::Renderer,
    size: Size,
) -> (TestElement<'a>, Tree, Node) {
    let mut view = crate::ui::view(app);
    let mut tree = Tree::new(&view);
    let node = view
        .as_widget_mut()
        .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));
    (view, tree, node)
}

fn focus_target_bounds(
    view: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: FocusTarget,
) -> Option<Rectangle> {
    let mut operation = TargetBounds {
        target: Id::from(focus_id(target)),
        bounds: None,
    };
    view.as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.bounds
}

fn dispatch(
    view: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    viewport: &Rectangle,
    event: Event,
    cursor: mouse::Cursor,
) -> Vec<crate::app::Message> {
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
fn sortable_headers_share_registered_focus_and_typed_pointer_keyboard_actions() {
    let size = Size::new(1180.0, 780.0);
    let viewport = Rectangle::with_size(size);
    let renderer = renderer();
    let target = FocusTarget::ProcessSortHeader(SortCol::Cpu);
    let mut keyboard_app = crate::IcedApp::demo();
    let _ = keyboard_app.update(crate::app::Message::WindowResized(size));
    let _ = keyboard_app.update(crate::app::Message::SelectPage(AppPage::Applications));
    let (mut keyboard_view, mut keyboard_tree, keyboard_node) =
        build_view(&keyboard_app, &renderer, size);
    assert!(
        focus_target_bounds(
            &mut keyboard_view,
            &mut keyboard_tree,
            &keyboard_node,
            &renderer,
            target,
        )
        .is_some()
    );
    let keyboard_messages = dispatch(
        &mut keyboard_view,
        &mut keyboard_tree,
        &keyboard_node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(
        keyboard_messages.iter().any(
            |message| matches!(message, crate::app::Message::Focus(found) if *found == target)
        )
    );
    assert!(
        keyboard_messages
            .iter()
            .any(|message| matches!(message, crate::app::Message::SortBy(SortCol::Cpu)))
    );

    let mut pointer_app = crate::IcedApp::demo();
    let _ = pointer_app.update(crate::app::Message::WindowResized(size));
    let _ = pointer_app.update(crate::app::Message::SelectPage(AppPage::Applications));
    let (mut pointer_view, mut pointer_tree, pointer_node) =
        build_view(&pointer_app, &renderer, size);
    let bounds = focus_target_bounds(
        &mut pointer_view,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        target,
    )
    .expect("the actual Applications sort header is registered as a focus stop");
    let point = Point::new(bounds.center_x(), bounds.center_y());
    let press = dispatch(
        &mut pointer_view,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    let release = dispatch(
        &mut pointer_view,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    assert!(
        press.iter().any(
            |message| matches!(message, crate::app::Message::Focus(found) if *found == target)
        )
    );
    assert!(
        !press
            .iter()
            .any(|message| matches!(message, crate::app::Message::SortBy(_)))
    );
    assert!(
        release
            .iter()
            .any(|message| matches!(message, crate::app::Message::SortBy(SortCol::Cpu)))
    );

    let mut inventory_app = crate::IcedApp::demo();
    let _ = inventory_app.update(crate::app::Message::WindowResized(size));
    let _ = inventory_app.update(crate::app::Message::SelectPage(AppPage::Startup));
    let (mut inventory_view, mut inventory_tree, inventory_node) =
        build_view(&inventory_app, &renderer, size);
    let inventory_target = FocusTarget::InfoSortHeader {
        table: InfoTable::Startup,
        column: InfoSortCol::Name,
    };
    assert!(
        focus_target_bounds(
            &mut inventory_view,
            &mut inventory_tree,
            &inventory_node,
            &renderer,
            inventory_target,
        )
        .is_some()
    );
    let inventory_messages = dispatch(
        &mut inventory_view,
        &mut inventory_tree,
        &inventory_node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(inventory_messages.iter().any(|message| matches!(
        message,
        crate::app::Message::Focus(found) if *found == inventory_target
    )));
    assert!(inventory_messages.iter().any(|message| matches!(
        message,
        crate::app::Message::SortInfoTable {
            table: InfoTable::Startup,
            column: InfoSortCol::Name,
        }
    )));
}

#[test]
fn process_group_and_parent_rows_are_focusable_and_keep_their_typed_actions() {
    let size = Size::new(1180.0, 780.0);
    let viewport = Rectangle::with_size(size);
    let renderer = renderer();
    let mut app = crate::IcedApp::demo();
    let _ = app.update(crate::app::Message::WindowResized(size));
    let _ = app.update(crate::app::Message::SelectPage(AppPage::Applications));
    let root = seed_shell_process_tree_with_partial_aggregate(&mut app.shell)
        .expect("the demo shell accepts the deterministic process-tree facts");
    app.shell.query = "capture".to_owned();
    let _ = app.update(crate::app::Message::ExpandAllProcessTree);
    let expansion_key = app_tree_expansion_key_for_identity(root);
    let root_group_expanded = app.projected_rows().rows().iter().any(|row| {
        matches!(
            row,
            crate::ui::process_projection::ProjectedRow::GroupHeader {
                expansion_key: key,
                expanded: true,
                ..
            } if key == &expansion_key
        )
    });
    if !root_group_expanded {
        let _ = app.update(crate::app::Message::ToggleGroupExpansion {
            name: expansion_key.clone(),
            row_key: None,
        });
    }
    let rows = app.projected_rows();
    let group_index = rows
        .rows()
        .iter()
        .position(|row| {
            matches!(
                row,
                crate::ui::process_projection::ProjectedRow::GroupHeader {
                    expansion_key: key,
                    ..
                } if key == &expansion_key
            )
        })
        .expect("the application aggregate header is projected");
    let process_index = rows
        .rows()
        .iter()
        .find_map(|row| match row {
            crate::ui::process_projection::ProjectedRow::Tree {
                flat_index,
                has_children: true,
                ..
            } => Some(*flat_index),
            _ => None,
        })
        .expect("the fixture has an expanded process parent");
    drop(rows);

    let group_target = FocusTarget::ProcessGroupHeader(group_index);
    let (mut group_view, mut group_tree, group_node) = build_view(&app, &renderer, size);
    assert!(
        focus_target_bounds(
            &mut group_view,
            &mut group_tree,
            &group_node,
            &renderer,
            group_target,
        )
        .is_some()
    );
    let group_messages = dispatch(
        &mut group_view,
        &mut group_tree,
        &group_node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(group_messages.iter().any(|message| matches!(
        message,
        crate::app::Message::Focus(found) if *found == group_target
    )));
    assert!(group_messages.iter().any(|message| matches!(
        message,
        crate::app::Message::ToggleGroupExpansion { name, .. } if name == &expansion_key
    )));

    let tree_target = FocusTarget::ProcessTreeNode(process_index);
    let (mut tree_view, mut tree, node) = build_view(&app, &renderer, size);
    assert!(
        focus_target_bounds(&mut tree_view, &mut tree, &node, &renderer, tree_target,).is_some()
    );
    let tree_messages = dispatch(
        &mut tree_view,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(tree_messages.iter().any(|message| matches!(
        message,
        crate::app::Message::Focus(found) if *found == tree_target
    )));
    assert!(tree_messages.iter().any(|message| matches!(
        message,
        crate::app::Message::ActivateTreeNode { flat_index, .. }
            if *flat_index == process_index
    )));
}
