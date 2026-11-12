// test-intent: behavior
//
// Behavior tests for the shared Iced control family: real widget trees,
// typed messages, resolved geometry, and the shared glass/focus-ring seams.

use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::Scrollable;
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::widget::{button, text_editor};
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use taskmanager_application::AppPage;
use taskmanager_application::i18n::{Language, with_language};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_theme::tokens::{RowDensity, SPACE_6};
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

use crate::Message;
use crate::app::FocusTarget;
use crate::focus::focus_id;
use crate::theme::{ControlVisualState, control_glass};
use crate::ui::components::divider;

use super::activation::{checkbox_style, checked_glass_background};
use super::state::{button_ink, button_style, text_editor_ink};
use super::text::{text_editor_style, text_input_style};
use super::*;

type TestElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

/// Bind the production multi-line editor into the compact component suite.
fn production_multi_line_text_input<'a>(
    theme_snapshot: &'a Theme,
    content: &'a text_editor::Content,
) -> TestElement<'a> {
    multi_line_text_input(
        theme_snapshot,
        None,
        content,
        "hint",
        Message::UpdateRunTaskEditor,
        None,
        false,
    )
}

fn production_text_editor_style(theme: &Theme, state: ControlVisualState) -> text_editor::Style {
    text_editor_style(theme, ControlRole::Input, state)
}

fn editor_paint_state(status: text_editor::Status) -> ControlVisualState {
    text_editor_ink(status)
}

#[derive(Default)]
struct StripBounds {
    focus: Vec<(Id, Rectangle)>,
    scroll: Vec<(Option<Id>, Rectangle, Rectangle)>,
}

