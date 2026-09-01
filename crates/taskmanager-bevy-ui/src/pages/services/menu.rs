//! The Services action menu: the [`ActionMenuContext`] instantiation for the
//! Services inventory — five shared verbs, committed into the shell's gate
//! through `select_service_control` + `RequestServiceControl`.
//!
//! The modal engine ([`crate::menu_modal`]) owns the open session, the
//! keyboard state machine, and overlay mounting. This module owns only the
//! Services-specific facts: the verb list, the freeze path, and the
//! open-attempt (bare Enter over a selected row, resolved through the
//! shell's `sorted_services` — the single "row N → target" authority).

use taskmanager_application::i18n::t;
use taskmanager_application::{AppAction, PlatformEffect};
use taskmanager_core::core::services::{ServiceAction, ServiceItem};

use taskmanager_shell::ShellApp;

use crate::menu_modal::{ActionMenuContext, MenuModal};
use crate::widgets::menu::{MenuItem, MenuSpec};

/// The five shared verbs, in the same display order as the TUI action menu.
pub(crate) const MENU_ACTIONS: [ServiceAction; 5] = [
    ServiceAction::Start,
    ServiceAction::Stop,
    ServiceAction::Restart,
    ServiceAction::Enable,
    ServiceAction::Disable,
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
}

/// The page's modal state resource.
pub(crate) type ServiceMenuModal = MenuModal<ServiceMenuCtx>;

/// Open the menu for one selected row, resolved through the shell's
/// `sorted_services`. Fails closed on an unknown or empty target.
pub(crate) fn open_for(
    modal: &mut ServiceMenuModal,
    shell: &ShellApp,
    target: &taskmanager_core::core::target::ServiceId,
) -> bool {
    let Some(service) = shell
        .sorted_services()
        .into_iter()
        .find(|s| &s.id == target)
    else {
        return false;
    };
    modal.open(ServiceMenuCtx(service.clone()))
}
