//! The Sessions action menu: the [`ActionMenuContext`] instantiation for the
//! Sessions inventory — Disconnect/Lock, committed into the shell's gate via
//! `select_session_control` (which freezes the provider-issued session
//! identity, the action, and a fresh correlation id).
//!
//! **The trigger**: the page publishes its own surface with
//! [`MenuModalTrigger`], so the popover is that surface's CHILD — the menu
//! floats over the page it acts on and the page stays live. The surface is
//! a popover, not a blocking modal (GPUI parity).

use bevy::app::App;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, ResMut};
use taskmanager_application::PlatformEffect;
use taskmanager_application::i18n::t;
use taskmanager_core::core::session::{SessionControlAction, SessionIdentity, SessionItem};

use taskmanager_shell::ShellApp;

use crate::menu_modal::{ActionMenuContext, MenuModal, MenuModalTrigger};
use crate::scene_attach::insert_into;
use crate::widgets::menu::{MenuItem, MenuSpec};

/// The two shared verbs, in display order; the picked index maps onto the
/// action (Disconnect → `Disconnect`).
const MENU_ACTIONS: [SessionControlAction; 2] =
    [SessionControlAction::Disconnect, SessionControlAction::Lock];

/// The frozen target: one provider-issued login session.
#[derive(Clone)]
pub(crate) struct SessionMenuCtx(pub(crate) SessionItem);

impl ActionMenuContext for SessionMenuCtx {
    fn spec(&self) -> MenuSpec {
        MenuSpec {
            title: t("users.session_actions").to_owned(),
            items: vec![
                MenuItem {
                    label: t("users.disconnect").to_owned(),
                    enabled: true,
                },
                MenuItem {
                    label: t("users.lock").to_owned(),
                    enabled: true,
                },
            ],
        }
    }

    fn commit(&self, pick: usize, shell: &mut ShellApp) -> Vec<PlatformEffect> {
        // Freezes the target only; the platform request comes from the gate's
        // typed confirm path, so there is no effect to queue here.
        let _ = shell.select_session_control(&self.0, MENU_ACTIONS[pick]);
        Vec::new()
    }

    /// The Sessions page publishes its own surface as the menu's trigger,
    /// so the popover floats against the page it acts on.
    fn register(app: &mut App) {
        app.add_observer(on_sessions_inventory_added);
    }
}

/// Publish the action-menu trigger on the mounted page surface. The marker
/// is a typed component, so the popover mounts as that surface's child and
/// the inventory behind it stays visible and interactive.
fn on_sessions_inventory_added(
    add: On<Add<super::SessionsPageRoot>>,
    mut commands: Commands,
    mut modal: ResMut<MenuModal<SessionMenuCtx>>,
) {
    let entity = add.event().entity;
    insert_into(&mut commands, entity, MenuModalTrigger);
    // The anchor is the page's OWN state, so a route change cannot
    // leave the menu floating against the page that is on its way out.
    modal.set_trigger(entity);
}

/// The page's menu state resource.
pub(crate) type SessionMenuModal = MenuModal<SessionMenuCtx>;

/// Open the menu for one selected session, resolved through the shell's
/// `sorted_sessions` (the single "row → target" authority).
pub(crate) fn open_for(
    modal: &mut SessionMenuModal,
    shell: &ShellApp,
    target: &SessionIdentity,
) -> bool {
    let rows = super::session_rows(shell);
    if rows
        .iter()
        .filter(|row| row.identity.as_ref() == Some(target))
        .count()
        != 1
    {
        return false;
    }
    let Some(session) = shell
        .sorted_sessions()
        .into_iter()
        .find(|session| session.identity().as_ref() == Some(target))
    else {
        return false;
    };
    modal.open(SessionMenuCtx(session.clone()))
}
