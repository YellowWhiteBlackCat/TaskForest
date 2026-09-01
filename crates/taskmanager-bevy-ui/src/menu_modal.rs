//! The generic frontend-local action-menu modal, shared by every inventory
//! page that offers control verbs (Services, Startup, Sessions).
//!
//! **Architecture**: a page instantiates [`MenuModal<Ctx>`] with its own
//! frozen-target type ([`ActionMenuContext`]). The engine owns the open
//! session, the clamped keyboard state machine, and the overlay mount/
//! despawn; the page owns only the menu spec, how a verb freezes into the
//! shell's shared confirmation gate ([`ActionMenuContext::commit`]), and how
//! a closed menu's open-attempt resolves a page selection into a frozen
//! target.
//!
//! **Modal precedence** is enforced by [`crate::input`]: the shell's armed
//! gate always wins; an open modal swallows every other key (TUI parity — a
//! frontend-local modal owns the keyboard chord-for-chord); with the menu
//! closed, the page's open-attempt (bare Enter over a selected row) runs
//! before the shell's free bindings.
//!
//! Mount/despawn flows through [`MenuModalChanged`], fired by the caller on
//! session transitions.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::keyboard::KeyCode;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, BorderRadius, FlexDirection, JustifyContent, Node, PositionType,
    UiRect, Val, percent, px,
};
use taskmanager_application::PlatformEffect;
use taskmanager_shell::ShellApp;

use crate::widgets::menu::{MenuInput, MenuSpec, MenuState, menu_scene_at};
use crate::window::{AppShellRoot, WindowPalette};

/// The per-page contract a modal menu instantiates: what the menu shows and
/// how a picked verb freezes into the shell's shared confirmation gate.
pub(crate) trait ActionMenuContext: Clone + Send + Sync + 'static {
    /// The menu entries (labels + enablement), in display order.
    fn spec(&self) -> MenuSpec;
    /// Freeze the picked verb (the confirmed entry index, into
    /// [`MenuSpec::items`]) into the shell's shared gate, returning the
    /// platform effects the caller must queue for the drain. Mirrors the TUI
    /// flow: the surfaces that only arm the gate return nothing — the
    /// platform request comes from the gate's typed confirm path — while a
    /// verb that submits through the shell's batch track carries its own
    /// [`PlatformEffect`] (`ExecuteBatch`) when no confirmation is needed.
    fn commit(&self, pick: usize, shell: &mut ShellApp) -> Vec<PlatformEffect>;
}

/// One open modal: the frozen target context plus the keyboard cursor.
pub(crate) struct MenuSession<Ctx> {
    pub(crate) frozen: Ctx,
    pub(crate) state: MenuState,
}

/// The per-page modal state: at most one open session. A `Resource` so each
/// instantiation lives in the `World` and the input adapter can drive it.
#[derive(bevy::ecs::resource::Resource)]
pub(crate) struct MenuModal<Ctx> {
    pub(crate) session: Option<MenuSession<Ctx>>,
}

impl<Ctx> Default for MenuModal<Ctx> {
    fn default() -> Self {
        // No Ctx: Default bound — an empty modal has no frozen target.
        Self { session: None }
    }
}

impl<Ctx: ActionMenuContext> MenuModal<Ctx> {
    /// Open with a frozen target. Returns whether the state changed (callers
    /// fire the mount event on a transition).
    pub(crate) fn open(&mut self, frozen: Ctx) -> bool {
        if self.session.is_some() {
            return false;
        }
        self.session = Some(MenuSession {
            frozen,
            state: MenuState::default(),
        });
        true
    }

    /// Close without any shell effect. Returns whether the state changed.
    /// Drive an open menu with one key. Returns `true` when the key was
    /// consumed — an open modal swallows EVERY key it does not use, so a
    /// frontend-local modal can never leak a chord to navigation or the
    /// shell. Confirm freezes the picked verb into the shell's gate; any
    /// platform effect the freeze produced is appended to `effects`.
    pub(crate) fn drive(
        &mut self,
        shell: &mut ShellApp,
        key: KeyCode,
        effects: &mut Vec<PlatformEffect>,
    ) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let input = match key {
            KeyCode::ArrowUp => Some(MenuInput::Up),
            KeyCode::ArrowDown => Some(MenuInput::Down),
            KeyCode::Enter => Some(MenuInput::Confirm),
            KeyCode::Escape => Some(MenuInput::Cancel),
            _ => None,
        };
        let Some(input) = input else {
            return true;
        };
        let spec = session.frozen.spec();
        match session.state.advance(&spec.items, input) {
            Some(crate::widgets::menu::MenuOutcome::Confirmed(index)) => {
                let frozen = session.frozen.clone();
                self.session = None;
                effects.extend(frozen.commit(index, shell));
                true
            }
            Some(crate::widgets::menu::MenuOutcome::Canceled) => {
                self.session = None;
                true
            }
            None => true,
        }
    }
}

