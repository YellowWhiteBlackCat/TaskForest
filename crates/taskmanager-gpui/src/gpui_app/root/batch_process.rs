//! Confirmation and result plumbing for typed multi-process actions.

use super::{RootView, platform_submission_time_ms};
use crate::core::process::{
    ProcessBatchAction, ProcessBatchHistory, ProcessBatchHistoryExportError,
    ProcessBatchHistoryFormat, ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult,
    descendant_pids, export_process_batch_history,
};
use crate::gpui_app::elements;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, IntoElement, ParentElement, ScrollHandle,
    Styled, Window, div, px,
};
use taskmanager_application::{
    ConfirmationKind, FailureKind, PendingConfirmation, ProcessControlRequest, RefreshRequest,
    SurfaceDismissReason, SurfaceKind,
};
use taskmanager_ui::layout::{BoundedScrollRailSpec, bounded_scroll_region_with_rail};

fn action_label(action: ProcessBatchAction) -> &'static str {
    match action {
        ProcessBatchAction::End => i18n::t("proc.end_task"),
        ProcessBatchAction::Kill => i18n::t("proc.kill"),
        ProcessBatchAction::Suspend => i18n::t("proc.suspend"),
        ProcessBatchAction::Resume => i18n::t("proc.resume"),
        // Shared tier mapping (求同): the confirmation and the result toast
        // agree, and the tier word is the honest cross-platform phrasing.
        ProcessBatchAction::SetPriority(tier) => super::process_feedback::priority_tier_label(tier),
    }
}

fn result_summary(result: &ProcessBatchResult) -> String {
    let (applied, skipped, failed) = result_counts(result);
    let summary = i18n::t("proc.batch_result")
        .replace("{applied}", &applied.to_string())
        .replace("{skipped}", &skipped.to_string())
        .replace("{failed}", &failed.to_string());

    let mut uniform_failure = None;
    for failure in result
        .targets
        .iter()
        .filter_map(|(_, status)| match status {
            ProcessBatchTargetResult::Failed(failure) => Some(*failure),
            _ => None,
        })
    {
        match uniform_failure {
            None => uniform_failure = Some(failure),
            Some(existing) if existing == failure => {}
            Some(_) => return summary,
        }
    }

    uniform_failure.map_or(summary.clone(), |failure| {
        format!(
            "{summary}: {}",
            i18n::t(process_batch_failure_feedback_key(failure))
        )
    })
}

fn result_counts(result: &ProcessBatchResult) -> (usize, usize, usize) {
    let applied = result.applied_count();
    let skipped = result
        .targets
        .iter()
        .filter(|(_, status)| {
            matches!(
                status,
                ProcessBatchTargetResult::IdentityChanged
                    | ProcessBatchTargetResult::IdentityUnavailable
                    | ProcessBatchTargetResult::Failed(FailureKind::IdentityChanged)
            )
        })
        .count();
    let failed = result.targets.len().saturating_sub(applied + skipped);
    (applied, skipped, failed)
}

const fn process_batch_failure_feedback_key(failure: FailureKind) -> &'static str {
    match failure {
        // RequiresEscalation is an escalatable denial; fold into the denial key.
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            "feedback.permission_denied"
        }
        FailureKind::IdentityChanged => "feedback.process_gone",
        FailureKind::Unsupported => "feedback.unsupported",
        FailureKind::MissingDependency | FailureKind::TemporarilyUnavailable => {
            "health.failure_provider_unavailable"
        }
        FailureKind::TimedOut => "feedback.timed_out",
        FailureKind::Rejected => "feedback.request_rejected",
        FailureKind::ProviderFault => "feedback.provider_failed",
    }
}

/// Consume a completed worker result into bounded audit history and return the
/// existing localized toast summary. `RootView::poll_process_batch_result` can
/// call this once its root-owned history field is present.
pub fn record_completed_process_batch(
    history: &mut ProcessBatchHistory,
    completed_at_unix_ms: u64,
    result: ProcessBatchResult,
) -> String {
    let summary = result_summary(&result);
    history.record_result(completed_at_unix_ms, result);
    summary
}

/// Serialize history and pass it to an injected non-file sink. A GPUI caller
/// can supply `App::write_to_clipboard`; tests can capture the exact payload.
/// No filesystem access or blocking work occurs on the UI thread.
pub fn export_process_batch_history_with(
    history: &ProcessBatchHistory,
    format: ProcessBatchHistoryFormat,
    write: impl FnOnce(String),
) -> Result<usize, ProcessBatchHistoryExportError> {
    let payload = export_process_batch_history(history, format)?;
    let byte_count = payload.len();
    write(payload);
    Ok(byte_count)
}

impl RootView {
    pub fn selected_process_pids(&self) -> Vec<u32> {
        self.selected_application_root().map_or_else(
            || self.shell.selection.batch_targets(),
            |root| descendant_pids(self.processes(), root),
        )
    }

    /// Freeze PID/name/start-time now. A later refresh cannot change what the
    /// confirmation represents.
    pub fn request_process_batch(&mut self, action: ProcessBatchAction) {
        let intent = self.selected_application_root().map_or_else(
            || ProcessBatchIntent::freeze(self.processes(), self.selected_process_pids(), action),
            |root| ProcessBatchIntent::freeze_tree(self.processes(), root, action),
        );
        if !intent.targets.is_empty() {
            self.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
        }
    }

    pub(crate) fn submit_process_batch_immediate(
        &mut self,
        action: ProcessBatchAction,
        pid: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let intent = ProcessBatchIntent::freeze(self.processes(), [pid], action);
        if intent.targets.is_empty() {
            return false;
        }
        self.submit_process_batch_intent(intent, cx);
        true
    }

