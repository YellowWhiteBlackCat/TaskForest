//! The Applications action menu: the [`ActionMenuContext`] instantiation for
//! the process table — the shared control verbs with the neutral scheduling
//! priority tiers (ARCH.md §8.1 语义平价律).
//!
//! The modal engine ([`crate::menu_modal`]) owns the open session, the
//! keyboard state machine, and overlay mounting. This module owns only the
//! Applications-specific facts: the verb list, the tier vocabulary with its
//! labels, and the freeze path. Priority is the one control surface every
//! frontend must offer in the same three-tier shape — the tier labels come
//! from the shell's single fold (`presentation::priority_tier_label`) and the
//! request is the neutral `ProcessBatchAction::SetPriority`, so this surface
//! never invents a nice number or a platform-specific priority word.

use taskmanager_application::AppAction;
use taskmanager_application::PlatformEffect;
use taskmanager_application::i18n::t;
use taskmanager_core::core::process::{PriorityTier, ProcessBatchAction, ProcessLiveKey};
use taskmanager_shell::presentation::process_batch_action_label;
use taskmanager_shell::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ProcessRowId, ShellApp,
};

use crate::menu_modal::{ActionMenuContext, MenuModal};
use crate::widgets::menu::{MenuItem, MenuSpec};

/// The shared control verbs, in the same display order as the TUI/GPUI
/// process menus: end task, end process tree, the suspend/resume pair, force
/// kill, then the three priority tiers — the full neutral
/// [`PriorityTier::ALL`] set, in canonical order. The platform adapter owns
/// tier → native (nice on Linux/macOS, priority class on Windows); nothing
/// here widens to raw numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcessMenuAction {
    EndTask,
    EndProcessTree,
    Suspend,
    Resume,
    Kill,
    Priority(PriorityTier),
}

/// The actions in display order.
const MENU_ACTIONS: [ProcessMenuAction; 8] = [
    ProcessMenuAction::EndTask,
    ProcessMenuAction::EndProcessTree,
    ProcessMenuAction::Suspend,
    ProcessMenuAction::Resume,
    ProcessMenuAction::Kill,
    ProcessMenuAction::Priority(PriorityTier::High),
    ProcessMenuAction::Priority(PriorityTier::Normal),
    ProcessMenuAction::Priority(PriorityTier::Low),
];

/// Localized label for one menu action. The three priority tiers route through
/// the shell's single tier→label fold so this menu names each tier exactly the
/// way every other frontend's menus, toasts, and confirmations do.
fn action_label(action: ProcessMenuAction) -> String {
    match action {
        ProcessMenuAction::EndTask => process_batch_action_label(ProcessBatchAction::End),
        ProcessMenuAction::EndProcessTree => t("proc.end_process_tree").to_owned(),
        ProcessMenuAction::Suspend => process_batch_action_label(ProcessBatchAction::Suspend),
        ProcessMenuAction::Resume => process_batch_action_label(ProcessBatchAction::Resume),
        ProcessMenuAction::Kill => process_batch_action_label(ProcessBatchAction::Kill),
        ProcessMenuAction::Priority(tier) => {
            taskmanager_shell::presentation::priority_tier_label(tier).to_owned()
        }
    }
}

/// The frozen target and the shell's availability projection at open time.
/// Travels inside the session so a list refresh between open and confirm cannot
/// retarget the verb; commit revalidates the same state at the shell boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessMenuCtx {
    pub(crate) identity: ProcessLiveKey,
    pub(crate) control_available: bool,
}

impl ActionMenuContext for ProcessMenuCtx {
    fn spec(&self) -> MenuSpec {
        MenuSpec {
            title: t("proc.actions").to_owned(),
            items: MENU_ACTIONS
                .iter()
                .map(|action| MenuItem {
                    label: action_label(*action),
                    enabled: self.control_available,
                })
                .collect(),
        }
    }

    fn commit(&self, pick: usize, shell: &mut ShellApp) -> Vec<PlatformEffect> {
        // Re-anchor the shell's semantic row onto the frozen identity first:
        // the shared end-task and batch reducers act on the shell's selection,
        // never on the menu's copy of it. An identity that left the projection
        // fails closed with the TUI's notice instead of acting on a neighbor.
        if !shell.select_row_id(ProcessRowId::Process(self.identity)) {
            shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "The frozen process is no longer in the list",
            );
            return Vec::new();
        }
        match MENU_ACTIONS[pick] {
            ProcessMenuAction::EndTask => {
                // Arms the shared EndTask gate; the confirm path submits.
                let _ = shell.apply_action(AppAction::RequestEndTask);
                Vec::new()
            }
            ProcessMenuAction::EndProcessTree => {
                shell.request_process_tree_end(self.identity);
                Vec::new()
            }
            ProcessMenuAction::Kill => shell
                .request_process_batch(ProcessBatchAction::Kill)
                .into_iter()
                .collect(),
            ProcessMenuAction::Suspend => shell
                .request_process_batch(ProcessBatchAction::Suspend)
                .into_iter()
                .collect(),
            ProcessMenuAction::Resume => shell
                .request_process_batch(ProcessBatchAction::Resume)
                .into_iter()
                .collect(),
            ProcessMenuAction::Priority(tier) => shell
                .request_process_batch(ProcessBatchAction::SetPriority(tier))
                .into_iter()
                .collect(),
        }
    }
}

/// The page's modal state resource.
pub(crate) type ProcessMenuModal = MenuModal<ProcessMenuCtx>;

/// Open the menu for the shell's selected row. The shell cursor is the same
/// "row N → process" authority the page's selection seams use, so a structural
/// or aggregate row without a live identity keeps the menu closed (fail-closed,
/// TUI `open_process_menu` parity).
pub(crate) fn open_for_selected(modal: &mut ProcessMenuModal, shell: &ShellApp) -> bool {
    let Some(process) = shell.visible_process_at(shell.selected) else {
        return false;
    };
    let Some(identity) = ProcessLiveKey::from_process(process) else {
        return false;
    };
    modal.open(ProcessMenuCtx {
        identity,
        control_available: shell.process_control_availability().is_ready(),
    })
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/process_menu.rs"]
mod tests;
