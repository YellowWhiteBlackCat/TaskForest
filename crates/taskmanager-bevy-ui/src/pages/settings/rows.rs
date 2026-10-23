//! The settings page's rows: the label / control / value ladder.
//!
//! Every settings row shares one shape — a bounded **label column**, the
//! **control** the preference owns, and the **live value** the authority
//! currently reports. The row builders live here so the page module stays
//! about the page's *authorities* and not about its furniture.
//!
//! The boolean rows own a real **switch** (the product's sliding control,
//! [`crate::widgets::switch`]); the discrete rows keep the official
//! `RadioButton` group. Both carry the page's typed choice on the same
//! entity as the widget, so one activation resolves to exactly one
//! preference.

use bevy::a11y::AccessibilityNode;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::Checked;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, Node, Overflow, PositionType, Val,
    percent, rem,
};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Checkbox, RadioButton, RadioGroup};

use crate::pages::settings::{SettingsChoice, SettingsField};
use crate::palette::{UiPalette, space_8};
use crate::widgets::switch;
use crate::window::{Role, TextRole};

/// One discrete choice on the page: display label, the typed choice it owns,
/// and whether the live authority currently selects it.
pub(crate) struct ChoiceEntry {
    pub(crate) label: String,
    pub(crate) choice: SettingsChoice,
    pub(crate) selected: bool,
}

/// The label column's width, in rem.
///
/// Every row shares one label column, so the controls line up. The width
/// rides the control height, which is the type ladder's own step — a UI-size
/// choice re-scales the column with the rest of the surface.
fn label_column_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem * 5.0
}

/// The control row's height, in rem: the type ladder's control step.
fn row_height_rem(palette: &UiPalette) -> f32 {
    palette.control_height_rem
}

/// The stable identity of one settings row.
///
/// The switch's slide memory is keyed by it, so it must survive the page
/// rebuild a preference change causes.
fn switch_key(field: &SettingsField) -> String {
    match field {
        SettingsField::Theme(_) => "theme".to_owned(),
        SettingsField::SystemMode => "system_mode".to_owned(),
        SettingsField::HighContrast(_) => "high_contrast".to_owned(),
        SettingsField::Language(_) => "language".to_owned(),
        SettingsField::Refresh(_) => "refresh".to_owned(),
        SettingsField::HistoryCapacity(_) => "history_capacity".to_owned(),
        SettingsField::HistoryPersistence(_) => "history_persistence".to_owned(),
        SettingsField::GrayZeroValues(_) => "gray_zero_values".to_owned(),
        SettingsField::UiSize(_) => "ui_size".to_owned(),
        SettingsField::PauseTelemetry => "pause_telemetry".to_owned(),
    }
}

/// A select-style row: caption label, the radio group, and the live value.
pub(crate) fn radio_row(
    label: &str,
    entries: Vec<ChoiceEntry>,
    value: &str,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let label = label.to_owned();
    let value = value.to_owned();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
             Node { width: rem(label_column_rem(palette)), height: Val::Auto, overflow: Overflow::clip_x() }
             Children [
                 Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
             ] --

                Node {
                    height: Val::Auto,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(space_8()),
                }
                RadioGroup
                Children [ { choice_widgets(entries, palette) } ]
            --
             Node { overflow: Overflow::clip_x() }
             Children [ Text(value) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ]
        ]
    })
}

/// A boolean row: caption label, the switch, and the live value.
pub(crate) fn toggle_row(
    label: &str,
    entry: ChoiceEntry,
    value: &str,
    palette: &UiPalette,
) -> Box<dyn Scene> {
    let label = label.to_owned();
    let value = value.to_owned();
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: Val::Auto,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
             Node { width: rem(label_column_rem(palette)), height: Val::Auto, overflow: Overflow::clip_x() }
             Children [
                 Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap }
             ] --
             { choice_widgets(vec![entry], palette) } --
             Node { overflow: Overflow::clip_x() }
             Children [ Text(value) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::NoWrap } ]
        ]
    })
}

/// One widget per choice. The shape pairs differ by the widget primitive
/// (`Checkbox` for the boolean toggles, `RadioButton` for the discrete
/// selects) and by the `Checked` marker, so the fan-out boxes the scenes
/// (the documented dynamic-children seam). The primitives are the official
/// unstyled ones; state is the `Checked` marker.
fn choice_widgets(entries: Vec<ChoiceEntry>, palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    entries
        .into_iter()
        .map(|entry| {
            let ChoiceEntry {
                label,
                choice,
                selected,
            } = entry;
            let SettingsChoice(field) = choice;
            let boolean = matches!(
                field,
                SettingsField::PauseTelemetry
                    | SettingsField::HistoryPersistence(_)
                    | SettingsField::GrayZeroValues(_)
            );
            match (boolean, selected) {
                (true, true) => Box::new(checked_switch_shape(switch_key(&field), field, palette))
                    as Box<dyn Scene>,
                (true, false) => {
                    Box::new(unchecked_switch_shape(switch_key(&field), field, palette))
                }
                (false, true) => Box::new(checked_radio_shape(label, field, palette)),
                (false, false) => Box::new(unchecked_radio_shape(label, field, palette)),
            }
        })
        .collect()
}

/// The switch in its ON position: the slot wears the accent and the `Checked`
/// marker states the preference.
fn checked_switch_shape(
    id: String,
    field: SettingsField,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let position = 1.0;
    bsn! {
        Node {
            position_type: PositionType::Relative,
            width: rem(switch::SLOT_WIDTH_REM),
            height: rem(switch::SLOT_HEIGHT_REM),
            border_radius: BorderRadius::all(rem(switch::SLOT_RADIUS_REM)),
            flex_shrink: 0.0,
        }
        BackgroundColor({ switch::slot_fill(true, palette) })
        Checkbox
        Checked
        AccessibilityNode({ switch::accessibility_node() })
        SettingsChoice(field)
        switch::SwitchId({ id })
        Children [ @{ switch::thumb_scene(palette, position) } ]
    }
}

/// The switch in its OFF position.
fn unchecked_switch_shape(
    id: String,
    field: SettingsField,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let position = 0.0;
    bsn! {
        Node {
            position_type: PositionType::Relative,
            width: rem(switch::SLOT_WIDTH_REM),
            height: rem(switch::SLOT_HEIGHT_REM),
            border_radius: BorderRadius::all(rem(switch::SLOT_RADIUS_REM)),
            flex_shrink: 0.0,
        }
        BackgroundColor({ switch::slot_fill(false, palette) })
        Checkbox
        AccessibilityNode({ switch::accessibility_node() })
        SettingsChoice(field)
        switch::SwitchId({ id })
        Children [ @{ switch::thumb_scene(palette, position) } ]
    }
}

fn checked_radio_shape(
    label: String,
    field: SettingsField,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            height: rem(row_height_rem(palette)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        RadioButton
        Checked
        SettingsChoice(field)
        Children [
             Text(label) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}

fn unchecked_radio_shape(
    label: String,
    field: SettingsField,
    palette: &UiPalette,
) -> impl Scene + use<> {
    bsn! {
        Node {
            height: rem(row_height_rem(palette)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
        }
        RadioButton
        SettingsChoice(field)
        Children [
             Text(label) TextRole(Role::Body) TextLayout { linebreak: LineBreak::NoWrap }
        ]
    }
}
