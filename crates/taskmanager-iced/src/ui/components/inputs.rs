//! Token-styled input controls shared by Iced pages.
//!
//! Owned by the iced-inputs workstream: switch, slider, select, search
//! input, and the segmented choice group. All controls take a theme
//! snapshot plus typed callbacks and stay focus-reachable through the
//! crate's focus shell.
//!
//! Family contracts:
//! - Every color and corner radius derives from the neutral [`Theme`]
//!   palette; spacing reads `tokens::SPACE_*` and the few remaining numbers
//!   are fixed control geometry (the `px(...)` contract), never a hue.
//! - Pointer paths stay toolkit-native (iced `button` / `slider` /
//!   `pick_list` / `text_input`); keyboard paths ride the crate focus shell
//!   ([`crate::focus`]) so Enter/Space activation, Tab traversal and the
//!   focus ring match every other control in this frontend.
//! - Degenerate states stay honest: a disabled switch is not a focus stop,
//!   an option-less select renders an inert placeholder surface instead of
//!   a fake interactive control, a non-positive slider step degrades to a
//!   continuous slider instead of dividing by zero, and an empty segmented
//!   choice list renders no dead track.

use std::ops::RangeInclusive;

use iced::widget::{Space, button, container, row, text};
use iced::{Background, Border, Color, Length, Vector};
use taskmanager_theme::color::mix;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::IconId;

use super::IcedElement;
use crate::app::{FocusTarget, Message};
use crate::focus::{FocusableButton, focus_id};

/// Switch track width (GPUI parity: 36px pill).
const SWITCH_TRACK_WIDTH: f32 = 36.0;
/// Switch track height (GPUI parity: 20px pill).
const SWITCH_TRACK_HEIGHT: f32 = 20.0;
/// Switch knob diameter (GPUI parity: 16px circle).
const SWITCH_KNOB_SIZE: f32 = 16.0;

/// A toggle switch: accent pill track when on, quiet border track when off.
/// Pointer clicks ride the inner iced button; keyboard Enter/Space rides the
/// crate focus shell, and the focus ring is the shell's standard accent ring.
/// `on_toggle` receives the NEW state (the inverted `enabled`). A disabled
/// switch loses its `on_press` — iced then keeps it in `Status::Disabled`
/// forever — and is not wrapped in the focus shell, so it is neither a focus
/// stop nor a click target while its dimmed geometry stays legible.
#[must_use]
pub(crate) fn switch<'a>(
    theme_snapshot: &'a Theme,
    focus_target: FocusTarget,
    enabled: bool,
    disabled: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> IcedElement<'a> {
    // Knob geometry: the knob occupies a token-width leading gap when on and
    // slides to the leading edge when off (row layout inside the padded
    // track) — the same 16px slide the GPUI switch renders.
    let mut track_children: Vec<IcedElement<'a>> = Vec::with_capacity(2);
    if enabled {
        track_children.push(
            Space::new()
                .width(Length::Fixed(f32::from(tokens::SPACE_16)))
                .into(),
        );
    }
    track_children.push(
        container(Space::new())
            .width(Length::Fixed(SWITCH_KNOB_SIZE))
            .height(Length::Fixed(SWITCH_KNOB_SIZE))
            .style(move |_theme| switch_knob_style(theme_snapshot, disabled))
            .into(),
    );
    let track = row(track_children)
        .padding(f32::from(tokens::SPACE_2))
        .align_y(iced::Alignment::Center)
        .width(Length::Fixed(SWITCH_TRACK_WIDTH))
        .height(Length::Fixed(SWITCH_TRACK_HEIGHT));

    let press = on_toggle(!enabled);
    let control = button(track)
        .padding(0.0)
        .width(Length::Fixed(SWITCH_TRACK_WIDTH))
        .height(Length::Fixed(SWITCH_TRACK_HEIGHT))
        .style(move |_theme, status| switch_track_style(theme_snapshot, enabled, status));
    if disabled {
        // Without `on_press` the iced button never fires and its style never
        // leaves `Status::Disabled`; no focus wrapper means no Tab stop.
        return control.into();
    }
    let palette = theme_snapshot.palette();
    FocusableButton::new(
        focus_id(focus_target),
        control.on_press(press.clone()).into(),
        press,
        focus_target,
        taskmanager_theme::iced::color(palette.accent),
        SWITCH_TRACK_HEIGHT / 2.0,
        false,
    )
    .into()
}

