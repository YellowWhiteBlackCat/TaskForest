//! The Startup action menu: the [`ActionMenuContext`] instantiation for the
//! Startup inventory — Enable/Disable, committed into the shell's gate via
//! `request_startup_control_for` (which arms the shared confirmation and
//! freezes the exact provider-issued entry).
//!
//! **The trigger**: the page publishes its own surface with
//! [`MenuModalTrigger`], so the popover is that surface's CHILD — the menu
//! floats over the page it acts on and the page stays live. The surface is
//! a popover, not a blocking modal (GPUI parity).

use bevy::app::App;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, ResMut};
use taskforest_core::core::startup::StartupEntry;
use taskforest_core::core::startup::StartupEntryId;
use taskmanager_application::PlatformEffect;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;

use crate::menu_modal::{ActionMenuContext, MenuModal, MenuModalTrigger};
use crate::scene_attach::insert_into;
use crate::widgets::menu::{MenuItem, MenuSpec};

/// The two shared verbs, in display order; the picked index maps onto the
/// enabled flag (Enable → `true`).
const MENU_ACTIONS_ENABLED: [bool; 2] = [true, false];

/// The frozen target: one provider-issued startup entry.
#[derive(Clone)]
pub(crate) struct StartupMenuCtx(pub(crate) StartupEntry);

impl ActionMenuContext for StartupMenuCtx {
    fn spec(&self) -> MenuSpec {
        MenuSpec {
            title: t("startup.applications").to_owned(),
            items: vec![
                MenuItem {
                    label: t("startup.enable").to_owned(),
                    enabled: true,
                },
                MenuItem {
                    label: t("startup.disable").to_owned(),
                    enabled: true,
                },
            ],
        }
    }

    fn commit(&self, pick: usize, shell: &mut ShellApp) -> Vec<PlatformEffect> {
        let enabled = MENU_ACTIONS_ENABLED[pick];
        // Arms the shared gate; the platform request comes from the gate's
        // typed confirm path, so there is no effect to queue here.
        let _ = shell.request_startup_control_for(self.0.clone(), enabled);
        Vec::new()
    }

    /// The Startup page publishes its own surface as the menu's trigger,
    /// so the popover floats against the page it acts on.
    fn register(app: &mut App) {
        app.add_observer(on_startup_inventory_added);
    }
}

/// Publish the action-menu trigger on the mounted page surface. The marker
/// is a typed component, so the popover mounts as that surface's child and
/// the inventory behind it stays visible and interactive.
fn on_startup_inventory_added(
    add: On<Add<super::StartupPageRoot>>,
    mut commands: Commands,
    mut modal: ResMut<MenuModal<StartupMenuCtx>>,
) {
    let entity = add.event().entity;
    insert_into(&mut commands, entity, MenuModalTrigger);
    // The anchor is the page's OWN state, so a route change cannot
    // leave the menu floating against the page that is on its way out.
    modal.set_trigger(entity);
}

/// The page's menu state resource.
pub(crate) type StartupMenuModal = MenuModal<StartupMenuCtx>;

/// Open the menu for one selected entry, resolved through the shell's
/// `sorted_startup_entries` (the single "row → target" authority).
pub(crate) fn open_for(
    modal: &mut StartupMenuModal,
    shell: &ShellApp,
    target: &StartupEntryId,
) -> bool {
    let Some(entry) = shell
        .sorted_startup_entries()
        .into_iter()
        .find(|entry| &entry.id == target)
    else {
        return false;
    };
    modal.open(StartupMenuCtx(entry.clone()))
}
