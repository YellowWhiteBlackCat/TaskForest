//! Clipboard copy feedback and process-failure messaging for the root shell.

use super::RootView;
use crate::core::process::ProcessItem;
use crate::i18n;
use gpui::{ClipboardItem, Context};
use taskmanager_application::FailureKind;

impl RootView {
    /// Look up the process captured by the menu action and apply `f` to its
    /// [`ProcessItem`] to produce the text to copy. Returns `(None, what)` when
    /// no pid is selected or the item has vanished since the menu opened, so the
    /// caller can surface a "nothing to copy" toast. `what` is the lowercase noun
    /// used in the feedback line ("name" / "PID" / "command line").
    pub(super) fn copy_field(
        &self,
        pid: u32,
        f: impl Fn(&ProcessItem) -> String,
        what: &'static str,
    ) -> (Option<String>, &'static str) {
        let text = self.processes().iter().find(|p| p.pid == pid).map(f);
        (text, what)
    }

    /// Write `text` to the system clipboard (when `Some`) and set a transient
    /// `local_feedback_toast` toast confirming the copy (or reporting there was nothing
    /// to copy). `cx` derefs to `&App`, so `cx.write_to_clipboard` targets the
    /// gpui platform clipboard directly — the real write path in gpui 0.2.2
    /// (`App::write_to_clipboard` + `ClipboardItem::new_string`).
    pub(super) fn finish_copy(
        &mut self,
        text: Option<String>,
        what: &'static str,
        cx: &mut Context<Self>,
    ) {
        match text {
            Some(t) => {
                cx.write_to_clipboard(ClipboardItem::new_string(t.clone()));
                self.show_local_feedback(format!("{} {what}: {t}", i18n::t("hint.copied")), cx);
            }
            None => {
                self.show_local_feedback(
                    i18n::t("hint.no_process_to_copy").replace("{}", what),
                    cx,
                );
            }
        }
        cx.notify();
    }
}

pub fn process_failure_message(kind: FailureKind) -> String {
    match kind {
        // RequiresEscalation is an escalatable denial; fold into the denial text.
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => "permission denied",
        FailureKind::IdentityChanged => "process does not exist or identity changed",
        FailureKind::Unsupported => "process control is not supported",
        FailureKind::MissingDependency => "process provider unavailable",
        FailureKind::TimedOut => "process control timed out",
        FailureKind::TemporarilyUnavailable => "process provider unavailable",
        FailureKind::Rejected => "process control rejected",
        FailureKind::ProviderFault => "process provider failed",
    }
    .to_string()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_clipboard_tests.rs"]
mod tests;
