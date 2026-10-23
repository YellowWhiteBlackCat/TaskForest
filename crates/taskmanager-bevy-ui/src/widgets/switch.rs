//! The product-owned switch control (BEVY-177).
//!
//! A real sliding switch, not a styled checkbox: a rounded **slot**, a
//! capsule **thumb** that slides between the two ends, and the official
//! `bevy_ui_widgets` toggle behavior (pointer click, `Enter`/`Space`)
//! re-declared with the `Switch` accessibility role the platform tree
//! publishes.
//!
//! The behavior primitive stays Bevy's: the `Checkbox` component owns the
//! activation and the `Checked` marker owns the state, exactly like the
//! radio rows. The *appearance* is the product's: the slot, the thumb and
//! the slide.
//!
//! The slide rides the frontend's shared ease-out curve
//! (`crate::widgets::easing`), so the switch and every other
//! interaction-state transition share one timing law (BEVY-182).
//!
//! **Why the caller composes the slot.** The page that owns the preference
//! declares the slot node, because the page's typed choice must ride the
//! *same* entity as the toggle behavior: one activation then resolves to
//! exactly one preference. The switch's geometry, ink, accessibility role,
//! thumb and slide live here. The `Checkbox` component is the official
//! toggle primitive — it owns the pointer/keyboard activation and the
//! `Checked` marker — and the `AccessibilityNode` overrides the
//! primitive's default `CheckBox` role with the `Switch` role, which is
//! what a platform assistive technology expects for a sliding switch.
//!
//! **Why the position memory.** A settings row rebuilds its whole subtree
//! when the preference it owns changes, so the *new* switch is born at its
//! new end state. The memory keeps the position the user last saw, and the
//! rebuilt switch slides from there — a preference change reads as one
//! moving knob instead of a knob that teleports.

use std::collections::HashMap;
use std::time::Duration;

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::picking::Pickable;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, BorderRadius, Node, PositionType, Val, rem};
use bevy::ui::{Checked, UiTransform, Val2};
use taskmanager_theme::tokens::{self, MotionPolicy};

use crate::palette::UiPalette;
use crate::widgets::easing::{Easing, eased_progress};
use crate::window::WindowPalette;

/// The slot's width, in rem — the switch rides the reading-size axis, so it
/// re-scales with the user's UI-size choice.
pub(crate) const SLOT_WIDTH_REM: f32 = 2.6;
/// The slot's height, in rem.
pub(crate) const SLOT_HEIGHT_REM: f32 = 1.3;
/// The slot's corner radius, in rem: the capsule.
pub(crate) const SLOT_RADIUS_REM: f32 = SLOT_HEIGHT_REM / 2.0;
/// The thumb's diameter, in rem.
const THUMB_REM: f32 = 1.0;
/// The travel between the two ends of the slot, in rem: the slot's width
/// minus the thumb and the two insets.
const TRAVEL_REM: f32 = SLOT_WIDTH_REM - THUMB_REM - 0.3;

/// The stable identity of one switch.
///
/// The id survives a page rebuild, so the slide memory can find the switch
/// again after the remount that follows a preference change.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SwitchId(pub(crate) String);

/// The thumb of one switch — the component the slide system drives.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SwitchThumb;

/// The slide state of one thumb.
///
/// The position is a **ratio** along the slot (`0.0` = off, `1.0` = on), not
/// a pixel offset: a UI-size choice re-scales the slot, and a ratio survives
/// that re-scale while a stale pixel offset would not.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SwitchSlide {
    /// The thumb's current position along the slot.
    pub(crate) position: f32,
    /// The position the thumb animates toward.
    pub(crate) target: f32,
    /// The position the running transition started from.
    pub(crate) from: f32,
    /// The elapsed time of the running transition.
    pub(crate) elapsed: Duration,
    /// Whether a transition is running.
    pub(crate) running: bool,
}

/// The remembered position of every switch, keyed by its stable id.
#[derive(Resource, Default)]
pub(crate) struct SwitchMemory {
    positions: HashMap<String, f32>,
}

