//! Service status dot — the typed running/stopped/failed indicator with its
//! breathing pulse.
//!
//! **Ownership**: the dot's semantic identity (which status it paints) is the
//! page's; its color comes from the palette tokens only. The Bevy palette
//! exposes no success/green token yet, so "running" wears the app accent —
//! the same token the positive status chip already uses — while "failed"
//! wears the destructive red and "stopped" the dim gray. When
//! `crate::palette::UiPalette` grows a success token, [`status_dot_color`] is
//! the one function to re-target.
//!
//! **The pulse is typed state, not a wall-clock animation**: [`ServicePulse`]
//! carries the accumulated breathing time, [`pulse_opacity`] /
//! [`pulse_scale`] are the pure curves, and one system advances the phase and
//! applies it to the dots that are mounted. A test asserts the typed phase
//! and the curve, never a sleep.
//!
//! **Hierarchy**: a running/failed unit reads at the full icon step, a
//! stopped one shrinks and dims; only the running dot breathes, because a
//! stopped or failed unit has nothing to announce.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::scene::{Scene, bsn};
use bevy::time::Time;
use bevy::ui::prelude::{BackgroundColor, BorderRadius, Node, Val, rem};

use crate::pages::services::StatusChip;
use crate::palette::UiPalette;
use crate::window::WindowPalette;

/// One breathing period of the status dot, in seconds.
pub(crate) const PULSE_PERIOD_SECS: f32 = 2.0;

/// Marker on one status dot: the semantic status the dot paints. The marker
/// is the dot's typed identity — the pulse system reads it instead of
/// guessing from the color.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceStatusDot(pub(crate) StatusChip);

/// The typed breathing state of the service status dots.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub(crate) struct ServicePulse {
    /// Accumulated breathing time, folded into [`PULSE_PERIOD_SECS`].
    pub(crate) elapsed: f32,
}

/// The pure breathing curve: `0` is the dimmest/smallest point of the cycle,
/// `1` the brightest/largest. A triangle wave — no easing table, no drift.
#[must_use]
pub(crate) fn pulse_phase(elapsed_secs: f32) -> f32 {
    let cycle = (elapsed_secs / PULSE_PERIOD_SECS).rem_euclid(1.0);
    if cycle < 0.5 {
        cycle * 2.0
    } else {
        (1.0 - cycle) * 2.0
    }
}

/// The opacity multiplier of one breathing phase.
#[must_use]
pub(crate) fn pulse_opacity(phase: f32) -> f32 {
    0.55 + 0.45 * phase.clamp(0.0, 1.0)
}

/// The scale multiplier of one breathing phase.
#[must_use]
pub(crate) fn pulse_scale(phase: f32) -> f32 {
    0.85 + 0.15 * phase.clamp(0.0, 1.0)
}

/// The dot ink per semantic status — palette tokens only.
#[must_use]
pub(crate) fn status_dot_color(chip: StatusChip, palette: &UiPalette) -> Color {
    match chip {
        // A running unit wears the theme's own success ink: the accent is the
        // product's interactive hue, not the positive-state hue.
        StatusChip::Positive => palette.success_color,
        StatusChip::Negative => palette.danger_color,
        StatusChip::Idle => palette.dim_color,
    }
}

/// The dot's resting opacity per semantic status: a stopped unit is present
/// but quiet, a failed unit is never quietened.
#[must_use]
pub(crate) fn status_dot_alpha(chip: StatusChip) -> f32 {
    match chip {
        StatusChip::Positive | StatusChip::Negative => 1.0,
        StatusChip::Idle => 0.7,
    }
}

/// The dot's base extent in rem — the status hierarchy. A running or failed
/// unit reads at the full icon step; a stopped one shrinks to a third less.
#[must_use]
pub(crate) fn status_dot_extent_rem(chip: StatusChip, palette: &UiPalette) -> f32 {
    let base = palette.icon_rem;
    match chip {
        StatusChip::Positive | StatusChip::Negative => base * 0.72,
        StatusChip::Idle => base * 0.48,
    }
}

/// The status dot scene: a solid disc whose ink and extent carry the status.
pub(crate) fn status_dot_scene(chip: StatusChip, palette: &UiPalette) -> Box<dyn Scene> {
    let extent = status_dot_extent_rem(chip, palette);
    let ink = status_dot_color(chip, palette);
    let radius = BorderRadius::all(Val::Rem(extent * 0.5));
    Box::new(bsn! {
        Node {
            width: rem(extent),
            height: rem(extent),
            flex_shrink: 0.0,
            border_radius: { radius },
        }
        BackgroundColor({ ink })
        ServiceStatusDot({ chip })
    })
}

/// Advance the typed breathing phase and apply it to the mounted dots.
///
/// The system runs in the `Last` schedule: the page body is mounted by the
/// coalesced painter in `PostUpdate`, and the scene entities only exist once
/// that schedule's commands are flushed — so `Last` is the first slot where
/// the dots are visible to a typed update.
pub(crate) fn advance_service_pulse(
    time: Res<Time>,
    mut pulse: ResMut<ServicePulse>,
    palette: Res<WindowPalette>,
    mut dots: Query<(&ServiceStatusDot, &mut BackgroundColor, &mut Node)>,
) {
    pulse.elapsed = (pulse.elapsed + time.delta_secs()).rem_euclid(PULSE_PERIOD_SECS);
    let phase = pulse_phase(pulse.elapsed);
    for (dot, mut fill, mut node) in &mut dots {
        let chip = dot.0;
        let breathing = chip == StatusChip::Positive;
        let opacity = status_dot_alpha(chip) * if breathing { pulse_opacity(phase) } else { 1.0 };
        let scale = if breathing { pulse_scale(phase) } else { 1.0 };
        let extent = status_dot_extent_rem(chip, &palette.inner) * scale;
        let ink = status_dot_color(chip, &palette.inner).to_srgba();
        let next = Color::srgba(ink.red, ink.green, ink.blue, opacity);
        if fill.0.to_srgba() != next.to_srgba() {
            fill.0 = next;
        }
        let width = Val::Rem(extent);
        if node.width != width {
            node.width = width;
            node.height = width;
            node.border_radius = BorderRadius::all(Val::Rem(extent * 0.5));
        }
    }
}
