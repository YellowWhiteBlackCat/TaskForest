//! Pointer-hover explanation tooltip overlay surface.
//!
//! Interactive controls and key metrics can attach [`TooltipText`] so pointer
//! hover reveals a viewport-fixed explanation card. This surface is declared
//! `Divergent`: it has no keyboard-focus trigger, though the overlay is
//! pointer-transparent and supports click/Escape dismissal.
//!
//! # Architecture
//!
//! Governed by an explicit discrete state machine ([`TooltipState`]):
//! `None` when inactive, `Some(session)` when an interactive element is
//! hovered.
//! State transitions emit [`TooltipStateChanged`] so the observer updates
//! the overlay without frame-by-frame polling.
//!
//! # The hover delay (BEVY-176)
//!
//! A tooltip that answers the pointer instantly is a tooltip that flickers
//! along a moving cursor. The card therefore waits [`TOOLTIP_DELAY`] before
//! it becomes *visible*: the card is mounted with the hover (so it can be
//! measured for the viewport fit) but stays hidden until the pointer has
//! settled on the same target for the delay.
//!
//! # The viewport fit (BEVY-176)
//!
//! The card is placed against its anchor, flipped above it when the
//! viewport's bottom edge would clip it, and clamped inside the viewport
//! horizontally. The placement is a pure function of the measured geometry
//! ([`resolve_placement`]), so the headless test asserts the *resolved*
//! position rather than the drawing.

use std::time::Duration;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::MessageReader;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Changed, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::picking::hover::PickingInteraction;
use bevy::prelude::Visibility;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, ComputedNode, FixedNode, JustifyContent, Node,
    UiGlobalTransform, UiRect, Val, percent, px,
};
use bevy::ui::widget::Text;

use crate::palette::{UiPalette, space_4, space_8};
use crate::scene_attach::attach_child;
use crate::window::{AppShellRoot, Role, TextRole, WindowPalette};

/// The hover delay before the card becomes visible.
pub(crate) const TOOLTIP_DELAY: Duration = Duration::from_millis(300);

/// Active session of an element hover tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TooltipSession {
    pub(crate) target: Entity,
    pub(crate) text: String,
}

/// Explicit state machine resource for tooltip display:
/// `None` when hidden, `Some(session)` when visible.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TooltipState {
    pub(crate) session: Option<TooltipSession>,
    dismissed_target: Option<Entity>,
    /// The target the current hover rests on.
    hovered_target: Option<Entity>,
    /// The moment the current hover began — the delay clock's origin.
    hovered_since: Option<Duration>,
    /// Whether the hover has lasted long enough for the card to show.
    ready: bool,
}

impl TooltipState {
    #[cfg_attr(not(test), allow(dead_code))]
    #[must_use]
    pub(crate) fn is_visible(&self) -> bool {
        self.session.is_some()
    }

    /// Whether the delay has elapsed and the card may be shown.
    #[must_use]
    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }

    pub(crate) fn show(&mut self, target: Entity, text: String) {
        self.session = Some(TooltipSession { target, text });
    }

    pub(crate) fn hide(&mut self) {
        self.session = None;
        self.ready = false;
        self.hovered_target = None;
        self.hovered_since = None;
    }

    /// Begin (or continue) a hover on `target` at the clock reading `now`,
    /// and answer how long the pointer has been over it.
    fn begin_hover(&mut self, target: Entity, now: Duration) -> Duration {
        if self.hovered_target != Some(target) {
            self.hovered_target = Some(target);
            self.hovered_since = Some(now);
            self.ready = false;
            return Duration::ZERO;
        }
        now.saturating_sub(self.hovered_since.unwrap_or(now))
    }

    /// The hover clock's origin, if a hover is running.
    #[must_use]
    fn hovered_since(&self) -> Option<Duration> {
        self.hovered_since
    }
}

/// Event triggered when tooltip session opens or closes.
#[derive(Event, Clone, Copy, Debug)]
pub(crate) struct TooltipStateChanged;

