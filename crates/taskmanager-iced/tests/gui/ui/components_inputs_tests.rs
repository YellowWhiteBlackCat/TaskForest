// test-intent: behavior
//
// Behavior tests for the inputs component family: real SegmentedTrack key
// events, state/token mapping seams (step snapping, track fills, clear
// gating), and construction across skins, modes, and degenerate states.

use super::*;
use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{Id, Tree};
use iced::{Element, Event, Rectangle, Size, mouse};
use taskmanager_assets::embedded_fonts;
use taskmanager_theme::ResolvedFonts;
use taskmanager_theme::tokens::{RowDensity, UiSize};
use taskmanager_theme::{GlassSurfaceRole, HighContrast, LightDark, Skin};
use taskmanager_ui_contract::presentation::control_ink::ControlInkState;
use taskmanager_ui_contract::presentation::sizes::{ControlRole, ControlState};

type TestElement<'a> = Element<'a, Message, iced::Theme, iced::Renderer>;

fn theme_for(skin: Skin, mode: LightDark) -> Theme {
    Theme::build(
        skin,
        mode,
        HighContrast::Off,
        ResolvedFonts::system_for(skin),
    )
}

fn target() -> FocusTarget {
    FocusTarget::StartupControl
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
    focused: bool,
}

impl Operation<()> for FocusById {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<()>)) {
        visit(self);
    }

    fn focusable(&mut self, id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            state.focus();
            self.focused = state.is_focused();
        }
    }
}

fn focus(
    element: &mut TestElement<'_>,
    tree: &mut Tree,
    node: &Node,
    renderer: &iced::Renderer,
    target: FocusTarget,
) -> bool {
    let mut operation = FocusById {
        target: Id::from(crate::focus::focus_id(target)),
        focused: false,
    };
    element
        .as_widget_mut()
        .operate(tree, Layout::new(node), renderer, &mut operation);
    operation.focused
}

fn key_pressed(named: iced::keyboard::key::Named, code: iced::keyboard::key::Code) -> Event {
    let key = iced::keyboard::key::Key::Named(named);
    Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: iced::keyboard::key::Physical::Code(code),
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
) -> Vec<Message> {
    let mut messages = Vec::new();
    {
        let mut shell = Shell::new(&mut messages);
        let mut clipboard = iced::advanced::clipboard::Null;
        element.as_widget_mut().update(
            tree,
            &event,
            Layout::new(node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut clipboard,
            &mut shell,
            viewport,
        );
    }
    messages
}

/// Every Iced input status folds through the shared interaction-ink verdict:
/// a disabled status wins over hover/press, a dragged slider stays pressed,
/// and an opened select keeps the focused ink even while hovered.
#[test]
fn input_status_folds_through_shared_control_ink() {
    use crate::ui::components::controls::state::{
        button_ink, pick_list_ink, slider_ink, text_input_ink,
    };

    let theme = theme_for(Skin::Gnome, LightDark::Dark).with_focus_visible(true);
    assert_eq!(
        button_ink(button::Status::Disabled),
        ControlVisualState::Disabled
    );
    let active_rail = slider_style(&theme, iced::widget::slider::Status::Active).rail;
    let dragged_rail = slider_style(&theme, iced::widget::slider::Status::Dragged).rail;
    assert_ne!(
        dragged_rail.backgrounds.1, active_rail.backgrounds.1,
        "a dragged slider must publish its resolved pressed ink"
    );
    assert_eq!(
        slider_ink(iced::widget::slider::Status::Dragged),
        ControlVisualState::Pressed
    );
    assert_eq!(
        pick_list_ink(iced::widget::pick_list::Status::Opened { is_hovered: true }),
        ControlVisualState::Focused
    );
    assert_eq!(
        text_input_ink(iced::widget::text_input::Status::Focused { is_hovered: true }),
        ControlVisualState::Focused
    );
    assert_eq!(
        ControlInkState::Focused.changes_measurement(),
        false,
        "control ink only reweights paint"
    );
    assert_ne!(
        switch_track_fill(&theme, true, button::Status::Disabled),
        switch_track_fill(&theme, true, button::Status::Hovered),
        "a disabled switch must not keep the hover ink"
    );
    assert_eq!(
        select_style(
            &theme,
            iced::widget::pick_list::Status::Opened { is_hovered: true },
        )
        .border
        .color,
        crate::theme::focus_ring_color(&theme.with_focus_visible(true), false),
        "an opened select must keep the focused ink over hover"
    );
}

/// The segmented segment resolves the shared SELECTION-DOMINANT fold, the
/// same verdict the GPUI segmented control resolves: the active segment
/// paints the accent as its own fill and keeps it while the pointer rests
/// on it, while an idle segment still wears the hover tint. The visible
/// pixels are the ones this frontend painted before the rule moved into the
/// shared fold.
#[test]
fn segmented_segments_resolve_the_selection_dominant_fold() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let palette = theme.palette();

    let active = segment_style(&theme, 0, 3, true, button::Status::Active);
    let active_hovered = segment_style(&theme, 0, 3, true, button::Status::Hovered);
    let idle_hovered = segment_style(&theme, 1, 3, false, button::Status::Hovered);
    let idle = segment_style(&theme, 1, 3, false, button::Status::Active);

    let Some(Background::Color(active_fill)) = active.background else {
        panic!("the active segment paints a solid fill");
    };
    let Some(Background::Color(active_hovered_fill)) = active_hovered.background else {
        panic!("the hovered active segment paints a solid fill");
    };
    assert_eq!(
        active_fill,
        crate::theme_binding::color(palette.accent),
        "the active segment wears the accent"
    );
    assert_eq!(
        active_hovered_fill, active_fill,
        "a pointer resting on the active segment must not erase the accent"
    );
    let Some(Background::Color(idle_hovered_fill)) = idle_hovered.background else {
        panic!("the hovered idle segment paints a solid fill");
    };
    assert_eq!(
        idle_hovered_fill,
        crate::theme_binding::color(palette.hover),
        "an idle segment still wears the shared hover ink"
    );
    let Some(Background::Color(idle_fill)) = idle.background else {
        panic!("the idle segment paints a fill");
    };
    assert_ne!(
        idle_fill, idle_hovered_fill,
        "the idle segment's hover ink must differ from its resting fill"
    );
}