impl iced::advanced::widget::Operation<()> for StripBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<()>)) {
        visit(self);
    }

    fn focusable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        if let Some(id) = id {
            self.focus.push((id.clone(), bounds));
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        _translation: iced::Vector,
        _state: &mut dyn Scrollable,
    ) {
        self.scroll.push((id.cloned(), bounds, content));
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

fn theme() -> Theme {
    Theme::build(
        Skin::Gnome,
        LightDark::Dark,
        HighContrast::Off,
        ResolvedFonts::system_for(Skin::Gnome),
    )
    .with_focus_visible(true)
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

/// Every control style is assembled from the same shared glass projection.
#[test]
fn control_styles_share_one_glass_material_and_keep_state_geometry_invariant() {
    let theme = theme();
    assert!(!ControlState::Focused.changes_measurement());
    assert!(!ControlState::Disabled.changes_measurement());

    for role in [ControlRole::Button, ControlRole::Input, ControlRole::Toggle] {
        let baseline = crate::presentation_binding::control_measurement(
            role,
            ControlState::Normal,
            crate::theme::ui_size_from_root(theme.root_rem_size),
            RowDensity::Comfortable,
        );
        let active = control_glass(&theme, role, ControlVisualState::Active);
        let focused = control_glass(&theme, role, ControlVisualState::Focused);
        let pointer_focused = control_glass(
            &theme.with_focus_visible(false),
            role,
            ControlVisualState::Focused,
        );
        assert_eq!(
            focused.border.color,
            crate::theme::focus_ring_color(&theme, false)
        );
        assert_eq!(focused.border.width, crate::theme::FOCUS_RING_WIDTH);
        assert_eq!(pointer_focused.border, active.border);
        for state in ControlState::ALL {
            let measured = crate::presentation_binding::control_measurement(
                role,
                state,
                crate::theme::ui_size_from_root(theme.root_rem_size),
                RowDensity::Comfortable,
            );
            assert_eq!(measured.extent, baseline.extent);
            assert_eq!(measured.padding_x, baseline.padding_x);
        }
    }
    let checked = checkbox_style(&theme, ControlVisualState::Active, true);
    let unchecked = checkbox_style(&theme, ControlVisualState::Active, false);
    assert_ne!(checked.background, unchecked.background);
    assert_eq!(checked.border, unchecked.border);
    let active = button_style(
        &theme,
        ControlRole::Button,
        ButtonTone::Secondary,
        ControlVisualState::Active,
    );
    let pressed = button_style(
        &theme,
        ControlRole::Button,
        ButtonTone::Secondary,
        ControlVisualState::Pressed,
    );
    assert_eq!(
        active.background,
        control_glass(&theme, ControlRole::Button, ControlVisualState::Active,).background
    );
    assert_eq!(
        pressed.background,
        control_glass(&theme, ControlRole::Button, ControlVisualState::Pressed,).background
    );
    assert_eq!(active.border, pressed.border);
    assert_eq!(active.shadow.offset, pressed.shadow.offset);
    assert_eq!(active.shadow.blur_radius, pressed.shadow.blur_radius);
    assert!(pressed.shadow.color.a < active.shadow.color.a);
    let input_active = text_input_style(&theme, ControlRole::Input, ControlVisualState::Active);
    let input_focused = text_input_style(&theme, ControlRole::Input, ControlVisualState::Focused);
    assert_eq!(input_active.border.radius, input_focused.border.radius);
    assert_eq!(
        input_focused.border.color,
        crate::theme::focus_ring_color(&theme, false)
    );
    let editor_active = production_text_editor_style(&theme, ControlVisualState::Active);
    let editor_focused = production_text_editor_style(&theme, ControlVisualState::Focused);
    assert_eq!(editor_active.border.radius, editor_focused.border.radius);
    assert_eq!(
        editor_focused.border.color,
        crate::theme::focus_ring_color(&theme, false)
    );
    let (start, end) = crate::ui::spinner::arc_span(0.25);
    assert!((end - start - 1.5 * std::f32::consts::PI).abs() < 0.001);
    assert!(crate::ui::spinner::arc_span(0.75).1 > end);
    let _ = (
        pill(&theme, FocusTarget::EndTask, "pill", false, Message::Tick),
        production_multi_line_text_input(&theme, &text_editor::Content::with_text("line")),
        text_input_style(&theme, ControlRole::Input, ControlVisualState::Active),
        production_text_editor_style(&theme, editor_paint_state(text_editor::Status::Active)),
        checkbox_style(&theme, ControlVisualState::Active, true),
        checkbox_style(&theme, ControlVisualState::Active, false),
        checked_glass_background(&theme),
        button_style(
            &theme,
            ControlRole::Button,
            ButtonTone::Secondary,
            ControlVisualState::Pressed,
        ),
        spinner(&theme, Some(0.25)),
        crate::ui::spinner::arc_span(0.0),
        crate::ui::spinner::arc_span(0.5),
        divider(&theme),
    );
}

/// Every native toolkit status the control family consumes resolves through
/// the shared interaction fold — there is no local status→ink table left in
/// this frontend. The verdicts are the shared ones, so a hovered button, a
/// disabled checkbox and a focused field read exactly like their GPUI twins.
#[test]
fn every_control_status_resolves_through_the_shared_fold() {
    use super::state::{checkbox_ink, text_input_ink};

    assert_eq!(
        button_ink(button::Status::Active),
        ControlVisualState::Active
    );
    assert_eq!(
        button_ink(button::Status::Hovered),
        ControlVisualState::Hovered
    );
    assert_eq!(
        button_ink(button::Status::Pressed),
        ControlVisualState::Pressed
    );
    assert_eq!(
        button_ink(button::Status::Disabled),
        ControlVisualState::Disabled
    );
    assert_eq!(
        checkbox_ink(iced::widget::checkbox::Status::Hovered { is_checked: false }),
        ControlVisualState::Hovered
    );
    assert_eq!(
        checkbox_ink(iced::widget::checkbox::Status::Disabled { is_checked: false }),
        ControlVisualState::Disabled
    );
    assert_eq!(
        text_input_ink(iced::widget::text_input::Status::Focused { is_hovered: true }),
        ControlVisualState::Focused,
        "a focused field outranks the pointer resting on it"
    );
    assert_eq!(
        text_editor_ink(text_editor::Status::Hovered),
        ControlVisualState::Hovered
    );
}

/// Native checkbox pointer and focused keyboard paths publish the same typed
/// transition, while a disabled checkbox publishes neither.
#[test]
fn checkbox_pointer_keyboard_and_disabled_paths_publish_typed_messages() {
    let theme = theme();
    let renderer = renderer();
    let size = Size::new(420.0, 120.0);
    let viewport = Rectangle::with_size(size);
    let target = FocusTarget::EndTask;
    let label = with_language(Language::En, || "Show columns");

    let mut element: TestElement<'_> = check_box(
        &theme,
        target,
        true,
        label,
        |checked| {
            Message::SelectProcessStatusFilter(if checked {
                ProcessStatusFilter::Running
            } else {
                ProcessStatusFilter::All
            })
        },
        false,
    );
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    let point = Point::new(node.bounds().center_x(), node.bounds().center_y());
    let pointer = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(point),
    );
    assert!(pointer.iter().any(|message| matches!(
        message,
        Message::SelectProcessStatusFilter(ProcessStatusFilter::All)
    )));

    assert!(focus(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        Id::from(focus_id(target)),
    ));
    let keyboard = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(Named::Space, Code::Space),
        mouse::Cursor::Unavailable,
    );
    assert!(keyboard.iter().any(|message| matches!(
        message,
        Message::SelectProcessStatusFilter(ProcessStatusFilter::All)
    )));

    let mut disabled: TestElement<'_> =
        check_box(&theme, target, true, label, |_| Message::CloseSearch, true);
    let mut disabled_tree = Tree::new(&disabled);
    let disabled_node = layout(&mut disabled, &mut disabled_tree, &renderer, size);
    let disabled_messages = dispatch(
        &mut disabled,
        &mut disabled_tree,
        &disabled_node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(
        !disabled_messages
            .iter()
            .any(|message| matches!(message, Message::CloseSearch))
    );
}