/// Component attached to interactive entities that offer a hover tooltip.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TooltipText(pub(crate) String);

/// Marker component on the mounted tooltip visual card.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct TooltipOverlay;

/// The card's resolved placement inside the viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TooltipPlacement {
    /// The card's top-left corner, in viewport coordinates (y down).
    pub(crate) origin: Vec2,
    /// Whether the card was flipped above its anchor.
    pub(crate) flipped: bool,
}

/// Resolve the card's placement against the viewport.
///
/// The card prefers to sit *below* its anchor. When the viewport's bottom
/// edge would clip it, the card flips above the anchor instead; when
/// neither side fits, it is clamped to the viewport's bottom edge. The
/// horizontal position starts at the anchor's left edge and is clamped
/// inside the viewport, so a card never hangs off the side.
#[must_use]
pub(crate) fn resolve_placement(
    anchor: Rect,
    card: Vec2,
    viewport: Vec2,
    gap: f32,
) -> TooltipPlacement {
    let below = anchor.max.y + gap;
    let above = anchor.min.y - gap - card.y;
    let flipped = below + card.y > viewport.y && above >= 0.0;
    let y = if flipped {
        above
    } else {
        below.min((viewport.y - card.y).max(0.0))
    };
    let x = anchor.min.x.clamp(0.0, (viewport.x - card.x).max(0.0));
    TooltipPlacement {
        origin: Vec2::new(x, y),
        flipped,
    }
}