fn apply(app: &mut crate::IcedApp, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

/// The shared select publishes the typed option message when the focused
/// trigger confirms the current selection, while the degenerate empty option
/// list stays inert: a placeholder surface, never a fabricated value.
#[test]
fn select_confirms_the_typed_option_message_and_keeps_the_empty_list_inert() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let renderer = renderer();
    let size = Size::new(240.0, 80.0);
    let viewport = Rectangle::with_size(size);
    let options = vec!["High".to_string(), "Low".to_string()];

    let mut element: TestElement<'_> = select(
        &theme,
        target(),
        &options,
        Some(&options[0]),
        "pick one",
        |option: &String| Message::ServicesSearchChanged(option.clone()),
    );
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    assert!(focus(&mut element, &mut tree, &node, &renderer, target()));
    let confirmed = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(
            iced::keyboard::key::Named::Enter,
            iced::keyboard::key::Code::Enter,
        ),
    );
    assert!(confirmed.iter().any(|message| matches!(
        message,
        Message::ServicesSearchChanged(value) if value == "High"
    )));

    let empty: Vec<String> = Vec::new();
    let mut inert: TestElement<'_> = select(&theme, target(), &empty, None, "pick one", |_| {
        Message::Tick
    });
    let mut inert_tree = Tree::new(&inert);
    let inert_node = layout(&mut inert, &mut inert_tree, &renderer, size);
    let inert_messages = dispatch(
        &mut inert,
        &mut inert_tree,
        &inert_node,
        &renderer,
        &viewport,
        key_pressed(
            iced::keyboard::key::Named::Enter,
            iced::keyboard::key::Code::Enter,
        ),
    );
    assert!(
        !inert_messages
            .iter()
            .any(|message| matches!(message, Message::Tick)),
        "an empty option list must stay inert instead of inventing a value"
    );
}

/// Arrow navigation wraps past both ends, never treats an unknown active
/// value as a position, and refuses to move in a choice-less or
/// single-choice group.
#[test]
fn segmented_keyboard_navigation_wraps_and_degrades() {
    let choices = [
        ("Slow".to_string(), 0),
        ("Normal".to_string(), 1),
        ("Fast".to_string(), 2),
    ];
    assert_eq!(segmented_active_index(&choices, 1), Some(1));
    assert_eq!(segmented_neighbor_index(&choices, 1, true), Some(2));
    assert_eq!(segmented_neighbor_index(&choices, 1, false), Some(0));
    assert_eq!(segmented_neighbor_index(&choices, 2, true), Some(0));
    assert_eq!(segmented_neighbor_index(&choices, 0, false), Some(2));
    assert_eq!(segmented_active_index(&choices, 42), None);
    assert_eq!(segmented_neighbor_index(&choices, 42, true), None);
    let single = [("Only".to_string(), 7)];
    assert_eq!(segmented_active_index(&single, 7), Some(0));
    assert_eq!(segmented_neighbor_index(&single, 7, false), None);
    assert_eq!(segmented_active_index(&[], 0), None);
}

