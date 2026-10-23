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
use taskmanager_theme::{HighContrast, LightDark, Skin};

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

fn apply(app: &mut crate::IcedApp, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
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