/// Scene for the hovering explanation card.
///
/// The card is viewport-anchored: the placement system writes its resolved
/// position every frame, so the card follows its anchor and never leaves
/// the viewport.
pub(crate) fn tooltip_card_scene(text: &str, palette: &UiPalette) -> impl Scene + use<> {
    let radius = palette.control_radius_px;
    let label = text.to_string();
    bsn! {
        Node {
            // Viewport-anchored hover card: offsets resolve against the
            // camera viewport, immune to any future shell layout reflow.
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            min_width: px(0.0),
            max_width: percent(90),
            padding: UiRect::axes(Val::Px(space_8()), Val::Px(space_4())),
            border_radius: BorderRadius::all(Val::Px(radius)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        FixedNode
        BackgroundColor({ palette.panel_fill })
        TooltipOverlay
        Pickable::IGNORE
        Children [
             Text(label) TextRole(Role::Caption) TextLayout { linebreak: LineBreak::WordOrCharacter }
             Node { min_width: px(0.0), max_width: percent(100) } Pickable::IGNORE
        ]
    }
}

/// System tracking hover interactions on entities with [`TooltipText`].
pub(crate) fn tooltip_hover_interaction_system(
    mut state: ResMut<TooltipState>,
    query: Query<(Entity, &TooltipText, &PickingInteraction), Changed<PickingInteraction>>,
    mut keyboard: MessageReader<KeyboardInput>,
    time: Res<bevy::time::Time>,
    mut commands: Commands,
) {
    let mut changed = false;
    for input in keyboard.read() {
        if input.state == ButtonState::Pressed
            && input.key_code == KeyCode::Escape
            && let Some(target) = state.session.as_ref().map(|session| session.target)
        {
            state.dismiss(target);
            changed = true;
        }
    }
    for (entity, text, interaction) in &query {
        match interaction {
            PickingInteraction::Hovered => {
                if state.dismissed_target != Some(entity) {
                    // The delay clock restarts whenever the pointer lands on
                    // a new target; the card stays hidden until it elapses.
                    state.begin_hover(entity, time.elapsed());
                    let next = TooltipSession {
                        target: entity,
                        text: text.0.clone(),
                    };
                    if state.session.as_ref() != Some(&next) {
                        state.show(entity, text.0.clone());
                        changed = true;
                    }
                    state.ready = false;
                }
            }
            PickingInteraction::None => {
                if state.dismissed_target == Some(entity) {
                    state.dismissed_target = None;
                }
                if state.session.as_ref().map(|s| s.target) == Some(entity) {
                    state.hide();
                    changed = true;
                }
            }
            PickingInteraction::Pressed => {
                if state.session.as_ref().map(|s| s.target) == Some(entity) {
                    state.dismiss(entity);
                    changed = true;
                }
            }
        }
    }
    if changed {
        commands.trigger(TooltipStateChanged);
    }
}

/// System that promotes a settled hover to a visible card.
///
/// The card is mounted with the hover — so its size is measurable for the
/// viewport fit — but it stays hidden until the pointer has rested on the
/// same target for [`TOOLTIP_DELAY`].
pub(crate) fn tooltip_delay_system(mut state: ResMut<TooltipState>, time: Res<bevy::time::Time>) {
    let Some(session) = state.session.clone() else {
        return;
    };
    if state.is_ready() {
        return;
    }
    let Some(since) = state.hovered_since() else {
        return;
    };
    if time.elapsed().saturating_sub(since) >= TOOLTIP_DELAY {
        state.ready = true;
    }
    let _ = session;
}

/// System that places the mounted card against its anchor and inside the
/// viewport.
pub(crate) fn tooltip_placement_system(
    state: Res<TooltipState>,
    palette: Res<WindowPalette>,
    roots: Query<(&ComputedNode, &UiGlobalTransform), With<AppShellRoot>>,
    targets: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut cards: Query<(&mut Node, &mut Visibility, &ComputedNode), With<TooltipOverlay>>,
) {
    let ready = state.is_ready();
    let session = state.session.clone();
    for (mut node, mut visibility, card) in &mut cards {
        // The card is mounted with the hover, but it stays hidden until the
        // pointer has rested on the same target for the delay.
        let next = if ready {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != next {
            *visibility = next;
        }
        let Some(session) = session.as_ref() else {
            continue;
        };
        let Ok((root, root_transform)) = roots.single() else {
            continue;
        };
        let Ok((target, target_transform)) = targets.get(session.target) else {
            continue;
        };
        let scale = root.inverse_scale_factor;
        let viewport = root.size() * scale;
        let target_size = target.size() * scale;
        let center = target_transform.translation * scale;
        let root_center = root_transform.translation * scale;
        // The layout's vertical axis runs down while the world's runs up.
        let anchor_center = Vec2::new(
            center.x - root_center.x + viewport.x * 0.5,
            viewport.y * 0.5 - (center.y - root_center.y),
        );
        let anchor = Rect::from_corners(
            anchor_center - target_size * 0.5,
            anchor_center + target_size * 0.5,
        );
        let card_size = card.size() * scale;
        let placement = resolve_placement(anchor, card_size, viewport, space_4());
        node.left = Val::Px(placement.origin.x);
        node.top = Val::Px(placement.origin.y);
    }
    let _ = palette;
}

impl TooltipState {
    fn dismiss(&mut self, target: Entity) {
        self.dismissed_target = Some(target);
        if self.session.as_ref().map(|session| session.target) == Some(target) {
            self.session = None;
            self.ready = false;
        }
    }
}

fn on_tooltip_state_changed(
    _changed: On<TooltipStateChanged>,
    state: Option<Res<TooltipState>>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<TooltipOverlay>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    let Some(state) = state else {
        return;
    };
    let Some(ref session) = state.session else {
        return;
    };
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    let scene = tooltip_card_scene(&session.text, &palette.inner);
    let overlay = commands.spawn_scene(scene).id();
    attach_child(&mut commands, root, overlay);
}

/// Register tooltip state, systems, and observer.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<TooltipState>();
    app.add_observer(on_tooltip_state_changed);
    app.add_systems(bevy::app::Update, tooltip_hover_interaction_system);
    app.add_systems(bevy::app::Update, tooltip_delay_system);
    app.add_systems(
        bevy::app::PostUpdate,
        tooltip_placement_system.before(bevy::ui::UiSystems::Layout),
    );
}

#[cfg(test)]
#[path = "../tests/headless/tooltip.rs"]
mod tests;