/// Real widget trees for text inputs, icon buttons, layout primitives and the
/// single spinner owner construct and lay out inside a compact slot.
#[test]
fn control_family_builds_within_a_compact_measured_viewport() {
    let theme = theme();
    let renderer = renderer();
    let size = Size::new(420.0, 240.0);
    let content = text_editor::Content::with_text("first\nsecond");
    let controls: Vec<TestElement<'_>> = vec![
        plain_text_input(
            &theme,
            None,
            "single",
            "",
            Message::SearchChanged,
            Some(Message::SubmitRunTask),
            false,
        ),
        production_multi_line_text_input(&theme, &content),
        icon_button(
            &theme,
            FocusTarget::SearchClose,
            IconId::Close,
            "Close",
            Message::CloseSearch,
            true,
        ),
        section_header(&theme, "Section"),
        label(&theme, "Caption", true),
        toolbar(vec![
            glass_button(
                &theme,
                FocusTarget::EndTask,
                "Run",
                Message::Tick,
                ButtonTone::Secondary,
                true,
            ),
            pill(
                &theme,
                FocusTarget::ProcessColumnsReset,
                "Pill",
                false,
                Message::Tick,
            ),
        ]),
        spinner(&theme, Some(0.5)),
    ];
    let mut element: TestElement<'_> = iced::widget::column(controls).spacing(4.0).into();
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    let bounds = node.bounds();
    assert!(bounds.width <= size.width + 0.001);
    assert!(bounds.x >= -0.001 && bounds.y >= -0.001);
    assert!(bounds.x + bounds.width <= size.width + 0.001);
}

/// A disabled shared button drops the focus stop and the activation, while
/// the measured extent stays on the shared role matrix: the availability
/// state changes paint, never geometry.
#[test]
fn a_disabled_shared_button_publishes_nothing_and_keeps_the_shared_extent() {
    let theme = theme();
    let renderer = renderer();
    let size = Size::new(320.0, 96.0);
    let viewport = Rectangle::with_size(size);
    let target = FocusTarget::EndTask;

    let mut enabled: TestElement<'_> = glass_button(
        &theme,
        target,
        "Run",
        Message::CloseSearch,
        ButtonTone::Primary,
        true,
    );
    let mut enabled_tree = Tree::new(&enabled);
    let enabled_node = layout(&mut enabled, &mut enabled_tree, &renderer, size);

    let mut disabled: TestElement<'_> = glass_button(
        &theme,
        target,
        "Run",
        Message::CloseSearch,
        ButtonTone::Primary,
        false,
    );
    let mut disabled_tree = Tree::new(&disabled);
    let disabled_node = layout(&mut disabled, &mut disabled_tree, &renderer, size);

    assert_eq!(enabled_node.bounds().height, disabled_node.bounds().height);
    assert_eq!(
        disabled_node.bounds().height,
        crate::theme::control_extent(&theme, ControlRole::Button)
    );

    assert!(focus(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        Id::from(focus_id(target)),
    ));
    let published = dispatch(
        &mut enabled,
        &mut enabled_tree,
        &enabled_node,
        &renderer,
        &viewport,
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(
        published
            .iter()
            .any(|message| matches!(message, Message::CloseSearch))
    );

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
        key_pressed(Named::Enter, Code::Enter),
        mouse::Cursor::Unavailable,
    );
    assert!(
        !silent
            .iter()
            .any(|message| matches!(message, Message::CloseSearch)),
        "a disabled control must publish no activation"
    );
}