/// The switch track fill for one pointer/disabled status, read off the
/// palette (`accent` on / `border` off; pointer states blend existing tokens
/// exactly like the crate's button styles; disabled dims toward the backdrop).
fn switch_track_fill(theme_snapshot: &Theme, on: bool, status: button::Status) -> Color {
    let palette = theme_snapshot.palette();
    let base = if on { palette.accent } else { palette.border };
    match status {
        button::Status::Disabled => {
            taskmanager_theme::iced::color(mix(base, palette.window_backdrop, 0.5))
        }
        button::Status::Hovered => taskmanager_theme::iced::color(mix(base, palette.hover, 0.28)),
        button::Status::Pressed => {
            taskmanager_theme::iced::color(mix(base, taskmanager_theme::Color::BLACK, 0.22))
        }
        _ => taskmanager_theme::iced::color(base),
    }
}

fn switch_track_style(theme_snapshot: &Theme, on: bool, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(switch_track_fill(
            theme_snapshot,
            on,
            status,
        ))),
        text_color: taskmanager_theme::iced::color(theme_snapshot.palette().fg),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: (SWITCH_TRACK_HEIGHT / 2.0).into(),
        },
        ..button::Style::default()
    }
}

fn switch_knob_style(theme_snapshot: &Theme, disabled: bool) -> container::Style {
    let palette = theme_snapshot.palette();
    let fill = if disabled {
        mix(palette.surface, palette.window_backdrop, 0.5)
    } else {
        palette.surface
    };
    container::Style {
        background: Some(Background::Color(taskmanager_theme::iced::color(fill))),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: (SWITCH_KNOB_SIZE / 2.0).into(),
        },
        shadow: iced::Shadow {
            color: taskmanager_theme::iced::color(palette.card_shadow),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        ..container::Style::default()
    }
}

/// A ranged value slider: iced's native slider in token styling, wrapped in
/// the crate focus shell. The value is clamped to `range` and snapped to the
/// `step` grid by [`slider_snapped_value`] BEFORE it reaches the widget, the
/// optional `label` readout and `on_change`, so the three can never disagree.
/// A non-positive or non-finite `step` degrades to a continuous slider
/// (iced's own step math divides by the step and must never see a zero).
/// Enter/Space while focused re-affirms the current snapped value — an
/// idempotent confirmation that keeps the control a real focus stop.
#[must_use]
pub(crate) fn slider<'a>(
    theme_snapshot: &'a Theme,
    focus_target: FocusTarget,
    range: RangeInclusive<f32>,
    step: f32,
    value: f32,
    on_change: impl Fn(f32) -> Message + 'a,
    label: Option<impl Fn(f32) -> String + 'a>,
) -> IcedElement<'a> {
    let snapped = slider_snapped_value(&range, step, value);
    let confirm = on_change(snapped);
    let mut native = iced::widget::slider(range.clone(), snapped, move |moved| {
        on_change(slider_snapped_value(&range, step, moved))
    })
    .style(move |_theme, status| slider_style(theme_snapshot, status))
    .width(Length::Fill);
    if step.is_finite() && step > 0.0 {
        native = native.step(step);
    }
    let control: IcedElement<'a> =
        crate::focus::focusable_control(theme_snapshot, focus_target, native.into(), confirm);
    match label {
        None => control,
        Some(format_value) => row![
            control,
            text(format_value(snapped))
                .size(f32::from(tokens::FONT_12))
                .font(taskmanager_theme::iced::mono_font(theme_snapshot))
        ]
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center)
        .into(),
    }
}

