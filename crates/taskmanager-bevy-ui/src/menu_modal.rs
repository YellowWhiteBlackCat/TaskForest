//! The generic frontend-local action menu: an ANCHORED POPOVER shared by
//! every inventory page that offers control verbs (Services, Startup,
//! Sessions, Applications).
//!
//! **Architecture**: a page instantiates `MenuModal<Ctx>` with its own
//! frozen-target type (`ActionMenuContext`). The engine owns the open
//! session, the clamped keyboard state machine, and the popover mount /
//! despawn; the page owns only the menu spec, how a verb freezes into the
//! shell's shared confirmation gate (`ActionMenuContext::commit`), and the
//! control the menu floats against (`ActionMenuContext::register`).
//!
//! **The surface is a popover, not a modal** (GPUI parity): the menu is a
//! CHILD of its trigger and carries Bevy's own anchored-popup placement, so
//! the inventory behind it stays visible and live while the user picks a
//! verb. There is NO scrim and NO full-screen catcher: a press outside the
//! menu simply reaches the background — the GPUI reference forwards that
//! press to the background target — and the menu closes in the same frame.
//!
//! **Keyboard precedence** is enforced by [`crate::input`]: the shell's
//! armed gate always wins; an open menu swallows every other key (TUI
//! parity — a frontend-local menu owns the keyboard chord-for-chord); with
//! the menu closed, the page's open-attempt (bare Enter over a selected row)
//! runs before the shell's free bindings.
//!
//! Mount/despawn flows through `MenuModalChanged`, fired by the caller on
//! session transitions.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::input::keyboard::KeyCode;
use bevy::picking::events::PointerPress;
use bevy::prelude::Visibility;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::ui::prelude::{FlexDirection, GlobalZIndex, Node, OverrideClip, PositionType, Val};
use bevy::ui_widgets::popover::{Popover, PopoverAlign, PopoverPlacement, PopoverSide};
use taskmanager_application::PlatformEffect;
use taskmanager_shell::ShellApp;

use crate::app::ContentSlot;
use crate::scene_attach::{attach_child, insert_into};
use crate::widgets::menu::{MenuInput, MenuItemIndex, MenuSpec, MenuState, menu_scene_at};
use crate::window::{AppShellRoot, WindowPalette};

/// The per-page contract a menu instantiates: what the menu shows and how a
/// picked verb freezes into the shell's shared confirmation gate.
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

    /// Register the page's own menu wiring.
    ///
    /// The engine owns the popover; the page owns the CONTROL the menu
    /// floats against — it publishes that control into the menu's own state
    /// with [`MenuModal::set_trigger`] — plus any page-specific observer. A
    /// page that publishes no trigger leaves the menu anchored to the
    /// window's content region, which still floats and still never blocks.
    ///
    /// The trigger lives in the menu's STATE, not in a world query: during a
    /// route change the outgoing and the incoming page roots coexist for one
    /// frame, so "the first trigger in the world" could anchor the menu to
    /// the page that is on its way out.
    fn register(_app: &mut bevy::app::App) {}
}

/// One open menu: the frozen target context plus the keyboard cursor.
pub(crate) struct MenuSession<Ctx> {
    pub(crate) frozen: Ctx,
    pub(crate) state: MenuState,
}

/// The per-page menu state: at most one open session. A `Resource` so each
/// instantiation lives in the `World` and the input adapter can drive it.
#[derive(bevy::ecs::resource::Resource)]
pub(crate) struct MenuModal<Ctx> {
    pub(crate) session: Option<MenuSession<Ctx>>,
    /// The surfaces hidden this frame; they are despawned at the END of it.
    pending_despawn: Vec<Entity>,
    /// The control the popover floats against, published by the page.
    ///
    /// The anchor is STATE, not a world scan: a route change keeps the
    /// outgoing and the incoming page roots alive for one frame, and
    /// "the first trigger in the world" would then anchor the menu to the
    /// page that is leaving.
    trigger: Option<Entity>,
}

impl<Ctx> Default for MenuModal<Ctx> {
    fn default() -> Self {
        // No Ctx: Default bound — an empty menu has no frozen target.
        Self {
            session: None,
            pending_despawn: Vec::new(),
            trigger: None,
        }
    }
}