#[test]
fn focusable_tooltips_follow_keyboard_focus_and_pointer_hover() {
    let renderer = renderer();
    let size = Size::new(180.0, 80.0);
    let viewport = Rectangle::with_size(size);
    let target = FocusTarget::SettingsTrigger;
    let keyboard_theme = theme();
    let mut keyboard_hint: TestElement<'_> = icon_button(
        &keyboard_theme,
        target,
        IconId::Settings,
        "Settings",
        Message::OpenSettings,
        true,
    );
    let mut keyboard_tree = Tree::new(&keyboard_hint);
    let keyboard_node = layout(&mut keyboard_hint, &mut keyboard_tree, &renderer, size);
    assert!(
        keyboard_hint
            .as_widget_mut()
            .overlay(
                &mut keyboard_tree,
                Layout::new(&keyboard_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "the hint starts hidden"
    );
    assert!(focus(
        &mut keyboard_hint,
        &mut keyboard_tree,
        &keyboard_node,
        &renderer,
        Id::from(focus_id(target)),
    ));
    assert!(
        keyboard_hint
            .as_widget_mut()
            .overlay(
                &mut keyboard_tree,
                Layout::new(&keyboard_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_some(),
        "keyboard-visible focus on a descendant opens its hint"
    );
    dispatch(
        &mut keyboard_hint,
        &mut keyboard_tree,
        &keyboard_node,
        &renderer,
        &viewport,
        key_pressed(Named::Escape, Code::Escape),
        mouse::Cursor::Unavailable,
    );
    assert!(
        keyboard_hint
            .as_widget_mut()
            .overlay(
                &mut keyboard_tree,
                Layout::new(&keyboard_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "Escape dismisses a focus-opened hint"
    );
    dispatch(
        &mut keyboard_hint,
        &mut keyboard_tree,
        &keyboard_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved {
            position: keyboard_node.bounds().center(),
        }),
        mouse::Cursor::Available(keyboard_node.bounds().center()),
    );
    assert!(
        keyboard_hint
            .as_widget_mut()
            .overlay(
                &mut keyboard_tree,
                Layout::new(&keyboard_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "hover does not reopen a focus hint dismissed with Escape"
    );

    let pointer_theme = keyboard_theme.with_focus_visible(false);
    let mut pointer_hint: TestElement<'_> = icon_button(
        &pointer_theme,
        target,
        IconId::Settings,
        "Settings",
        Message::OpenSettings,
        true,
    );
    let mut pointer_tree = Tree::new(&pointer_hint);
    let pointer_node = layout(&mut pointer_hint, &mut pointer_tree, &renderer, size);
    assert!(focus(
        &mut pointer_hint,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        Id::from(focus_id(target)),
    ));
    assert!(
        pointer_hint
            .as_widget_mut()
            .overlay(
                &mut pointer_tree,
                Layout::new(&pointer_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "pointer or programmatic focus alone does not open a keyboard hint"
    );
    let pointer = pointer_node.bounds().center();
    dispatch(
        &mut pointer_hint,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        pointer_hint
            .as_widget_mut()
            .overlay(
                &mut pointer_tree,
                Layout::new(&pointer_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "pointer press suppresses the hover hint on the focused control"
    );
    dispatch(
        &mut pointer_hint,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        pointer_hint
            .as_widget_mut()
            .overlay(
                &mut pointer_tree,
                Layout::new(&pointer_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_none(),
        "the dismissed hint stays closed until the pointer leaves"
    );
    let outside = Point::new(pointer.x + 200.0, pointer.y);
    dispatch(
        &mut pointer_hint,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        mouse::Cursor::Available(outside),
    );
    dispatch(
        &mut pointer_hint,
        &mut pointer_tree,
        &pointer_node,
        &renderer,
        &viewport,
        Event::Mouse(mouse::Event::CursorMoved { position: pointer }),
        mouse::Cursor::Available(pointer),
    );
    assert!(
        pointer_hint
            .as_widget_mut()
            .overlay(
                &mut pointer_tree,
                Layout::new(&pointer_node),
                &renderer,
                &viewport,
                iced::Vector::ZERO,
            )
            .is_some(),
        "pointer hover still opens the hint"
    );
}

/// The shared progress bar keeps the shared bar height and clamps its ratio;
/// unavailable data stays the unknown grammar instead of a fabricated zero.
#[test]
fn the_shared_progress_bar_keeps_the_shared_geometry_and_the_clamped_ratio() {
    let theme = theme();
    let renderer = renderer();
    let size = Size::new(200.0, 60.0);
    let mut element: TestElement<'_> = crate::ui::components::progress(
        &theme,
        Some(1.6),
        crate::ui::components::BadgeTone::Accent,
    );
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    assert_eq!(node.bounds().height, f32::from(SPACE_6));
    assert_eq!(
        crate::ui::components::progress_fill(Some(1.6)),
        crate::ui::components::ProgressFill::Determinate(1.0)
    );
    assert_eq!(
        crate::ui::components::progress_fill(Some(f32::NAN)),
        crate::ui::components::ProgressFill::Unknown
    );
    assert_eq!(
        crate::ui::components::progress_fill(None),
        crate::ui::components::ProgressFill::Unknown
    );
}

/// The bounded toolbar keeps every action group's intrinsic width and gives
/// the overflow a real horizontal viewport instead of shrinking controls
/// into half-painted fragments at the right edge.
#[test]
fn a_bounded_toolbar_keeps_complete_action_groups_and_one_scroll_owner() {
    let theme = theme();
    let renderer = renderer();
    let size = Size::new(320.0, 80.0);
    let actions: Vec<TestElement<'_>> = [
        "End task",
        "Force kill",
        "Open file location",
        "Search online",
        "Suspend",
        "Resume",
        "Affinity",
        "Efficiency mode",
        "Choose columns",
        "Run new task",
        "Set priority…",
        "Delete confirms",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, label)| {
        glass_button(
            &theme,
            FocusTarget::TableRow {
                page: AppPage::Applications,
                index,
            },
            label,
            Message::Tick,
            ButtonTone::Secondary,
            true,
        )
    })
    .collect();
    let mut element: TestElement<'_> =
        crate::ui::components::bounded_toolbar(&theme, ControlRole::Button, actions);
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    assert_eq!(
        node.bounds().height,
        crate::theme::control_extent(&theme, ControlRole::Button),
        "a button-only strip uses the shared Button extent without Input padding"
    );
    let mut operation = StripBounds::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut operation);

    assert_eq!(operation.scroll.len(), 1);
    let (_, viewport, content) = operation
        .scroll
        .first()
        .map(|(id, viewport, content)| (id.clone(), *viewport, *content))
        .expect("the toolbar must register one horizontal scroll owner");
    assert!(viewport.width <= size.width + 0.001);
    assert!(content.width > viewport.width);
    assert_eq!(viewport.height, node.bounds().height);
    assert!(
        operation
            .focus
            .iter()
            .all(|(_, bounds)| bounds.width > 0.0 && bounds.height > 0.0),
        "every complete action group must remain measurable: {:?}",
        operation.focus
    );
    assert!(
        operation
            .focus
            .iter()
            .any(|(_, bounds)| bounds.x + bounds.width > viewport.width),
        "the later action groups must live inside the horizontal content, not be shrunk: {:?}",
        operation.focus
    );
}

#[test]
fn a_bounded_toolbar_exposes_one_typed_scroll_owner_per_role() {
    let theme = theme();
    let renderer = renderer();
    let size = Size::new(320.0, 80.0);
    for role in [
        ControlRole::Button,
        ControlRole::IconButton,
        ControlRole::Input,
    ] {
        let actions: Vec<TestElement<'_>> = (0..24)
            .map(|index| {
                glass_button(
                    &theme,
                    FocusTarget::TableRow {
                        page: AppPage::Applications,
                        index,
                    },
                    format!("Action {index}"),
                    Message::Tick,
                    ButtonTone::Secondary,
                    true,
                )
            })
            .collect();
        let mut element: TestElement<'_> =
            crate::ui::components::bounded_toolbar(&theme, role, actions);
        let mut tree = Tree::new(&element);
        let node = layout(&mut element, &mut tree, &renderer, size);
        let mut operation = StripBounds::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut operation);
        assert_eq!(operation.scroll.len(), 1);
        let (_, viewport, content) = operation
            .scroll
            .first()
            .map(|(id, viewport, content)| (id.clone(), *viewport, *content))
            .expect("the bounded toolbar registers its typed scroll owner");
        assert_eq!(
            node.bounds().height,
            crate::theme::control_extent(&theme, role)
        );
        assert!(viewport.width <= size.width + 0.001);
        assert!(content.width > viewport.width);
    }
}

#[test]
fn a_bounded_toolbar_spends_only_its_declared_control_extent() {
    let theme = theme();
    let renderer = renderer();
    for role in [
        ControlRole::Button,
        ControlRole::IconButton,
        ControlRole::Input,
    ] {
        let mut element: TestElement<'_> =
            crate::ui::components::bounded_toolbar(&theme, role, Vec::new());
        let mut tree = Tree::new(&element);
        let node = layout(&mut element, &mut tree, &renderer, Size::new(320.0, 80.0));
        assert_eq!(
            node.bounds().height,
            crate::theme::control_extent(&theme, role),
            "the {role:?} strip must not add internal control padding to its slot"
        );
    }
}
