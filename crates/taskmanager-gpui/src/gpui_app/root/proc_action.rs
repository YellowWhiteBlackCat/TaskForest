//! Process-action dispatch: feedback toasts, hover, and the right-click
//! menu action fan-out (signals, termination, open-file, properties).

use gpui::{AppContext, Context, Entity};
use taskmanager_application::ProcessControlRequest;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessLiveKey, ProcessSignal,
};

use super::{Hover, ProcMenuAction, ProcessDetailsSection, RootView};
use crate::gpui_app::root::dispatch::apply_search_online;
use crate::gpui_app::root::process_feedback::ProcessControlAction;

/// A single-target control intent extracted from a process-menu action.
/// `Suspend`/`Resume` route through the neutral request vocabulary
/// (ARCH §8.1 语义完备律): the POSIX stop/continue signals are adapter
/// mapping details and must never be named by a UI tree
/// (`tests/logic/control_vocabulary_boundary.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuControlRequest {
    Suspend,
    Resume,
    Signal(ProcessSignal),
}

/// Compose the neutral control request and its feedback label once the frozen
/// target resolves. Pure so the menu vocabulary mapping is testable without a
/// platform client.
fn menu_control_submission(
    control: MenuControlRequest,
    target: FrozenProcessIdentity,
) -> (ProcessControlRequest, ProcessControlAction) {
    match control {
        MenuControlRequest::Suspend => (
            ProcessControlRequest::Suspend { target },
            ProcessControlAction::Suspend,
        ),
        MenuControlRequest::Resume => (
            ProcessControlRequest::Resume { target },
            ProcessControlAction::Resume,
        ),
        MenuControlRequest::Signal(signal) => (
            ProcessControlRequest::SendSignal { target, signal },
            ProcessControlAction::Signal(signal),
        ),
    }
}

impl RootView {
    pub(crate) fn show_local_feedback(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        use taskmanager_ui::overlays::toast::{ToastEvent, ToastKind, ToastState};
        let msg = msg.into();
        let kind = if msg.starts_with("\u{26a0} ") {
            ToastKind::Danger
        } else {
            ToastKind::Info
        };
        self.local_feedback_seq += 1;
        let toast = cx.new(|cx| ToastState::new(self.local_feedback_seq, msg, kind, cx));
        toast.update(cx, |toast, cx| {
            toast.arm_auto_dismiss(std::time::Duration::from_secs(6), cx);
        });
        self.local_feedback_subscription = Some(cx.subscribe(
            &toast,
            move |view: &mut Self,
                  _toast: Entity<taskmanager_ui::overlays::toast::ToastState>,
                  _: &ToastEvent,
                  cx: &mut Context<Self>| {
                view.local_feedback_toast = None;
                cx.notify();
            },
        ));
        self.local_feedback_toast = Some(toast);
        cx.notify();
    }
    pub(crate) fn set_hover(&mut self, h: Option<Hover>, cx: &mut Context<Self>) {
        if self.hovered != h {
            self.hovered = h;
            cx.notify();
        }
    }

    /// Dispatch a typed process-menu action ([`ProcMenuAction`]) and close the menu.
    /// Replaces the old magic-`u16`-id `apply_menu_item`: `build_proc_menu` now attaches
    /// a typed [`ProcMenuAction`] to each item, so the builder and this dispatch agree
    /// by type rather than by keeping two `0..=8` match arms in sync.
    pub(crate) fn apply_proc_action(
        &mut self,
        identity: ProcessLiveKey,
        action: ProcMenuAction,
        cx: &mut Context<Self>,
    ) {
        // Clear any previous transient process-action feedback so the toast is
        // replaced (or hidden) by this action rather than lingering across actions.
        self.local_feedback_toast = None;

        // Non-control actions are handled before the control dispatch.
        let control = match action {
            // Properties: open the typed process-details surface for the
            // context-menu target (or dismiss it when the target is absent).
            ProcMenuAction::Properties => {
                self.open_process_details(identity, ProcessDetailsSection::Overview);
                cx.notify();
                return;
            }
            // Open file location: submit the selected process's frozen identity to
            // the resource-reveal facet. Provider-specific path resolution and
            // desktop integration stay outside GPUI.
            ProcMenuAction::OpenLocation => {
                self.request_reveal_process(identity, cx);
                cx.notify();
                return;
            }
            // Search online: dispatched to the body in `dispatch.rs`
            // (apply_search_online), which sets `local_feedback_toast` for failure cases.
            ProcMenuAction::SearchOnline => {
                apply_search_online(self, identity, cx);
                cx.notify();
                return;
            }
            // Win11-TM "Copy" submenu: write the respective ProcessItem field to the
            // system clipboard via gpui's App::write_to_clipboard (ClipboardItem is
            // re-exported by `gpui::*`; cx: &mut Context<RootView> derefs to &App).
            // A transient local_feedback_toast toast confirms what was copied (or reports
            // there was nothing to copy — e.g. the item vanished between ticks).
            ProcMenuAction::CopyName => {
                let (text, what) = self.copy_field(identity, |i| i.name.clone(), "name");
                self.finish_copy(text, what, cx);
                return;
            }
            ProcMenuAction::CopyPid => {
                let (text, what) = self.copy_field(identity, |i| i.pid.to_string(), "PID");
                self.finish_copy(text, what, cx);
                return;
            }
            ProcMenuAction::CopyCmdline => {
                let (text, what) = self.copy_field(identity, |i| i.cmdline.clone(), "command line");
                self.finish_copy(text, what, cx);
                return;
            }
            ProcMenuAction::EndTask => {
                self.request_end_task_confirmation(identity);
                cx.notify();
                return;
            }
            ProcMenuAction::EndProcessTree => {
                self.request_process_tree_end(identity);
                cx.notify();
                return;
            }
            ProcMenuAction::Kill => {
                if self
                    .processes()
                    .iter()
                    .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
                {
                    self.select_process_single(identity);
                    self.request_process_batch(ProcessBatchAction::Kill, cx);
                }
                cx.notify();
                return;
            }
            ProcMenuAction::Suspend => Some(MenuControlRequest::Suspend),
            ProcMenuAction::Resume => Some(MenuControlRequest::Resume),
            ProcMenuAction::Signal(s) => Some(MenuControlRequest::Signal(s)),
        };
        if self.shell.process_control_capability_allowed()
            && let Some(control) = control
            && let Some(target) = self.frozen_process(identity)
        {
            let (request, feedback_action) = menu_control_submission(control, target);
            self.submit_process_control(request, feedback_action, identity, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_proc_action_tests.rs"]
mod tests;