    /// Cancel and Escape only clear UI state; no worker request occurs.
    pub fn cancel_process_batch(&mut self) {
        self.dismiss_shared_surface(
            SurfaceKind::Confirmation(ConfirmationKind::ProcessBatch),
            SurfaceDismissReason::Cancel,
        );
    }

    pub fn confirm_process_batch(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(effect) = self.confirm_confirmation(ConfirmationKind::ProcessBatch) else {
            return false;
        };
        self.dispatch_confirmed_effect(effect, cx)
    }

    pub(crate) fn submit_process_batch_intent(
        &mut self,
        intent: ProcessBatchIntent,
        cx: &mut Context<Self>,
    ) {
        let attempt = self.shell.begin_process_batch(intent.clone());
        let result = self.platform.as_mut().map_or_else(
            || Err(FailureKind::TemporarilyUnavailable),
            |platform| {
                platform
                    .submit_process_control(
                        ProcessControlRequest::ExecuteBatch(intent),
                        platform_submission_time_ms(),
                    )
                    .map_err(super::process_control::submission_failure_kind)
            },
        );
        match result {
            Ok(request_id) => {
                self.shell.accept_process_batch(attempt, request_id);
                self.show_local_feedback(i18n::t("proc.batch_queued").to_string(), cx);
            }
            Err(kind) => {
                self.shell.reject_process_batch(attempt, kind);
                self.show_local_feedback(
                    i18n::t("health.failure_provider_unavailable").to_string(),
                    cx,
                );
            }
        }
    }

    pub(crate) fn accept_process_batch_result(
        &mut self,
        result: ProcessBatchResult,
        cx: &mut Context<Self>,
    ) {
        let feedback = record_completed_process_batch(
            &mut self.process_batch_history,
            platform_submission_time_ms(),
            result,
        );
        self.show_local_feedback(feedback, cx);
        self.request_refresh(RefreshRequest::Processes);
    }

    /// Copy the complete bounded audit history as stable JSON. Clipboard I/O
    /// is the only side effect; no filesystem or process operation is involved.
    pub fn copy_process_batch_history(&mut self, cx: &mut Context<Self>) {
        let result = export_process_batch_history_with(
            &self.process_batch_history,
            ProcessBatchHistoryFormat::Json,
            |payload| cx.write_to_clipboard(ClipboardItem::new_string(payload)),
        );
        self.show_local_feedback(
            match result {
                Ok(_) => format!(
                    "{}: {}",
                    i18n::t("hint.copied"),
                    i18n::t("proc.batch_history")
                ),
                Err(error) => format!("{}: {error}", i18n::t("proc.batch_history_export_failed")),
            },
            cx,
        );
        cx.notify();
    }
}

pub(super) fn render_process_batch_dialog(
    theme: &Theme,
    intent: ProcessBatchIntent,
    entity: Entity<RootView>,
    scroll: ScrollHandle,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let count = intent.targets.len();
    let title = i18n::t("proc.batch_confirm_title").replace("{count}", &count.to_string());
    let message = i18n::t("proc.batch_confirm_message")
        .replace("{action}", action_label(intent.action))
        .replace("{count}", &count.to_string());
    let close = entity.clone();
    let on_close = move |_window: &mut Window, cx: &mut App| {
        close.update(cx, |view, cx| {
            view.cancel_process_batch();
            cx.notify();
        });
    };
    let cancel = entity.clone();
    let confirm = entity;
    // The confirm dialog reports the full target count in its title/message;
    // the scroll list materializes only a bounded window (select-all on a
    // filtered list can select thousands) with the rest behind the explicit
    // "… {count} more" hint.
    const MAX_BATCH_TARGET_ROWS: usize = 50;
    let shown = intent.targets.len().min(MAX_BATCH_TARGET_ROWS);
    let hidden = intent.targets.len() - shown;
    let mut targets = bounded_scroll_region_with_rail(
        BoundedScrollRailSpec {
            id: "process-batch-target-scroll",
            viewport_selector: "tm-process-batch-target-scroll",
            scrollbar_id: "process-batch-target-scrollbar",
            scrollbar_selector: "tm-process-batch-target-scrollbar",
            track_selector: "tm-process-batch-target-scrollbar-track",
            width: None,
            max_height: px(150.0),
            scroll,
            palette: theme.palette(),
        },
        div().flex().flex_col().gap(tokens::SPACE_3).children(
            intent.targets.iter().take(shown).map(|target| {
                div()
                    .text_size(tokens::FONT_12)
                    .text_color(theme.fg)
                    .child(format!("{} (PID {})", target.name, target.pid))
            }),
        ),
    )
    .p(tokens::SPACE_8)
    .rounded(tokens::control_radius(theme))
    .bg(theme.sidebar_card_bg);
    if hidden > 0 {
        targets = targets.child(elements::more_rows_hint(theme, hidden));
    }
    let content: AnyElement = div()
        .w(px(420.0))
        .flex()
        .flex_col()
        .gap(tokens::SPACE_12)
        .child(
            div()
                .text_size(tokens::FONT_13)
                .text_color(theme.fg)
                .child(message),
        )
        .child(targets)
        .child(
            div()
                .flex()
                .justify_end()
                .gap(tokens::SPACE_8)
                .child(elements::pill(
                    theme,
                    "process-batch-cancel",
                    i18n::t("common.cancel"),
                    false,
                    false,
                    move |_window, cx| {
                        cancel.update(cx, |view, cx| {
                            view.cancel_process_batch();
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "process-batch-confirm",
                    action_label(intent.action),
                    true,
                    false,
                    move |_window, cx| {
                        confirm.update(cx, |view, cx| {
                            view.confirm_process_batch(cx);
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
#[path = "../../../tests/gui/gpui_gpui_app_root_batch_process_tests.rs"]
mod tests;
