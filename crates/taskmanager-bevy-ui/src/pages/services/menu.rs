//! The Services action menu: the [`ActionMenuContext`] instantiation for the
//! Services inventory — five shared verbs, committed into the shell's gate
//! through `select_service_control` + `RequestServiceControl`.
//!
//! The menu engine ([`crate::menu_modal`]) owns the open session, the
//! keyboard state machine, and the anchored popover. This module owns only
//! the Services-specific facts: the verb list, the freeze path, the
//! open-attempt (bare Enter over a selected row, resolved through the
//! shell's `sorted_services` — the single "row N → target" authority), and
//! the control the menu floats against.
//!
//! **The trigger**: the page publishes its own surface with
//! [`MenuModalTrigger`], so the popover is that surface's CHILD — the menu
//! floats over the page it acts on and the page stays live. The surface is
//! a popover, not a blocking modal (GPUI parity).

use bevy::app::App;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::system::{Commands, ResMut};
use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, PlatformEffect};
use taskmanager_core::core::services::{ServiceAction, ServiceItem};

use taskmanager_shell::ShellApp;

use crate::menu_modal::{ActionMenuContext, MenuModal, MenuModalTrigger};
use crate::scene_attach::insert_into;
use crate::widgets::menu::{MenuItem, MenuSpec};
use taskmanager_core::core::target::ServiceId;

/// The six shared verbs, in the same display order as the TUI action menu.
pub(crate) const MENU_ACTIONS: [ServiceAction; 6] = [
    ServiceAction::Start,
    ServiceAction::Stop,
    ServiceAction::Restart,
    ServiceAction::Enable,
    ServiceAction::Disable,
    ServiceAction::ReloadDaemon,
];

/// The frozen target: one provider-issued service row. Travels inside the
/// session so a refresh between open and confirm cannot retarget it.
#[derive(Clone)]
pub(crate) struct ServiceMenuCtx(pub(crate) ServiceItem);

impl ActionMenuContext for ServiceMenuCtx {
    fn spec(&self) -> MenuSpec {
        MenuSpec {
            title: t("svc.service_actions").to_owned(),
            items: MENU_ACTIONS
                .iter()
                .map(|action| MenuItem {
                    label: crate::confirmation::service_action_label(*action).to_owned(),
                    enabled: true,
                })
                .collect(),
        }
    }

    fn commit(&self, pick: usize, shell: &mut ShellApp) -> Vec<PlatformEffect> {
        if shell.select_service_control(&self.0, MENU_ACTIONS[pick]) {
            // Arms the shared gate; the platform request comes from the gate's
            // typed confirm path, so there is no effect to queue here.
            let _ = shell.apply_action(AppAction::RequestServiceControl);
        }
        Vec::new()
    }

    /// The Services page publishes its own surface as the menu's trigger,
    /// so the popover floats against the page it acts on.
    fn register(app: &mut App) {
        app.add_observer(on_services_inventory_added);
    }
}

/// Publish the action-menu trigger on the mounted page surface. The marker
/// is a typed component, so the popover mounts as that surface's child and
/// the inventory behind it stays visible and interactive.
fn on_services_inventory_added(
    add: On<Add<super::ServicesPageRoot>>,
    mut commands: Commands,
    mut modal: ResMut<MenuModal<ServiceMenuCtx>>,
) {
    let entity = add.event().entity;
    insert_into(&mut commands, entity, MenuModalTrigger);
    // The anchor is the page's OWN state, so a route change cannot
    // leave the menu floating against the page that is on its way out.
    modal.set_trigger(entity);
}

/// The page's menu state resource.
pub(crate) type ServiceMenuModal = MenuModal<ServiceMenuCtx>;

/// Open the menu for one selected row, resolved through the shell's
/// `sorted_services`. Fails closed on an unknown or empty target.
pub(crate) fn open_for(modal: &mut ServiceMenuModal, shell: &ShellApp, target: &ServiceId) -> bool {
    let Some(service) = shell
        .sorted_services()
        .into_iter()
        .find(|s| &s.id == target)
    else {
        return false;
    };
    modal.open(ServiceMenuCtx(service.clone()))
}
