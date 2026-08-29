//! The Startup action menu: the [`ActionMenuContext`] instantiation for the
//! Startup inventory — Enable/Disable, committed into the shell's gate via
//! `request_startup_control_for` (which arms the shared confirmation and
//! freezes the exact provider-issued entry).

use taskmanager_application::i18n::t;
use taskmanager_core::core::startup::StartupEntry;
use taskmanager_core::core::startup::StartupEntryId;
use taskmanager_shell::ShellApp;

use crate::menu_modal::{ActionMenuContext, MenuModal};
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

    fn commit(&self, pick: usize, shell: &mut ShellApp) {
        let enabled = MENU_ACTIONS_ENABLED[pick];
        let _ = shell.request_startup_control_for(self.0.clone(), enabled);
    }
}

/// The page's modal state resource.
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
