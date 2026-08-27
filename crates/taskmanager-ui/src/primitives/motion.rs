//! Motion helpers for token-driven, one-shot visual transitions.
//!
//! GPUI 0.2.x has no CSS-style transition system, so every transition here is
//! an explicit `with_animation` wrapper. `MotionPolicy::NoMotion` and no-op
//! endpoints skip that wrapper and apply the final style directly; this also
//! avoids passing zero durations to GPUI. `Reduced` keeps only a short,
//! token-capped opacity/color sweep.
//!
//! The helpers never animate layout. Callers own the stable id of the wrapped
//! element; only the hover motion id should encode the state with
//! [`hover_state_key`], which restarts the explicit one-shot sweep without
//! changing the target element's identity or bounds. Idle hover states are
//! applied directly, so virtualized lists do not create one animation per
//! row.

use std::time::Duration;

use gpui::{Animation, AnimationExt, AnyElement, ElementId, IntoElement, Styled, ease_in_out};
use taskmanager_theme::Color;
use taskmanager_theme::color::mix;
use taskmanager_theme::tokens::{DURATION_FAST, DURATION_HOVER, DURATION_MEDIUM, MotionPolicy};

/// The hover/state-transition animation: 120ms ease-in-out (Fluent hover
/// class, [`DURATION_HOVER`]).
pub fn hover_animation() -> Animation {
    Animation::new(DURATION_HOVER).with_easing(ease_in_out)
}

/// Policy-aware hover animation. `None` means the caller should paint the
/// final hover state without mounting an animation wrapper.
pub fn hover_animation_for(policy: MotionPolicy) -> Option<Animation> {
    taskmanager_theme::gpui::motion_animation(policy, DURATION_HOVER)
}

/// Pure key encoder for a two-state hover transition: packs both booleans
/// into the animation id's integer slot. Equal state pairs map to equal keys
/// (the animation keeps running across frames) and distinct pairs map to
/// distinct keys (gpui restarts the animation exactly on state changes).
pub const fn hover_state_key(state_a: bool, state_b: bool) -> u64 {
    ((state_a as u64) << 1) | (state_b as u64)
}

fn safe_opacity(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn safe_progress(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn interpolate_opacity(from: f32, to: f32, progress: f32) -> f32 {
    let from = safe_opacity(from);
    let to = safe_opacity(to);
    (from + (to - from) * safe_progress(progress)).clamp(0.0, 1.0)
}

fn interpolate_color(from: Color, to: Color, progress: f32) -> Color {
    mix(from, to, safe_progress(progress))
}

fn animation_for(policy: MotionPolicy, duration: Duration) -> Option<Animation> {
    taskmanager_theme::gpui::motion_animation(policy, duration)
}

fn opacity_transition_with_duration<E>(
    element: E,
    id: impl Into<ElementId>,
    from: f32,
    to: f32,
    policy: MotionPolicy,
    duration: Duration,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    let from = safe_opacity(from);
    let to = safe_opacity(to);
    if from == to {
        return element.opacity(to).into_any_element();
    }

    match animation_for(policy, duration) {
        Some(animation) => element
            .with_animation(id, animation, move |element, progress| {
                element.opacity(interpolate_opacity(from, to, progress))
            })
            .into_any_element(),
        None => element.opacity(to).into_any_element(),
    }
}

fn color_transition_with_duration<E>(
    element: E,
    id: impl Into<ElementId>,
    from: Color,
    to: Color,
    policy: MotionPolicy,
    duration: Duration,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if from == to {
        return element.bg(to).into_any_element();
    }

    match animation_for(policy, duration) {
        Some(animation) => element
            .with_animation(id, animation, move |element, progress| {
                element.bg(interpolate_color(from, to, progress))
            })
            .into_any_element(),
        None => element.bg(to).into_any_element(),
    }
}

/// Animate an element's opacity with the fast theme token.
///
/// The endpoints are clamped to the GPUI opacity range. `NoMotion` applies
/// `to` immediately, and the element's layout/id remain untouched.
pub fn opacity_transition<E>(
    element: E,
    id: impl Into<ElementId>,
    from: f32,
    to: f32,
    policy: MotionPolicy,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    opacity_transition_with_duration(element, id, from, to, policy, DURATION_FAST)
}

/// Animate an element's background color with the fast theme token.
///
/// Colors must come from the theme token layer. The helper only interpolates
/// the supplied colors and never invents a product color.
pub fn color_transition<E>(
    element: E,
    id: impl Into<ElementId>,
    from: Color,
    to: Color,
    policy: MotionPolicy,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    color_transition_with_duration(element, id, from, to, policy, DURATION_FAST)
}

/// Hover background transition: while `hovered` the background eases
/// base→hover over [`DURATION_HOVER`] (the sweep restarts each time the
/// state flips — see the module docs on why leaving is instant). The
/// caller's `id` must include the hovered state (see [`hover_state_key`]).
pub fn hover_bg_transition<E>(
    element: E,
    id: impl Into<ElementId>,
    base: Color,
    hover: Color,
    hovered: bool,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    hover_bg_transition_with_policy(element, id, base, hover, hovered, MotionPolicy::Normal)
}

/// Policy-aware hover background transition using the 120ms hover token.
///
/// Entering hover eases `base`→`hover`; leaving is applied immediately so a
/// newly mounted idle element cannot flash its hover tint. The caller should
/// include the hover state in `id` with [`hover_state_key`].
pub fn hover_bg_transition_with_policy<E>(
    element: E,
    id: impl Into<ElementId>,
    base: Color,
    hover: Color,
    hovered: bool,
    policy: MotionPolicy,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if !hovered {
        return element.bg(base).into_any_element();
    }

    color_transition_with_duration(element, id, base, hover, policy, DURATION_HOVER)
}

/// Fade an element in with the 180ms appear token.
///
/// This is an opacity-only entrance helper: it never changes layout or
/// translates a list item. `NoMotion` paints the element at full opacity.
pub fn appear_transition<E>(
    element: E,
    id: impl Into<ElementId>,
    policy: MotionPolicy,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    opacity_transition_with_duration(element, id, 0.0, 1.0, policy, DURATION_MEDIUM)
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_motion_tests.rs"]
mod tests;