/// A focused production SegmentedTrack turns Space into its active typed
/// value and Right into the next typed value, then the Iced app reducer stores
/// the emitted values in the graph-points configuration.
#[test]
fn segmented_track_keyboard_events_publish_and_apply_the_selected_value() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let target = target();
    let choices = vec![("Low".to_owned(), 10), ("High".to_owned(), 20)];
    let renderer = renderer();
    let size = Size::new(320.0, 60.0);
    let viewport = Rectangle::with_size(size);
    let mut app = crate::IcedApp::demo();

    let mut element = segmented(&theme, target, &choices, 10, |value| {
        Message::SelectPerformanceGraphPoints(
            u32::try_from(value).expect("test choice fits the graph-point count"),
        )
    });
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    assert!(focus(&mut element, &mut tree, &node, &renderer, target));

    let reaffirmed = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(
            iced::keyboard::key::Named::Space,
            iced::keyboard::key::Code::Space,
        ),
    );
    assert!(
        reaffirmed
            .iter()
            .any(|message| matches!(message, Message::Focus(candidate) if *candidate == target))
    );
    assert!(
        reaffirmed
            .iter()
            .any(|message| matches!(message, Message::SelectPerformanceGraphPoints(10)))
    );
    drop(element);
    apply(&mut app, reaffirmed);
    assert_eq!(app.config_draft().graph_data_points, 10);

    let mut element = segmented(&theme, target, &choices, 10, |value| {
        Message::SelectPerformanceGraphPoints(
            u32::try_from(value).expect("test choice fits the graph-point count"),
        )
    });
    let mut tree = Tree::new(&element);
    let node = layout(&mut element, &mut tree, &renderer, size);
    assert!(focus(&mut element, &mut tree, &node, &renderer, target));
    let selected = dispatch(
        &mut element,
        &mut tree,
        &node,
        &renderer,
        &viewport,
        key_pressed(
            iced::keyboard::key::Named::ArrowRight,
            iced::keyboard::key::Code::ArrowRight,
        ),
    );
    assert!(
        selected
            .iter()
            .any(|message| matches!(message, Message::SelectPerformanceGraphPoints(20)))
    );
    drop(element);
    apply(&mut app, selected);
    assert_eq!(app.config_draft().graph_data_points, 20);
}

/// Slider values clamp to the range, snap onto the step grid anchored at
/// the range start, and degrade safely for zero/negative/NaN steps and
/// unordered ranges (never a division by zero or an out-of-range value).
#[test]
fn slider_values_clamp_and_snap_to_the_step_grid() {
    let range = 0.0..=10.0_f32;
    assert_eq!(slider_snapped_value(&range, 3.0, 5.2), 6.0);
    assert_eq!(slider_snapped_value(&range, 3.0, 4.2), 3.0);
    assert_eq!(slider_snapped_value(&range, 3.0, 12.0), 10.0);
    assert_eq!(slider_snapped_value(&range, 3.0, -4.0), 0.0);
    assert_eq!(slider_snapped_value(&range, 0.0, 4.7), 4.7);
    assert_eq!(slider_snapped_value(&range, -2.0, 4.7), 4.7);
    assert_eq!(slider_snapped_value(&range, f32::NAN, 4.7), 4.7);
    assert_eq!(slider_snapped_value(&(10.0..=0.0), 1.0, 4.0), 4.0);
    assert_eq!(slider_snapped_value(&(5.0..=5.0), 1.0, 9.0), 5.0);
}