impl SwitchMemory {
    /// The last position seen for `id`, if the switch was mounted before.
    #[must_use]
    pub(crate) fn position(&self, id: &str) -> Option<f32> {
        self.positions.get(id).copied()
    }

    /// Remember the position `id` currently shows.
    pub(crate) fn remember(&mut self, id: &str, position: f32) {
        self.positions.insert(id.to_owned(), position);
    }
}

/// The slide's duration: the theme's medium transition token under the
/// shared motion policy. A policy that forbids motion resolves to no
/// transition at all — the thumb lands on its end state immediately.
#[must_use]
pub(crate) fn slide_duration() -> Duration {
    MotionPolicy::Normal
        .animation_duration(tokens::DURATION_MEDIUM)
        .unwrap_or(Duration::ZERO)
}

/// The slot's fill: the accent when the switch is on, the theme's border
/// tone when it is off.
pub(crate) fn slot_fill(on: bool, palette: &UiPalette) -> Color {
    if on {
        palette.accent
    } else {
        palette.border_color
    }
}

/// The switch's accessibility node: the platform tree publishes a *switch*,
/// not a checkbox, so assistive technology announces the control the user
/// actually sees.
#[must_use]
pub(crate) fn accessibility_node() -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::Switch)
}

/// The thumb: a capsule that sits at the slot's off end and slides to the
/// on end. The slide system owns the offset, so the scene only declares the
/// thumb's own geometry and its resting state.
pub(crate) fn thumb_scene(palette: &UiPalette, position: f32) -> impl Scene + use<> {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: rem((SLOT_HEIGHT_REM - THUMB_REM) / 2.0),
            width: rem(THUMB_REM),
            height: rem(THUMB_REM),
            border_radius: BorderRadius::all(rem(THUMB_REM / 2.0)),
        }
        BackgroundColor({ palette.panel_fill })
        SwitchThumb
        SwitchSlide {
            position: { position },
            target: { position },
            from: { position },
        }
        Pickable::IGNORE
    }
}

/// Advance every switch thumb toward its slot's checked state.
///
/// The transition is a *typed* curve over a *typed* duration: the thumb
/// eases from its old end to its new end on the shared ease-out law, and a
/// switch whose state did not change never touches its transform.
pub(crate) fn sync_switch_thumbs(
    palette: Res<WindowPalette>,
    time: Res<bevy::time::Time>,
    mut memory: ResMut<SwitchMemory>,
    mut thumbs: Query<(&mut SwitchSlide, &mut UiTransform, &ChildOf), With<SwitchThumb>>,
    slots: Query<(Option<&Checked>, Option<&SwitchId>)>,
) {
    let duration = slide_duration();
    let travel_px = TRAVEL_REM * palette.inner.rem_px;
    for (mut slide, mut transform, parent) in &mut thumbs {
        let Ok((checked, id)) = slots.get(parent.parent()) else {
            continue;
        };
        let on = checked.is_some();
        let target = if on { 1.0 } else { 0.0 };
        // A rebuilt switch is born at its new end state; the memory says
        // where the user last saw it, and the thumb slides from there.
        if !slide.running
            && let Some(id) = id
            && let Some(remembered) = memory.position(&id.0)
            && (remembered - target).abs() > f32::EPSILON
        {
            slide.from = remembered;
            slide.position = remembered;
            slide.target = target;
            slide.elapsed = Duration::ZERO;
            slide.running = true;
        }
        if slide.target != target {
            slide.from = slide.position;
            slide.target = target;
            slide.elapsed = Duration::ZERO;
            slide.running = true;
        }
        if slide.running {
            slide.elapsed += time.delta();
            let progress = eased_progress(Easing::EaseOut, slide.elapsed, duration);
            slide.position = slide.from + (slide.target - slide.from) * progress;
            if progress >= 1.0 {
                slide.position = slide.target;
                slide.running = false;
            }
        }
        if let Some(id) = id {
            memory.remember(&id.0, slide.position);
        }
        let offset = slide.position * travel_px;
        let translation = Val2::px(offset, 0.0);
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
}
