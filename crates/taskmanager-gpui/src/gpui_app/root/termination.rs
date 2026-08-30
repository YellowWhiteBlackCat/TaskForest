//! Shared confirmation flow for destructive process termination.
//!
//! Action-bar buttons, context-menu items, and the Delete shortcut all create a
//! application-owned [`ProcessTerminationConfirmation`] through `InteractionState`.
//! This module owns snapshot construction, the testable feedback adapter and
//! the confirmation dialog; the renderer never stores the frozen payload.

use super::{ProcessControlAction, RootView};
use crate::gpui_app::elements;
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement, Styled, Window, div, px, relative,
};
use taskmanager_application::i18n;
use taskmanager_application::{
    ConfirmationKind, PendingConfirmation, SurfaceDismissReason, SurfaceKind,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessItem, ProcessLiveKey, descendant_live_keys,
};
use taskmanager_theme::Theme;

use taskmanager_application::{ProcessTerminationAction, ProcessTerminationConfirmation};
use taskmanager_theme::tokens;

fn feedback_action(action: ProcessTerminationAction) -> ProcessControlAction {
    match action {
        ProcessTerminationAction::EndTask => ProcessControlAction::EndTask,
        ProcessTerminationAction::ForceKill => ProcessControlAction::Kill,
        ProcessTerminationAction::EndProcessTree => ProcessControlAction::EndProcessTree,
    }
}

fn dialog_title(action: ProcessTerminationAction) -> &'static str {
    match action {
        ProcessTerminationAction::EndTask => i18n::t("proc.confirm_end_title"),
        ProcessTerminationAction::ForceKill => i18n::t("proc.confirm_kill_title"),
        ProcessTerminationAction::EndProcessTree => i18n::t("proc.confirm_tree_title"),
    }
}

fn dialog_message(
    action: ProcessTerminationAction,
    name: &str,
    pid: u32,
    descendant_count: usize,
) -> String {
    let template = match action {
        ProcessTerminationAction::EndTask => i18n::t("proc.confirm_end_message"),
        ProcessTerminationAction::ForceKill => i18n::t("proc.confirm_kill_message"),
        ProcessTerminationAction::EndProcessTree => i18n::t("proc.confirm_tree_message"),
    };
    template
        .replace("{name}", name)
        .replace("{pid}", &pid.to_string())
        .replace("{count}", &descendant_count.to_string())
}

fn button_label(action: ProcessTerminationAction) -> &'static str {
    match action {
        ProcessTerminationAction::EndTask => i18n::t("proc.end_task"),
        ProcessTerminationAction::ForceKill => i18n::t("proc.kill"),
        ProcessTerminationAction::EndProcessTree => i18n::t("proc.end_process_tree"),
    }
}

pub(super) fn snapshot_single_process(
    action: ProcessTerminationAction,
    identity: ProcessLiveKey,
    procs: &[ProcessItem],
) -> Option<ProcessTerminationConfirmation> {
    let root = procs
        .iter()
        .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
        .and_then(FrozenProcessIdentity::from_process)?;
    Some(ProcessTerminationConfirmation {
        action,
        root,
        descendants_leaf_first: Vec::new(),
    })
}