/// The switch track reads its fill off the palette tokens per state, and
/// the disabled status dims toward the backdrop instead of leaving the
/// accent (an unavailable control must not look actionable).
#[test]
fn switch_track_fill_maps_state_onto_palette_tokens() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let palette = theme.palette();
    assert_eq!(
        switch_track_fill(&theme, true, button::Status::Active),
        crate::theme_binding::color(palette.accent)
    );
    assert_eq!(
        switch_track_fill(&theme, false, button::Status::Active),
        crate::theme_binding::color(palette.border)
    );
    let disabled = switch_track_fill(&theme, true, button::Status::Disabled);
    assert_ne!(disabled, crate::theme_binding::color(palette.accent));
    assert_eq!(
        disabled,
        crate::theme_binding::color(mix(palette.accent, palette.window_backdrop, 0.5))
    );
}

/// Every control constructs without I/O across every skin and mode, in its
/// normal and its degenerate shapes (disabled switch, degenerate slider
/// step, empty/unselected select, empty/valued search query, unknown-active
/// and empty segmented groups).
#[test]
fn input_controls_build_across_skins_modes_and_degenerate_states() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark, LightDark::EyeForest] {
            let theme = theme_for(skin, mode);

            let _ = switch(theme, target(), true, false, Message::RequestStartupControl);
            let _ = switch(
                theme,
                target(),
                false,
                false,
                Message::RequestStartupControl,
            );
            let _ = switch(theme, target(), true, true, Message::RequestStartupControl);

            let _ = slider(
                &theme,
                target(),
                0.0..=100.0,
                5.0,
                42.0,
                |value| Message::RequestStartupControl(value > 50.0),
                Some(|value| format!("{value:.0}%")),
            );
            let _ = slider(
                &theme,
                target(),
                0.0..=100.0,
                0.0,
                42.0,
                |_| Message::Tick,
                None::<fn(f32) -> String>,
            );

            let options = vec!["Five seconds".to_string(), "Sixty seconds".to_string()];
            let _ = select(
                &theme,
                target(),
                &options,
                Some(&options[0]),
                "pick one",
                |_| Message::Tick,
            );
            let _ = select(&theme, target(), &options, None, "pick one", |_| {
                Message::Tick
            });
            let empty: Vec<String> = Vec::new();
            let _ = select(&theme, target(), &empty, None, "pick one", |_| {
                Message::Tick
            });

            let _ = search_input(
                &theme,
                target(),
                "Search…",
                "",
                Message::ServicesSearchChanged,
            );
            let _ = search_input(
                &theme,
                target(),
                "Search…",
                "disk",
                Message::ServicesSearchChanged,
            );
            assert!(!search_shows_clear(""));
            assert!(search_shows_clear("disk"));

            let choices = vec![("Low".to_string(), 0), ("High".to_string(), 1)];
            let _ = segmented(&theme, target(), &choices, 1, |value| {
                Message::SelectPerformanceGraphPoints(value as u32)
            });
            let _ = segmented(&theme, target(), &choices, 42, |_| Message::Tick);
            let _ = segmented(&theme, target(), &[], 0, |_| Message::Tick);
        }
    }
}

/// Input, select and segmented styles resolve through the shared control
/// glass: their fills are the theme's own glass gradient, their borders the
/// shared specular edge, and hover/pressed/disabled/focused are distinct
/// token-derived states rather than a second local palette.
#[test]
fn input_styles_consume_the_shared_control_glass() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark).with_focus_visible(true);
    let active = crate::theme::control_glass(
        &theme,
        ControlRole::Input,
        crate::theme::ControlVisualState::Active,
    );
    let select_active = select_style(&theme, iced::widget::pick_list::Status::Active);
    let select_hovered = select_style(&theme, iced::widget::pick_list::Status::Hovered);
    let select_open = select_style(
        &theme,
        iced::widget::pick_list::Status::Opened { is_hovered: false },
    );
    let search_active = search_style(&theme, iced::widget::text_input::Status::Active);
    let search_focused = search_style(
        &theme,
        iced::widget::text_input::Status::Focused { is_hovered: false },
    );
    let search_disabled = search_style(&theme, iced::widget::text_input::Status::Disabled);

    assert_eq!(
        select_active.background,
        active.background.expect("glass fill")
    );
    assert_eq!(select_active.border, active.border);
    assert_ne!(select_hovered.background, select_active.background);
    assert_eq!(
        select_open.border.color,
        crate::theme::focus_ring_color(&theme, false)
    );
    assert_ne!(select_open.border.width, select_active.border.width);
    assert_eq!(
        search_active.background,
        active.background.expect("glass fill")
    );
    assert_eq!(
        search_focused.border.color,
        crate::theme::focus_ring_color(&theme, false)
    );
    assert_eq!(
        search_disabled.value,
        crate::theme_binding::color(theme.palette().fg_muted)
    );
    assert_eq!(
        search_disabled.icon,
        crate::theme_binding::color(theme.palette().fg_muted)
    );
    assert_eq!(
        search_disabled.placeholder,
        crate::theme_binding::color(theme.palette().fg_muted)
    );
}