impl<Ctx: ActionMenuContext> MenuModal<Ctx> {
    /// Publish the control the popover floats against.
    ///
    /// The page calls this when it mounts its surface, so the anchor always
    /// names the page that is CURRENTLY on screen.
    pub(crate) fn set_trigger(&mut self, entity: Entity) {
        self.trigger = Some(entity);
    }

    /// The published anchor, if it is still alive.
    pub(crate) fn trigger(&self) -> Option<Entity> {
        self.trigger
    }

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
    /// consumed — an open menu swallows EVERY key it does not use, so a
    /// frontend-local menu can never leak a chord to navigation or the
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

/// Publishes menu transitions: `true` mounts the popover, `false` retires
/// it. One event type per context (Bevy observers are typed).
#[derive(Event)]
pub(crate) struct MenuModalChanged<Ctx>(pub(crate) bool, pub(crate) std::marker::PhantomData<Ctx>);

impl<Ctx> Clone for MenuModalChanged<Ctx> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Ctx> Copy for MenuModalChanged<Ctx> {}

/// Type-erased menu driver so the input seam can route keys through every
/// instantiated menu without knowing the frozen-target types. An open menu
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

/// Marker on the one mounted popover. Deliberately non-generic: at most one
/// menu is open at a time (opening one closes the others), so a single
/// marker is unambiguous and every context's observer can retire it.
#[derive(Component, Clone, Default)]
pub(crate) struct MenuModalPopover;

/// Marker on the control an action menu floats against. The page publishes
/// it on the surface its menu belongs to (the inventory table block); the
/// popover is mounted as that control's CHILD, so the menu travels with the
/// page and never covers it.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct MenuModalTrigger;

/// The menu surface's shared reads: the window shell root, the window's
/// content region, the page-published trigger and the mounted popover.
/// Bundled so the mount observer stays inside the argument budget.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MenuSurface<'w, 's> {
    pub(crate) roots: Query<'w, 's, Entity, With<AppShellRoot>>,
    pub(crate) slots: Query<'w, 's, Entity, With<ContentSlot>>,
    pub(crate) triggers: Query<'w, 's, Entity, With<MenuModalTrigger>>,
    pub(crate) popovers: Query<'w, 's, Entity, With<MenuModalPopover>>,
}

