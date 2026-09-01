//! The Sessions action menu: the [`ActionMenuContext`] instantiation for the
//! Sessions inventory — Disconnect/Lock, committed into the shell's gate via
//! `select_session_control` (which freezes the provider-issued session
//! identity, the action, and a fresh correlation id).

use taskmanager_application::PlatformEffect;
use taskmanager_application::i18n::t;
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::target::SessionId;

use taskmanager_shell::ShellApp;

use crate::menu_modal::{ActionMenuContext, MenuModal};
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
}

/// The page's modal state resource.
pub(crate) type SessionMenuModal = MenuModal<SessionMenuCtx>;

/// Open the menu for one selected session, resolved through the shell's
/// `sorted_sessions` (the single "row → target" authority).
pub(crate) fn open_for(modal: &mut SessionMenuModal, shell: &ShellApp, target: &SessionId) -> bool {
    let Some(session) = shell
        .sorted_sessions()
        .into_iter()
        .find(|session| session.id == *target)
    else {
        return false;
    };
    modal.open(SessionMenuCtx(session.clone()))
}
