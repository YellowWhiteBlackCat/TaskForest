//! The product-owned skin for `bevy_ui_widgets::Slider`.
//!
//! The official widget owns the value, range, drag and keyboard semantics;
//! this module owns only the surface language: a rem-scaled capsule track,
//! accent fill and thumb. Pages compose a typed identity onto the scene and
//! keep their own value changes, but they never fork the geometry or ink.

use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, Node, PositionType, UiRect, Val, percent, px, rem,
};
use bevy::ui_widgets::{
    Slider, SliderOrientation, SliderPrecision, SliderRange, SliderStep, SliderThumb, SliderValue,
    TrackClick,
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
