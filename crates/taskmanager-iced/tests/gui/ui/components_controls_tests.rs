// test-intent: behavior
//
// Behavior tests for the shared Iced control family: real widget trees,
// typed messages, resolved geometry, and the shared glass/focus-ring seams.

use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Tree};
use iced::keyboard::key::{Code, Key, Named, Physical};
use iced::widget::text_editor;
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use taskmanager_application::i18n::{Language, with_language};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_theme::tokens::RowDensity;
use taskmanager_theme::{HighContrast, LightDark, ResolvedFonts, Skin, Theme};
use taskmanager_ui_contract::IconId;
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

use crate::Message;
use crate::app::FocusTarget;
use crate::focus::focus_id;
use crate::theme::{ControlVisualState, control_glass};
use crate::ui::components::divider;

use super::activation::{checkbox_style, checked_glass_background};
use super::state::{ControlPaintState, button_style};
use super::text::text_input_style;
use super::*;

type TestElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

/// The multi-line editor's local control. The product has no multi-line text
/// surface (the GPUI reference input is single-line too), so the shape lives
/// here instead of the production component tree — but it is built from the
/// SAME shared material and geometry seams, so the multi-line shape keeps
/// the shared contract proven.
fn multi_line_text_input<'a>(
    theme_snapshot: &'a Theme,
    content: &'a text_editor::Content,
    placeholder: &'a str,
) -> TestElement<'a> {
    let extent = crate::theme::control_extent(theme_snapshot, ControlRole::Input);
    let padding_x = crate::theme::control_padding_x(theme_snapshot, ControlRole::Input);
    text_editor::TextEditor::new(content)
        .placeholder(placeholder)
        .padding([4.0, padding_x])
        .size(13.0)
        .height(iced::Length::Fixed(extent * 3.0))
        .style(move |_, status| text_editor_style(theme_snapshot, editor_paint_state(status)))
        .into()
}

/// The multi-line editor's paint seam: the shared Input-role glass with the
/// same keyboard-only focus ring the production single-line field reads.
fn text_editor_style(theme: &Theme, state: ControlPaintState) -> text_editor::Style {
    let glass = control_glass(theme, ControlRole::Input, state.visual());
    let palette = theme.palette();
    let ink = glass
        .text_color
        .unwrap_or_else(|| crate::theme_binding::color(palette.fg));
    text_editor::Style {
        background: glass.background.unwrap_or_else(|| {
            iced::Background::Color(crate::theme_binding::color(palette.surface))
        }),
        border: glass.border,
        placeholder: ink,
        value: ink,
        selection: crate::theme_binding::color(palette.selection),
    }
}

fn editor_paint_state(status: text_editor::Status) -> ControlPaintState {
    match status {
        text_editor::Status::Active => ControlPaintState::Active,
        text_editor::Status::Hovered => ControlPaintState::Hovered,
        text_editor::Status::Focused { .. } => ControlPaintState::Focused,
        text_editor::Status::Disabled => ControlPaintState::Disabled,
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
    let checked = checkbox_style(&theme, ControlPaintState::Active, true);
    let unchecked = checkbox_style(&theme, ControlPaintState::Active, false);
    assert_ne!(checked.background, unchecked.background);
    assert_eq!(checked.border, unchecked.border);
    let active = button_style(
        &theme,
        ControlRole::Button,
        ButtonTone::Secondary,
        ControlPaintState::Active,
    );
    let pressed = button_style(
        &theme,
        ControlRole::Button,
        ButtonTone::Secondary,
        ControlPaintState::Pressed,
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
    let input_active = text_input_style(&theme, ControlRole::Input, ControlPaintState::Active);
    let input_focused = text_input_style(&theme, ControlRole::Input, ControlPaintState::Focused);
    assert_eq!(input_active.border.radius, input_focused.border.radius);
    assert_eq!(
        input_focused.border.color,
        crate::theme::focus_ring_color(&theme, false)
    );
    let editor_active = text_editor_style(&theme, ControlPaintState::Active);
    let editor_focused = text_editor_style(&theme, ControlPaintState::Focused);
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
        multi_line_text_input(&theme, &text_editor::Content::with_text("line"), "hint"),
        text_input_style(&theme, ControlRole::Input, ControlPaintState::Active),
        text_editor_style(&theme, editor_paint_state(text_editor::Status::Active)),
        checkbox_style(&theme, ControlPaintState::Active, true),
        checkbox_style(&theme, ControlPaintState::Active, false),
        checked_glass_background(&theme),
        button_style(
            &theme,
            ControlRole::Button,
            ButtonTone::Secondary,
            ControlPaintState::Pressed,
        ),
        spinner(&theme, Some(0.25)),
        crate::ui::spinner::arc_span(0.0),
        crate::ui::spinner::arc_span(0.5),
        divider(&theme),
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
            Message::UpdateRunTaskCommand,
            Some(Message::SubmitRunTask),
            false,
        ),
        multi_line_text_input(&theme, &content, "multi"),
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