/// Observer: mount the anchored popover under its trigger, or retire the
/// previous one. One registration per context (see [`register`]).
fn on_menu_modal_changed<Ctx: ActionMenuContext>(
    changed: On<MenuModalChanged<Ctx>>,
    palette: Option<Res<WindowPalette>>,
    surface: MenuSurface,
    mut modal: ResMut<MenuModal<Ctx>>,
    mut commands: Commands,
) {
    // The previous surface is HIDDEN now and despawned at the end of the
    // frame: a rebuild never hands a vanished child to an observer.
    for entity in &surface.popovers {
        insert_into(&mut commands, entity, Visibility::Hidden);
        modal.pending_despawn.push(entity);
    }
    if !changed.event().0 {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let Some(session) = modal.session.as_ref() else {
        return;
    };
    let Ok(root) = surface.roots.single() else {
        return;
    };
    // The trigger: the control the page published into this menu's own
    // state, else the window's content region, else the shell root. The
    // menu floats in every case — it never blocks the page behind it.
    //
    // The published anchor must still be a live trigger surface: a route
    // change despawns the outgoing page's surface, and a dead anchor would
    // take the popover with it.
    let published = modal
        .trigger()
        .filter(|entity| surface.triggers.contains(*entity));
    let trigger = published
        .or_else(|| surface.slots.iter().next())
        .unwrap_or(root);
    let popover = commands
        .spawn_scene(menu_popover_scene(session, &palette.inner))
        .id();
    attach_child(&mut commands, trigger, popover);
}

/// Observer: handle click activation on a menu item.
fn on_menu_item_activated<Ctx: ActionMenuContext>(
    activate: On<bevy::ui_widgets::Activate>,
    items: Query<&MenuItemIndex>,
    mut modal: ResMut<MenuModal<Ctx>>,
    track: Option<bevy::ecs::system::NonSendMut<crate::app::FrontendTrack>>,
    pending: Option<ResMut<crate::input::PendingEffects>>,
    mut commands: Commands,
) {
    let Ok(item) = items.get(activate.event().entity) else {
        return;
    };
    let Some(session) = modal.session.as_mut() else {
        return;
    };
    let index = item.0;
    let spec = session.frozen.spec();
    if index >= spec.items.len() || !spec.items[index].enabled {
        return;
    }
    let frozen = session.frozen.clone();
    modal.session = None;
    if let Some(mut track) = track {
        let effects = frozen.commit(index, &mut track.shell);
        if let Some(mut pending) = pending {
            pending.0.extend(effects);
        }
    }
    commands.trigger(MenuModalChanged::<Ctx>(false, std::marker::PhantomData));
    commands.trigger(crate::input::ShellInteractionApplied);
}

/// Observer: a press OUTSIDE the open menu dismisses it — and the press
/// still reaches the background.
///
/// The GPUI reference forwards the outside press to the background target
/// (the Iced `DropdownMenu` captures that press instead — the recorded
/// difference between the two surfaces). The menu therefore needs NO scrim
/// and NO full-screen catcher: the press lands on the page behind the menu,
/// and the menu closes in the same frame.
fn on_press_outside_the_menu<Ctx: ActionMenuContext>(
    press: On<PointerPress>,
    popovers: Query<&MenuModalPopover>,
    parents: Query<&ChildOf>,
    mut modal: ResMut<MenuModal<Ctx>>,
    mut commands: Commands,
) {
    if modal.session.is_none() {
        return;
    }
    if is_inside_the_popover(&popovers, &parents, press.entity) {
        return;
    }
    modal.session = None;
    commands.trigger(MenuModalChanged::<Ctx>(false, std::marker::PhantomData));
    commands.trigger(crate::input::ShellInteractionApplied);
}

/// Whether the pressed entity belongs to the open popover (a menu row is
/// inside it; the page behind it is not).
fn is_inside_the_popover(
    popovers: &Query<&MenuModalPopover>,
    parents: &Query<&ChildOf>,
    entity: Entity,
) -> bool {
    let mut current = entity;
    for _ in 0..16 {
        if popovers.contains(current) {
            return true;
        }
        let Ok(parent) = parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
    false
}

/// The surfaces hidden this frame are despawned at the END of it, so every
/// observer of this frame has already run against the live child and a
/// rebuild can never hand one a vanished entity.
fn flush_pending_despawns<Ctx: ActionMenuContext>(
    mut modal: ResMut<MenuModal<Ctx>>,
    mut commands: Commands,
) {
    for entity in modal.pending_despawn.drain(..) {
        // `try_despawn` never panics on an entity that is already gone.
        commands.entity(entity).try_despawn();
    }
}

/// Register one menu's resource and observers on the app composition.
pub(crate) fn register<Ctx: ActionMenuContext>(app: &mut bevy::app::App) {
    app.init_resource::<MenuModal<Ctx>>();
    app.add_observer(on_menu_modal_changed::<Ctx>);
    app.add_observer(on_menu_item_activated::<Ctx>);
    app.add_observer(on_press_outside_the_menu::<Ctx>);
    app.add_systems(bevy::app::Last, flush_pending_despawns::<Ctx>);
    Ctx::register(app);
}

/// The anchored popover: the menu card, positioned against its trigger by
/// Bevy's own popover placement (the same shape the column chooser uses).
/// No scrim, no full-screen catcher — the page behind stays live.
///
/// The menu hangs off the trigger's TRAILING edge: the inventory's control
/// column is where the verbs belong, so the menu opens where the reference
/// surface opens its row menus. The second placement flips the menu above
/// the trigger when the window edge leaves no room below.
fn menu_popover_scene<Ctx: ActionMenuContext>(
    session: &MenuSession<Ctx>,
    palette: &crate::palette::UiPalette,
) -> Box<dyn Scene> {
    let spec = session.frozen.spec();
    let card = menu_scene_at(&spec, &session.state, palette);
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            width: Val::Auto,
            height: Val::Auto,
            flex_direction: FlexDirection::Column,
        }
        OverrideClip
        GlobalZIndex({ 100 })
        MenuModalPopover
        Popover {
            positions: { vec![
                PopoverPlacement { side: PopoverSide::Bottom, align: PopoverAlign::End, gap: 4.0 },
                PopoverPlacement { side: PopoverSide::Top, align: PopoverAlign::End, gap: 4.0 },
            ] },
            window_margin: { 8.0_f32 },
        }
        Children [
             @{ card }
        ]
    }) as Box<dyn Scene>
}

#[cfg(test)]
#[path = "../tests/headless/menu_popover_tests.rs"]
mod tests;