/// The select's dropdown menu wears the shared `Menu` glass role — the same
/// Card-tier fill, hairline edge, panel radius and elevation the GPUI
/// dropdown paints — and its cursor row spends the shared hover ink with the
/// normal foreground instead of the toolkit's default accent highlight.
#[test]
fn the_select_dropdown_wears_the_shared_menu_role_and_hover_ink() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme_for(skin, mode);
            let palette = theme.palette();
            let menu = select_menu_style(&theme);
            let surface = crate::theme::glass_surface_style(&theme, GlassSurfaceRole::Menu);

            assert_eq!(
                menu.background,
                surface
                    .background
                    .expect("the shared Menu role paints a fill"),
                "{skin:?}/{mode:?}: the dropdown panel spends the shared Menu glass fill"
            );
            assert_eq!(
                menu.border, surface.border,
                "{skin:?}/{mode:?}: the dropdown panel keeps the shared hairline edge and radius"
            );
            assert_eq!(
                menu.shadow, surface.shadow,
                "{skin:?}/{mode:?}: the dropdown panel keeps the shared elevation"
            );
            assert_ne!(
                menu.shadow,
                iced::Shadow::default(),
                "{skin:?}/{mode:?}: a floating menu panel must not lose its elevation"
            );
            assert_eq!(
                menu.selected_background,
                Background::Color(crate::theme_binding::color(palette.hover)),
                "{skin:?}/{mode:?}: the cursor row spends the shared hover ink"
            );
            assert_eq!(
                menu.selected_text_color,
                crate::theme_binding::color(palette.fg),
                "{skin:?}/{mode:?}: the cursor row keeps the normal foreground"
            );
            assert_eq!(
                menu.text_color,
                crate::theme_binding::color(palette.fg),
                "{skin:?}/{mode:?}: the option text keeps the normal foreground"
            );
        }
    }
}

/// The segmented control paints connected glass segments with hairline
/// dividers and rounds only the two OUTER track ends, matching the GPUI
/// connected-track grammar: one glass track, transparent idle segments over
/// it, and the accent fill on the active choice.
#[test]
fn segmented_styles_round_outer_ends_and_paint_token_dividers() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let first = segment_style(&theme, 0, 3, false, button::Status::Active);
    let middle = segment_style(&theme, 1, 3, false, button::Status::Active);
    let last = segment_style(&theme, 2, 3, false, button::Status::Hovered);
    let single = segment_style(&theme, 0, 1, false, button::Status::Active);
    let radius = crate::theme::scaled_radius(&theme, theme.palette().control_radius);
    assert_eq!(
        (first.border.radius.top_left, first.border.radius.top_right),
        (radius, 0.0)
    );
    assert_eq!(
        (
            middle.border.radius.top_left,
            middle.border.radius.top_right
        ),
        (0.0, 0.0)
    );
    assert_eq!(
        (
            last.border.radius.bottom_left,
            last.border.radius.bottom_right
        ),
        (0.0, radius)
    );
    assert_eq!(
        (
            single.border.radius.top_left,
            single.border.radius.bottom_right
        ),
        (radius, radius)
    );
    assert_eq!(
        crate::theme::control_divider_color(&theme),
        crate::theme::card_glass_style(&theme).border.color
    );
    assert_ne!(last.background, middle.background);

    // The active choice keeps the accent fill and the accent foreground; an
    // idle segment stays transparent so the ONE connected track shows
    // through it.
    let active = segment_style(&theme, 1, 3, true, button::Status::Active);
    assert_eq!(
        active.background,
        Some(Background::Color(crate::theme_binding::color(
            theme.palette().accent
        )))
    );
    assert_eq!(
        active.text_color,
        crate::theme_binding::color(theme.accent_text)
    );
    assert_eq!(
        middle.background,
        Some(Background::Color(Color::TRANSPARENT)),
        "an idle segment must not paint its own tile over the track"
    );

    // The track itself is ONE shared glass surface.
    let track = segmented_track_style(&theme);
    assert_eq!(
        track.background,
        crate::theme::card_glass_style(&theme).background
    );
    assert_eq!(
        track.border.color,
        crate::theme::card_glass_style(&theme).border.color
    );
}