/// Clamp `value` into `range` and snap it onto the `step` grid anchored at
/// the range start. The range endpoints stay exact (a value at or past an
/// end lands exactly on that end, so the extremes are always reachable);
/// interior values snap to the nearest grid point. Degenerate steps
/// (non-finite, zero, negative) and unordered/empty ranges degrade to plain
/// clamping between both ends — never a division by zero or a NaN escaping
/// into the view.
fn slider_snapped_value(range: &RangeInclusive<f32>, step: f32, value: f32) -> f32 {
    let (start, end) = (*range.start(), *range.end());
    let (low, high) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let clamped = value.clamp(low, high);
    if !step.is_finite() || step <= 0.0 || end == start || clamped == low || clamped == high {
        return clamped;
    }
    let steps = ((clamped - start) / step).round();
    (start + steps * step).clamp(low, high)
}

fn slider_style(
    theme_snapshot: &Theme,
    _status: iced::widget::slider::Status,
) -> iced::widget::slider::Style {
    let palette = theme_snapshot.palette();
    iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (
                Background::Color(taskmanager_theme::iced::color(palette.accent)),
                Background::Color(taskmanager_theme::iced::color(palette.border)),
            ),
            width: f32::from(tokens::SPACE_4),
            border: Border::default(),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle {
                radius: f32::from(tokens::SPACE_8),
            },
            background: Background::Color(taskmanager_theme::iced::color(palette.accent)),
            border_width: f32::from(tokens::SPACE_2),
            border_color: taskmanager_theme::iced::color(palette.surface),
        },
    }
}

/// A single-choice select over iced's `pick_list`, token-styled and stretched
/// to `Fill`. The trigger shows the selected option (or `placeholder`) and
/// picking an option runs `on_change` with a borrow of the chosen option.
/// Degenerate cases stay honest: an empty option list renders an inert
/// placeholder surface (no dead focus stop, no fake interaction), and
/// Enter/Space while focused re-affirms the current selection — a plain
/// focus notice when nothing is selected, never a fabricated value.
#[must_use]
pub(crate) fn select<'a, T>(
    theme_snapshot: &'a Theme,
    focus_target: FocusTarget,
    options: &'a [T],
    selected: Option<&'a T>,
    placeholder: &'a str,
    on_change: impl Fn(&T) -> Message + 'a,
) -> IcedElement<'a>
where
    T: ToString + PartialEq + Clone + 'a,
{
    if options.is_empty() {
        return container(
            text(placeholder)
                .size(f32::from(tokens::FONT_12))
                .color(crate::theme::muted_text_color(theme_snapshot)),
        )
        .style(move |_theme| select_placeholder_style(theme_snapshot))
        .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
        .width(Length::Fill)
        .into();
    }
    let confirm = match selected {
        Some(current) => on_change(current),
        None => Message::Focus(focus_target),
    };
    let pick = iced::widget::pick_list(options, selected, move |picked: T| on_change(&picked))
        .placeholder(placeholder)
        .width(Length::Fill)
        .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
        .style(move |_theme, status| select_style(theme_snapshot, status));
    crate::focus::focusable_control(theme_snapshot, focus_target, pick.into(), confirm)
}

fn select_style(
    theme_snapshot: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let palette = theme_snapshot.palette();
    let border_color = match status {
        iced::widget::pick_list::Status::Opened { .. } => {
            taskmanager_theme::iced::color(palette.accent)
        }
        _ => taskmanager_theme::iced::color(palette.border),
    };
    iced::widget::pick_list::Style {
        text_color: taskmanager_theme::iced::color(palette.fg),
        placeholder_color: taskmanager_theme::iced::color(palette.fg_muted),
        handle_color: taskmanager_theme::iced::color(palette.fg_muted),
        background: Background::Color(taskmanager_theme::iced::color(palette.surface)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: f32::from(palette.control_radius).into(),
        },
    }
}

fn select_placeholder_style(theme_snapshot: &Theme) -> container::Style {
    let palette = theme_snapshot.palette();
    container::Style {
        background: Some(Background::Color(taskmanager_theme::iced::color(
            palette.surface,
        ))),
        text_color: Some(taskmanager_theme::iced::color(palette.fg_muted)),
        border: Border {
            color: taskmanager_theme::iced::color(palette.border),
            width: 1.0,
            radius: f32::from(palette.control_radius).into(),
        },
        ..container::Style::default()
    }
}