/// Publishes modal transitions: `true` mounts the overlay, `false` despawns.
/// One event type per context (Bevy observers are typed).
#[derive(Event)]
pub(crate) struct MenuModalChanged<Ctx>(pub(crate) bool, pub(crate) std::marker::PhantomData<Ctx>);

impl<Ctx> Clone for MenuModalChanged<Ctx> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Ctx> Copy for MenuModalChanged<Ctx> {}

/// Type-erased modal driver so the input seam can route keys through every
/// instantiated menu without knowing the frozen-target types. An open modal
/// swallows every key (TUI modal parity).
pub(crate) trait ModalDriver {
    fn is_open(&self) -> bool;
    fn drive(
        &mut self,
        shell: &mut ShellApp,
        key: KeyCode,
        effects: &mut Vec<PlatformEffect>,
    ) -> bool;
}

impl<Ctx: ActionMenuContext> ModalDriver for MenuModal<Ctx> {
    fn is_open(&self) -> bool {
        self.session.is_some()
    }

    fn drive(
        &mut self,
        shell: &mut ShellApp,
        key: KeyCode,
        effects: &mut Vec<PlatformEffect>,
    ) -> bool {
        MenuModal::<Ctx>::drive(self, shell, key, effects)
    }
}

impl<Ctx: ActionMenuContext> ModalDriver for bevy::ecs::system::ResMut<'_, MenuModal<Ctx>> {
    fn is_open(&self) -> bool {
        self.session.is_some()
    }

    fn drive(
        &mut self,
        shell: &mut ShellApp,
        key: KeyCode,
        effects: &mut Vec<PlatformEffect>,
    ) -> bool {
        MenuModal::<Ctx>::drive(self, shell, key, effects)
    }
}

/// Marker on the one mounted overlay. Deliberately non-generic: at most one
/// modal is open at a time (opening one closes the others), so a single
/// marker is unambiguous and every context's observer can despawn it.
#[derive(Component, Clone, Default)]
pub(crate) struct MenuModalOverlay;

/// Observer: mount/despawn the overlay under the app shell root. One
/// registration per context (see [`register`]).
fn on_menu_modal_changed<Ctx: ActionMenuContext>(
    changed: On<MenuModalChanged<Ctx>>,
    palette: Option<Res<WindowPalette>>,
    roots: Query<Entity, With<AppShellRoot>>,
    overlays: Query<Entity, With<MenuModalOverlay>>,
    modal: ResMut<MenuModal<Ctx>>,
    mut commands: Commands,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    if !changed.event().0 {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let Ok(root) = roots.single() else {
        return;
    };
    if modal.session.is_none() {
        return;
    }
    let overlay = commands
        .spawn_scene(menu_overlay_scene(&modal, &palette.inner))
        .id();
    commands.entity(root).add_one_related::<ChildOf>(overlay);
}

/// Register one modal's resource and observers on the app composition.
pub(crate) fn register<Ctx: ActionMenuContext>(app: &mut bevy::app::App) {
    app.init_resource::<MenuModal<Ctx>>();
    app.add_observer(on_menu_modal_changed::<Ctx>);
}

/// Centered modal card over a dim scrim — the same staging every
/// destructive-adjacent surface uses, so ownership reads identically.
fn menu_overlay_scene<Ctx: ActionMenuContext>(
    modal: &MenuModal<Ctx>,
    palette: &crate::palette::UiPalette,
) -> Box<dyn Scene> {
    let Some(session) = modal.session.as_ref() else {
        // Unreachable through the mount observer; the empty body keeps the
        // builder total without inventing menu content.
        return Box::new(bsn! { Node { width: percent(100) } }) as Box<dyn Scene>;
    };
    let spec = session.frozen.spec();
    let card = menu_scene_at(&spec, &session.state, palette);
    let scrim = palette.scrim;
    let radius = palette.panel_radius_px;
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        BackgroundColor({ scrim })
        MenuModalOverlay
        Children [
            (
                Node {
                    width: px(360.0),
                    height: Val::Auto,
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(crate::palette::space_16())),
                    border_radius: BorderRadius::all(Val::Px(radius)),
                }
                BackgroundColor({ palette.panel_fill })
                Children [
                    ( { card } ),
                ]
            ),
        ]
    }) as Box<dyn Scene>
}
