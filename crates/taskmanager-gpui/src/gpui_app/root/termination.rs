//! GPUI adapter for the shared process confirmation payloads.
//!
//! The application owns the frozen `EndTask` identity and the frozen
//! `ProcessBatchIntent`. This module owns only the GPUI dialog for the former;
//! batch/tree/kill dialogs are rendered by `batch_process`. No GPUI-specific
//! termination DTO or target expansion lives here.

use super::RootView;
use crate::gpui_app::elements;
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement, Styled, Window, div, px, relative,
};
use taskmanager_application::i18n;
use taskmanager_application::{
    ConfirmationKind, PendingConfirmation, SurfaceDismissReason, SurfaceKind,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

impl RootView {
    /// Resolve one live identity and arm the application-owned EndTask
    /// confirmation. A stale or capability-unavailable row produces no effect.
    pub fn request_end_task_confirmation(&mut self, identity: ProcessLiveKey) {
        if !self.shell.process_control_capability_allowed() {
            return;
        }
        let Some(target) = self.frozen_process(identity) else {
            return;
        };
        self.arm_confirmation(PendingConfirmation::EndTask(target));
    }

    /// Freeze a process tree through the shell's shared target-expansion
    /// helper and arm the ordinary ProcessBatch confirmation branch.
    pub fn request_process_tree_end(&mut self, identity: ProcessLiveKey) {
        if !self.shell.process_control_capability_allowed() {
            return;
        }
        let Some(intent) = self.shell.process_tree_end_intent(identity) else {
            return;
        };
        self.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
    }

    /// Dismiss the shared EndTask confirmation without submitting work.
    pub fn cancel_end_task_confirmation(&mut self) {
        self.dismiss_shared_surface(
            SurfaceKind::Confirmation(ConfirmationKind::EndTask),
            SurfaceDismissReason::Cancel,
        );
    }

    /// Consume the matching EndTask confirmation and dispatch its one typed
    /// platform effect through the ordinary GPUI submission seam.
    pub fn confirm_end_task(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(effect) = self.confirm_confirmation(ConfirmationKind::EndTask) else {
            return false;
        };
        self.dispatch_confirmed_effect(effect, cx);
        true
    }
}

/// Build the GPUI EndTask dialog from the frozen application identity. Closing
/// via X/scrim and Cancel only dismisses the matching branch; Confirm is the
/// sole path that consumes the frozen effect.
pub(super) fn render_end_task_confirmation_dialog(
    theme: &Theme,
    target: FrozenProcessIdentity,
    entity: Entity<RootView>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let title = i18n::t("proc.confirm_end_title");
    let message = i18n::t("proc.confirm_end_message")
        .replace("{name}", &target.name)
        .replace("{pid}", &target.pid.to_string())
        .replace("{count}", "0");
    let confirm_label = i18n::t("proc.end_task");

    let close_entity = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close_entity.update(cx, |view, cx| {
            view.cancel_end_task_confirmation();
            cx.notify();
        });
    };
    let cancel_entity = entity.clone();
    let confirm_entity = entity;
    let content = div()
        .w(px(420.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_14,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .line_height(relative(1.45))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(message),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(elements::pill(
                    theme,
                    "end-task-confirmation-cancel",
                    i18n::t("common.cancel"),
                    false,
                    false,
                    move |_window, cx| {
                        cancel_entity.update(cx, |view, cx| {
                            view.cancel_end_task_confirmation();
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "end-task-confirmation-confirm",
                    confirm_label,
                    true,
                    false,
                    move |_window, cx| {
                        confirm_entity.update(cx, |view, cx| {
                            view.confirm_end_task(cx);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        )
        .into_any_element();

    elements::dialog_overlay(theme, window, cx, title, on_close, content).into_any_element()
}
