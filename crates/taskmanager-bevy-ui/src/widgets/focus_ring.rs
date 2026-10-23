//! The product-owned focus ring layer (BEVY-179).
//!
//! The keyboard focus ring is **two layers**: an inner 2px highlight that
//! marks the focused control, and an outer 1px translucent halo that lifts
//! the ring off the surface it sits on. The inner highlight is the engine's
//! `Outline` (the focus owner's seam); this module owns the *outer halo*
//! and the geometry both layers share.
//!
//! The halo is a single overlay node in the app shell — never a child of
//! the focused control, so it can never disturb the control's own layout,
//! clipping or stacking. The system places it over the focused control's
//! outlined box: the halo band sits immediately outside the inner
//! highlight, which is what makes the two read as one ring.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res};
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::prelude::Visibility;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, BorderColor, Node, PositionType, UiRect, Val};
use bevy::ui::{ComputedNode, UiGlobalTransform};

use crate::focus_visible::{FocusRing, InputModalityState};
use crate::palette::{UiPalette, space_1, space_2};
use crate::window::{AppShellRoot, WindowPalette};
use bevy::input_focus::InputFocus;

/// The focus ring's two layers: the inner highlight and the outer halo.
///
/// The widths are the theme's hairline scale (`space_2` = 2px, `space_1` =
/// 1px): a hairline contract, not a type-ladder step, so it stays put when
/// the reading size changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FocusRingLayers {
    /// The inner highlight's width, in pixels.
    pub(crate) inner_width: f32,
    /// The outer halo's width, in pixels.
    pub(crate) halo_width: f32,
    /// The inner highlight's ink.
    pub(crate) inner_ink: Color,
    /// The outer halo's ink — the same hue, translucent.
    pub(crate) halo_ink: Color,
}

/// The halo's alpha: strong enough to read as a glow, weak enough to stay
/// a halo.
const HALO_ALPHA: f32 = 0.35;

/// The focus ring's layers for the live palette.
#[must_use]
/// The focus ring's inner outline width.
///
/// The widths ride the theme's hairline scale and do NOT depend on the
/// palette's inks, so a caller can read them without the palette resource.
pub(crate) fn focus_ring_inner_width() -> f32 {
    space_2()
}

pub(crate) fn focus_ring_layers(palette: &UiPalette) -> FocusRingLayers {
    let mut halo = palette.focus_ring.to_srgba();
    halo.alpha = (halo.alpha * HALO_ALPHA).clamp(0.0, 1.0);
    FocusRingLayers {
        inner_width: space_2(),
        halo_width: space_1(),
        inner_ink: palette.focus_ring,
        halo_ink: Color::srgba(halo.red, halo.green, halo.blue, halo.alpha),
    }
}

/// The overlay node that paints the focus ring's outer halo.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FocusRingHalo;

/// The halo overlay scene: an empty box whose only ink is its 1px border.
///
/// The scene is declared in the app shell (one node for the whole window);
/// the focus system moves and resizes it, and hides it when no control
/// holds the keyboard focus.
pub(crate) fn focus_ring_halo_scene(palette: &UiPalette) -> impl Scene + use<> {
    let layers = focus_ring_layers(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(0.0),
            height: Val::Px(0.0),
            border: UiRect::all(Val::Px(layers.halo_width)),
        }
        BackgroundColor({ Color::NONE })
        BorderColor {
            left: { layers.halo_ink },
            top: { layers.halo_ink },
            right: { layers.halo_ink },
            bottom: { layers.halo_ink },
        }
        FocusRingHalo
        Pickable::IGNORE
        Visibility::Hidden
    }
}

/// The halo's placement inside the shell root, in logical pixels.
///
/// The control's world centre is measured in physical pixels; the shell
/// root's box is the window, and the layout's vertical axis runs down
/// while the world's runs up — so the y axis is mirrored here. The result
/// is scaled back into logical pixels, which is the unit `Val::Px`
/// resolves in.
#[must_use]
pub(crate) fn halo_origin(
    control_center: Vec2,
    root_center: Vec2,
    root_size: Vec2,
    halo_size: Vec2,
    inverse_scale: f32,
) -> Vec2 {
    let x = (control_center.x - root_center.x) + root_size.x * 0.5 - halo_size.x * 0.5;
    let y = (root_center.y - control_center.y) + root_size.y * 0.5 - halo_size.y * 0.5;
    Vec2::new(x * inverse_scale, y * inverse_scale)
}

/// Place the halo over the focused control, or hide it.
///
/// The system runs before the UI layout, so the halo's box is resolved in
/// the same frame the focus moves. The control's own measurements are the
/// previous frame's — the standard one-frame trade-off, and the one the
/// engine's own focus ring lives with.
pub(crate) fn sync_focus_ring_halo(
    focus: Res<InputFocus>,
    modality: Res<InputModalityState>,
    palette: Res<WindowPalette>,
    roots: Query<(&ComputedNode, &UiGlobalTransform), With<AppShellRoot>>,
    controls: Query<(&ComputedNode, &UiGlobalTransform), With<FocusRing>>,
    mut halos: Query<(&mut Node, &mut Visibility), With<FocusRingHalo>>,
) {
    let layers = focus_ring_layers(&palette.inner);
    for (mut node, mut visibility) in &mut halos {
        let placed = place_halo(
            focus.get(),
            modality.shows_focus_ring(),
            &layers,
            &roots,
            &controls,
            &mut node,
        );
        let next = if placed {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != next {
            *visibility = next;
        }
    }
}

/// Compute the halo's box for the focused control.
fn place_halo(
    focused: Option<bevy::ecs::entity::Entity>,
    keyboard: bool,
    layers: &FocusRingLayers,
    roots: &Query<(&ComputedNode, &UiGlobalTransform), With<AppShellRoot>>,
    controls: &Query<(&ComputedNode, &UiGlobalTransform), With<FocusRing>>,
    node: &mut Node,
) -> bool {
    let Some(focused) = focused else {
        return false;
    };
    if !keyboard {
        return false;
    }
    let Ok((control, control_transform)) = controls.get(focused) else {
        return false;
    };
    let Ok((root, root_transform)) = roots.single() else {
        return false;
    };
    // The halo band sits immediately outside the inner highlight, which the
    // engine draws around the control's outlined box.
    let outlined = control.outlined_node_size();
    let halo_size = outlined + Vec2::splat(layers.halo_width * 2.0);
    let origin = halo_origin(
        control_transform.translation,
        root_transform.translation,
        root.size(),
        halo_size,
        root.inverse_scale_factor,
    );
    let halo_width = layers.halo_width * root.inverse_scale_factor;
    node.left = Val::Px(origin.x);
    node.top = Val::Px(origin.y);
    node.width = Val::Px(halo_size.x * root.inverse_scale_factor);
    node.height = Val::Px(halo_size.y * root.inverse_scale_factor);
    let _ = halo_width;
    true
}