/// Snapshot a root and every currently-known descendant without issuing process
/// control. The live-identity order comes from the ONE shared leaf-first
/// traversal (`core::process::descendant_live_keys`, the same walk
/// `freeze_tree` freezes); this dialog only adds its own name lookup for the
/// confirmation preview.
pub fn snapshot_process_tree(
    procs: &[ProcessItem],
    root: ProcessLiveKey,
) -> Option<ProcessTerminationConfirmation> {
    let closure = descendant_live_keys(procs, root);
    let (root_identity, descendant_identities) = match closure.split_last() {
        Some((root_identity, descendant_identities)) => (root_identity, descendant_identities),
        None => return None,
    };
    let root = procs
        .iter()
        .find(|process| ProcessLiveKey::from_process(process) == Some(*root_identity))
        .and_then(FrozenProcessIdentity::from_process)?;
    let descendants_leaf_first = descendant_identities
        .iter()
        .map(|identity| {
            procs
                .iter()
                .find(|process| ProcessLiveKey::from_process(process) == Some(*identity))
                .and_then(FrozenProcessIdentity::from_process)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ProcessTerminationConfirmation {
        action: ProcessTerminationAction::EndProcessTree,
        root,
        descendants_leaf_first,
    })
}

impl RootView {
    /// Open the shared process-termination confirmation dialog. Action bar,
    /// context menu, and Delete all call this method with the same typed intent.
    pub fn request_process_termination(
        &mut self,
        action: ProcessTerminationAction,
        identity: ProcessLiveKey,
    ) {
        if let Some(intent) = snapshot_single_process(action, identity, self.processes()) {
            self.arm_confirmation(PendingConfirmation::ProcessTermination(intent));
        }
    }

    /// Snapshot the selected process tree now; later refreshes cannot add, drop,
    /// or rename targets in the pending confirmation.
    pub fn request_process_tree_termination(&mut self, identity: ProcessLiveKey) {
        if let Some(intent) = snapshot_process_tree(self.processes(), identity) {
            self.arm_confirmation(PendingConfirmation::ProcessTermination(intent));
        }
    }

    /// Dismiss the pending confirmation without executing process control.
    pub fn cancel_process_termination(&mut self) {
        self.dismiss_shared_surface(
            SurfaceKind::Confirmation(ConfirmationKind::ProcessTermination),
            SurfaceDismissReason::Cancel,
        );
    }

    /// Consume and execute the application-emitted effect through an injected
    /// operation.
    /// Injection keeps the state machine testable without signaling a real PID;
    /// production passes this module's platform-neutral executor. Returns false
    /// when there was nothing to confirm.
    pub fn confirm_process_termination_with(
        &mut self,
        execute: impl FnOnce(taskmanager_application::PlatformEffect) -> Result<(), FailureKind>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(intent) = self.process_termination_confirmation().cloned() else {
            return false;
        };
        let Some(effect) = self.confirm_confirmation(ConfirmationKind::ProcessTermination) else {
            return false;
        };
        let feedback_action = feedback_action(intent.action);
        let Some(root_identity) = intent.root.live_key() else {
            return false;
        };
        let result = execute(effect);
        self.record_process_control_result(feedback_action, root_identity, result, cx);
        true
    }

    pub fn confirm_process_termination(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(effect) = self.confirm_confirmation(ConfirmationKind::ProcessTermination) else {
            return false;
        };
        self.dispatch_confirmed_effect(effect, cx);
        true
    }
}

/// Build the complete destructive-process confirmation dialog. Closing via
/// X/scrim and the Cancel button only clear pending state; the confirm button is
/// the sole UI path to `confirm_process_termination`.
pub(super) fn render_process_termination_dialog(
    theme: &Theme,
    intent: ProcessTerminationConfirmation,
    entity: Entity<RootView>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let title = dialog_title(intent.action);
    let message = dialog_message(
        intent.action,
        &intent.root.name,
        intent.root.pid,
        intent.descendant_count(),
    );
    let confirm_label = button_label(intent.action);
    let is_high_risk = intent.action != ProcessTerminationAction::EndTask;

    let close_entity = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close_entity.update(cx, |view, cx| {
            view.cancel_process_termination();
            cx.notify();
        });
    };
    let cancel_entity = entity.clone();
    let confirm_entity = entity;
    let mut content = div()
        .w(px(420.0))
        .flex()
        .flex_col()
        .gap(tokens::SPACE_14)
        .child(
            div()
                .text_size(tokens::FONT_13)
                .line_height(relative(1.45))
                .text_color(if is_high_risk { theme.danger } else { theme.fg })
                .child(message),
        );
    if intent.action == ProcessTerminationAction::EndProcessTree {
        let mut preview = div()
            .flex()
            .flex_col()
            .gap(tokens::SPACE_3)
            .p(tokens::SPACE_8)
            .rounded(tokens::control_radius(theme))
            .bg(theme.sidebar_card_bg)
            .child(
                div()
                    .text_size(tokens::FONT_11)
                    .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                    .text_color(theme.fg_dim)
                    .child(
                        i18n::t("proc.tree_descendants")
                            .replace("{count}", &intent.descendant_count().to_string()),
                    ),
            );
        for target in intent.descendants_leaf_first.iter().take(5) {
            preview = preview.child(
                div()
                    .text_size(tokens::FONT_12)
                    .text_color(theme.fg)
                    .child(format!("{} (PID {})", target.name, target.pid)),
            );
        }
        if intent.descendant_count() > 5 {
            preview = preview.child(
                div()
                    .text_size(tokens::FONT_11)
                    .text_color(theme.fg_dim)
                    .child(
                        i18n::t("proc.more_descendants")
                            .replace("{count}", &(intent.descendant_count() - 5).to_string()),
                    ),
            );
        }
        content = content.child(preview);
    }
    let content: AnyElement = content
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap(tokens::SPACE_8)
                .child(elements::pill(
                    theme,
                    "process-termination-cancel",
                    i18n::t("common.cancel"),
                    false,
                    false,
                    move |_window, cx| {
                        cancel_entity.update(cx, |view, cx| {
                            view.cancel_process_termination();
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "process-termination-confirm",
                    confirm_label,
                    true,
                    false,
                    move |_window, cx| {
                        confirm_entity.update(cx, |view, cx| {
                            view.confirm_process_termination(cx);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        )
        .into_any_element();

    elements::dialog_overlay(theme, window, cx, title, on_close, content).into_any_element()
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_termination_tests.rs"]
mod tests;