/// A search field: SVG search-icon prefix, token-styled text input, and a
/// clear affordance that appears only for a non-empty query and publishes
/// `on_change("")`. The field itself registers under the crate's stable
/// focus id and is focusable through iced's native text-input focus, so
/// keyboard users reach and edit it with the standard traversal; the clear
/// button is the pointer convenience over the same contract. Consumed by the
/// Services page's filter field; the Users and Startup pages carry no search
/// field today, so they have no call site yet.
#[must_use]
pub(crate) fn search_input<'a>(
    theme_snapshot: &'a Theme,
    focus_target: FocusTarget,
    placeholder: &'a str,
    value: &'a str,
    on_change: impl Fn(String) -> Message + 'a,
) -> IcedElement<'a> {
    // Computed before `on_change` moves into the input closure: the clear
    // button publishes the same `on_change` contract with an empty query.
    let clear = on_change(String::new());
    let input = iced::widget::text_input(placeholder, value)
        .id(iced::widget::Id::from(focus_id(focus_target)))
        .on_input(on_change)
        .width(Length::Fill)
        .style(move |_theme, status| search_style(theme_snapshot, status));
    let mut field = row![
        crate::icons::icon(theme_snapshot, IconId::Search, 14.0),
        input,
    ]
    .spacing(f32::from(tokens::SPACE_6))
    .padding(f32::from(tokens::SPACE_4))
    .align_y(iced::Alignment::Center);
    if search_shows_clear(value) {
        field = field.push(
            button(crate::icons::icon(theme_snapshot, IconId::Close, 12.0))
                .padding(f32::from(tokens::SPACE_2))
                .style(move |_theme, status| {
                    crate::theme::ghost_button_style(theme_snapshot, status)
                })
                .on_press(clear),
        );
    }
    field.into()
}

/// The clear affordance exists only for a query that can be cleared; an
/// empty field never renders a dead button.
fn search_shows_clear(value: &str) -> bool {
    !value.is_empty()
}

fn search_style(
    theme_snapshot: &Theme,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    let palette = theme_snapshot.palette();
    let border_color = match status {
        iced::widget::text_input::Status::Focused { .. } => {
            taskmanager_theme::iced::color(palette.accent)
        }
        _ => taskmanager_theme::iced::color(palette.border),
    };
    iced::widget::text_input::Style {
        background: Background::Color(taskmanager_theme::iced::color(palette.surface)),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: f32::from(palette.control_radius).into(),
        },
        icon: taskmanager_theme::iced::color(palette.fg_muted),
        placeholder: taskmanager_theme::iced::color(palette.fg_muted),
        value: taskmanager_theme::iced::color(palette.fg),
        selection: taskmanager_theme::iced::color(palette.selection),
    }
}

/// A mutually-exclusive segmented choice group rendered as ONE connected
/// control: outer border/surface track, flush inner segments, accent fill on
/// the active choice — the settings-page replacement for loose pill rows.
///
/// `choices` carries `(label, value)` pairs; `active` is a VALUE compared
/// against each choice and `on_change` receives the newly selected VALUE.
/// The whole track is a single focus stop ([`SegmentedTrack`]): Left/Right
/// moves the selection to the adjacent choice, wrapping past the ends and
/// publishing exactly the message that choice's segment publishes; Enter/
/// Space re-affirms the active choice. Pointer clicks ride the inner iced
/// buttons. An unknown `active` marks no segment and disables arrow moves
/// until the caller's state names a real choice; an empty choice list
/// renders no dead track.
#[must_use]
pub(crate) fn segmented<'a>(
    theme_snapshot: &'a Theme,
    focus_target: FocusTarget,
    choices: &[(String, usize)],
    active: usize,
    on_change: impl Fn(usize) -> Message + 'a,
) -> IcedElement<'a> {
    if choices.is_empty() {
        return Space::new().into();
    }
    let on_change = Box::new(on_change);
    let last = choices.len() - 1;
    let segments: Vec<IcedElement<'a>> = choices
        .iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let selected = *value == active;
            button(text(label.clone()))
                .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
                .style(move |_theme, status| {
                    segment_style(theme_snapshot, selected, index, last, status)
                })
                .on_press(on_change(*value))
                .into()
        })
        .collect();
    let track = container(row(segments))
        .style(move |_theme| segmented_track_style(theme_snapshot))
        .width(Length::Shrink);
    let palette = theme_snapshot.palette();
    SegmentedTrack::new(
        focus_id(focus_target),
        track.into(),
        on_change,
        choices.to_vec(),
        active,
        focus_target,
        taskmanager_theme::iced::color(palette.accent),
        f32::from(palette.control_radius),
    )
    .into()
}

