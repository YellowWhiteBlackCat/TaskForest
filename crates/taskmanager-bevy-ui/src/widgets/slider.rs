//! The product-owned skin for `bevy_ui_widgets::Slider`.
//!
//! The official widget owns the value, range, drag and keyboard semantics;
//! this module owns only the surface language: a rem-scaled capsule track,
//! accent fill and thumb. Pages compose a typed identity onto the scene and
//! keep their own value changes, but they never fork the geometry or ink.
//!
//! The keyboard binding lives here too, because the slider is the one
//! control that *owns* the keyboard while it holds the focus: the product
//! answers GPUI's key set (the arrows step, Home/End jump to the range
//! ends), the official widget keeps the value authority, and the
//! frontend's key adapter never hands a slider-owned key to the shell —
//! the GPUI `stop_propagation` contract, spelled as one vocabulary here.

use bevy::ecs::event::EntityEvent;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, Query};
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::FocusedInput;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, Node, PositionType, UiRect, Val, percent, px, rem,
};
use bevy::ui_widgets::{
    SetSliderValue, Slider, SliderOrientation, SliderPrecision, SliderRange, SliderStep,
    SliderThumb, SliderValue, SliderValueChange, TrackClick, slider_self_update,
};
use zhuge_ui_core::value::interval_fraction;

use crate::palette::UiPalette;

/// One painted part of a slider's track.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bevy::ecs::component::Component)]
pub(crate) enum SliderPart {
    #[default]
    Fill,
    Thumb,
}

/// A page-scoped owner for the shared slider's painted parts.
#[derive(Clone, Debug, Default, PartialEq, Eq, bevy::ecs::component::Component)]
pub(crate) struct SliderPartOwner(pub(crate) String);

/// A flat range parks the handle at the start of the axis.
const FLAT_HANDLE_AT_START: f32 = 0.0;

#[derive(Clone, Debug)]
pub(crate) struct SliderAxis {
    pub(crate) value: f32,
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) step: f32,
    pub(crate) precision: i32,
    pub(crate) width: Val,
    pub(crate) owner: String,
}

impl SliderAxis {
    #[must_use]
    pub(crate) fn fraction(&self) -> f32 {
        interval_fraction(self.value, self.min, self.max, FLAT_HANDLE_AT_START)
    }

    #[must_use]
    pub(crate) fn filled_percent(&self) -> f32 {
        self.fraction() * 100.0
    }
}

#[must_use]
pub(crate) fn fill_percent(value: f32, min: f32, max: f32) -> f32 {
    interval_fraction(value, min, max, FLAT_HANDLE_AT_START) * 100.0
}

/// The keys a focused slider owns, matching GPUI's slider key set: left and
/// down step down, right and up step up, Home and End jump to the range
/// ends. The official widget answers left/right/home/end; the product
/// answers the vertical axis through the widget's own relative-step entry,
/// so the step size and the range clamp stay the widget's authority.
#[must_use]
pub(crate) fn owns_key(key_code: KeyCode) -> bool {
    matches!(
        key_code,
        KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::ArrowUp
            | KeyCode::ArrowDown
            | KeyCode::Home
            | KeyCode::End
    )
}

/// Step the focused slider on the vertical axis.
///
/// GPUI's slider answers up/down beside left/right; the official widget
/// answers left/right only. The step rides the widget's relative-step
/// entry, so the range clamp and the step size are the widget's authority
/// and never a second copy of that arithmetic.
pub(crate) fn step_on_vertical_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    sliders: Query<(), With<Slider>>,
    mut commands: Commands,
) {
    let (entity, step) = {
        let event = key.event();
        if event.input.state != ButtonState::Pressed
            || event.event_target() != event.focused_entity
            || !sliders.contains(event.focused_entity)
        {
            return;
        }
        let step = match event.input.key_code {
            KeyCode::ArrowUp => 1.0,
            KeyCode::ArrowDown => -1.0,
            _ => return,
        };
        (event.focused_entity, step)
    };
    key.propagate(false);
    commands.trigger(SetSliderValue {
        entity,
        change: SliderValueChange::RelativeStep(step),
    });
}

/// Bind the slider widget's behavior once for the whole frontend: every
/// mounted slider keeps its canonical `SliderValue` in step with the change
/// it emits (so a keyboard step composes from the value the user sees, not
/// from a stale one), and the product answers the vertical axis of GPUI's
/// key set. The write-back used to be a page-local registration, which
/// left every page that had not mounted it with a frozen widget value.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_observer(slider_self_update)
        .add_observer(step_on_vertical_key);
}

/// Paint the official slider with the product's one track language.
pub(crate) fn slider_scene(axis: SliderAxis, palette: &UiPalette) -> impl Scene + use<> {
    let filled = axis.filled_percent();
    let range = SliderRange::new(axis.min, axis.max);
    let diameter_px = palette.control_height_px() / 2.0;
    let track_height_px = diameter_px / 2.0;
    let diameter = rem(palette.rem_ratio(diameter_px));
    let track_height = rem(palette.rem_ratio(track_height_px));
    let half_diameter = rem(palette.rem_ratio(diameter_px / 2.0));
    let track_top = rem(palette.rem_ratio((diameter_px - track_height_px) / 2.0));
    Box::new(bsn! {
        Node {
            width: { axis.width },
            min_width: px(0.0),
            height: diameter,
            position_type: PositionType::Relative,
            margin: UiRect::horizontal(half_diameter),
            flex_shrink: 1.0,
            align_self: bevy::ui::AlignSelf::Center,
        }
        SliderValue({ axis.value })
        SliderRange { ..range }
        SliderStep({ axis.step })
        SliderPrecision({ axis.precision })
        Slider { track_click: TrackClick::Snap, orientation: SliderOrientation::Horizontal }
        Children [
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                right: px(0.0),
                top: track_top,
                height: track_height,
                border_radius: BorderRadius::all(Val::Percent(50.0)),
            }
            BackgroundColor({ palette.panel_fill })
            bevy::picking::Pickable::IGNORE --
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: track_top,
                width: percent(filled),
                height: track_height,
                border_radius: BorderRadius::all(Val::Percent(50.0)),
            }
            BackgroundColor({ palette.accent })
            SliderPart::Fill
            SliderPartOwner({ axis.owner.clone() })
            bevy::picking::Pickable::IGNORE --
            Node {
                position_type: PositionType::Absolute,
                left: percent(filled),
                top: px(0.0),
                width: diameter,
                height: diameter,
                margin: UiRect::left(-half_diameter),
                border_radius: BorderRadius::all(Val::Percent(50.0)),
            }
            BackgroundColor({ palette.accent })
            SliderThumb
            SliderPart::Thumb
            SliderPartOwner({ axis.owner.clone() })
        ]
    })
}