/// In compact viewports every control still fits its measured bounds and
/// keeps the shared hit-target floor: focus, hover and disabled change paint,
/// never geometry. The disabled switch remains inert through both input
/// paths (proved by the production interaction test).
#[test]
fn compact_control_bounds_and_states_stay_geometry_invariant() {
    let renderer = renderer();
    let compact = Size::new(360.0, 240.0);
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let choices = vec![("Low".to_string(), 0), ("High".to_string(), 1)];
    let mut controls: Vec<TestElement<'_>> = vec![
        switch(theme, target(), true, false, |_| Message::Tick),
        switch(theme, target(), true, true, |_| Message::Tick),
        slider(
            &theme,
            target(),
            0.0..=10.0,
            1.0,
            4.0,
            |_| Message::Tick,
            None::<fn(f32) -> String>,
        ),
        search_input(&theme, target(), "find", "disk", |_| Message::Tick),
        segmented(&theme, target(), &choices, 0, |_| Message::Tick),
    ];
    for (index, element) in controls.iter_mut().enumerate() {
        let mut tree = Tree::new(&*element);
        let node = layout(element, &mut tree, &renderer, compact);
        let bounds = node.bounds();
        assert!(
            bounds.x >= 0.0 && bounds.y >= 0.0,
            "control {index} stays inside the left/top compact edge"
        );
        assert!(
            bounds.x + bounds.width <= compact.width + 0.5
                && bounds.y + bounds.height <= compact.height + 0.5,
            "control {index} stays inside the right/bottom compact edge"
        );
        assert!(bounds.width > 0.0 && bounds.height > 0.0);
    }
    let measured = crate::presentation_binding::control_measurement(
        ControlRole::Input,
        ControlState::Normal,
        UiSize::Small,
        RowDensity::Compact,
    );
    assert_eq!(measured.extent, measured.realized_extent);
    assert_eq!(measured.padding_x, 12.0);
    assert!(!ControlState::Focused.changes_measurement());
    assert!(!ControlState::Disabled.changes_measurement());
    assert!(!ControlState::Active.changes_measurement());
}

/// The switch knob is a RAISED element inside the track: it wears the shared
/// Card glass contact layer, not a locally numbered shadow, and disabling it
/// keeps the same material with a dimmed fill. The knob's lift is the same
/// token the two-layer shadow projection uses.
#[test]
fn switch_knob_wears_the_shared_contact_shadow() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let enabled = switch_knob_style(&theme, false);
    let disabled = switch_knob_style(&theme, true);
    let contact = crate::theme::glass_contact_shadow(&theme, GlassTier::Card);
    assert_eq!(enabled.shadow, contact);
    assert_eq!(disabled.shadow, contact);
    assert_ne!(enabled.background, disabled.background);
    assert_eq!(enabled.border.radius, (SWITCH_KNOB_SIZE / 2.0).into());
}

/// A focused search field paints the shared focus ring; a disabled one mutes
/// its icon/value/placeholder without inventing a second text palette. The
/// clear affordance keeps the standard status states (hover/pressed/disabled)
/// over the same control glass.
#[test]
fn search_clear_and_disabled_stay_on_the_shared_control_glass() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark).with_focus_visible(true);
    let active = search_clear_style(&theme, button::Status::Active);
    let hovered = search_clear_style(&theme, button::Status::Hovered);
    let pressed = search_clear_style(&theme, button::Status::Pressed);
    let disabled = search_clear_style(&theme, button::Status::Disabled);
    let glass = crate::theme::control_glass(
        &theme,
        ControlRole::IconButton,
        crate::theme::ControlVisualState::Active,
    );
    assert_eq!(active.background, glass.background);
    assert_eq!(active.border, glass.border);
    assert_eq!(active.shadow, glass.shadow);
    assert_ne!(hovered.background, active.background);
    assert!(
        pressed.shadow.blur_radius == active.shadow.blur_radius,
        "the shadow geometry stays the shared projection's; only the ink changes"
    );
    assert!(pressed.shadow.color.a < active.shadow.color.a);
    assert_eq!(
        disabled.text_color,
        crate::theme_binding::color(theme.palette().fg_muted)
    );
}