/// The index of the active VALUE, if the caller's state names a real choice.
fn segmented_active_index(choices: &[(String, usize)], active: usize) -> Option<usize> {
    choices.iter().position(|&(_, value)| value == active)
}

/// The index adjacent to the active choice in the `right` direction,
/// wrapping past both ends. `None` when the active value is unknown or the
/// group has no neighbor to move to — the keyboard path then leaves the
/// event alone instead of inventing a target.
fn segmented_neighbor_index(
    choices: &[(String, usize)],
    active: usize,
    right: bool,
) -> Option<usize> {
    let current = segmented_active_index(choices, active)?;
    let len = choices.len();
    if len <= 1 {
        return None;
    }
    Some(if right {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    })
}

fn segmented_track_style(theme_snapshot: &Theme) -> container::Style {
    let palette = theme_snapshot.palette();
    container::Style {
        background: Some(Background::Color(taskmanager_theme::iced::color(
            palette.surface,
        ))),
        text_color: Some(taskmanager_theme::iced::color(palette.fg)),
        border: Border {
            color: taskmanager_theme::iced::color(palette.border),
            width: 1.0,
            radius: f32::from(palette.control_radius).into(),
        },
        ..container::Style::default()
    }
}

/// One segment's surface. The active segment fills with the accent and wins
/// over hover (a selected segment stays accent-filled); idle segments are
/// transparent over the track surface with token hover/pressed tints. Only
/// the outer corners round — the track's control radius on the first/last
/// segment's outer edges — so the group reads as one connected control.
fn segment_style(
    theme_snapshot: &Theme,
    selected: bool,
    index: usize,
    last: usize,
    status: button::Status,
) -> button::Style {
    let palette = theme_snapshot.palette();
    let corner = f32::from(palette.control_radius);
    let radius = iced::border::Radius {
        top_left: if index == 0 { corner } else { 0.0 },
        top_right: if index == last { corner } else { 0.0 },
        bottom_right: if index == last { corner } else { 0.0 },
        bottom_left: if index == 0 { corner } else { 0.0 },
    };
    let (background, text_color) = if selected {
        (
            Background::Color(taskmanager_theme::iced::color(palette.accent)),
            taskmanager_theme::iced::color(theme_snapshot.accent_text),
        )
    } else {
        match status {
            button::Status::Hovered => (
                Background::Color(taskmanager_theme::iced::color(palette.hover)),
                taskmanager_theme::iced::color(palette.fg),
            ),
            button::Status::Pressed => (
                Background::Color(taskmanager_theme::iced::color(mix(
                    palette.hover,
                    palette.window_backdrop,
                    0.45,
                ))),
                taskmanager_theme::iced::color(palette.fg),
            ),
            _ => (
                Background::Color(Color::TRANSPARENT),
                taskmanager_theme::iced::color(palette.fg),
            ),
        }
    };
    button::Style {
        background: Some(background),
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius,
        },
        ..button::Style::default()
    }
}

mod segmented_track;
use segmented_track::SegmentedTrack;

#[cfg(test)]
#[path = "../../../tests/gui/ui/components_inputs_tests.rs"]
mod components_inputs_tests;
