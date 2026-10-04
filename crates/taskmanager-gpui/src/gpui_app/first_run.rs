//! Mission Center-compatible first-run setup dialog.
//!
//! The dialog is driven by the application `first-run.setup` capability. It
//! never interprets or launches the displayed command strings; View/Run/Revert
//!/Restart each submit their own typed action to the native provider.

use gpui::{
    App, ClipboardItem, Context, Div, Entity, InteractiveElement, ParentElement, Styled, Window,
    div, px,
};
use taskmanager_shell::presentation::first_run::first_run_failure_key;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

use crate::gpui_app::elements;
use crate::gpui_app::root::{RootView, platform_submission_time_ms};
use crate::gpui_app::root::{WindowSurfaceDismissReason, WindowSurfaceKind};
use taskmanager_application::CorrelatedSetupScriptEvent;
use taskmanager_application::i18n;
use taskmanager_core::core::setup::SetupScriptAction;
use taskmanager_platform_contract::OperationFailure;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use taskmanager_application::first_run::{
    DOCUMENTATION_URL, FirstRunCompletion, FirstRunPhase, FirstRunUiState,
};
fn empty_state_message_key(phase: &FirstRunPhase) -> &'static str {
    match phase {
        FirstRunPhase::Failed(kind) => first_run_failure_key(*kind),
        _ => "first_run.discovering",
    }
}

fn info_row(
    theme: &Theme,
    label_key: &'static str,
    value: String,
    copy_id: &'static str,
    copy_label_key: &'static str,
) -> impl gpui::IntoElement {
    let copy_value = value.clone();
    div()
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_4))
        .child(
            div()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(i18n::t(label_key)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(definite_length(tokens::SPACE_8))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(font_size(tokens::FONT_12))
                        .text_color(hsla(theme.fg))
                        .child(value),
                )
                .child(elements::pill(
                    theme,
                    copy_id,
                    i18n::t(copy_label_key),
                    false,
                    false,
                    move |_window: &mut Window, cx: &mut App| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_value.clone()));
                    },
                    |_, _, _| {},
                )),
        )
}

fn action_button(
    theme: &Theme,
    entity: Entity<RootView>,
    id: &'static str,
    label_key: &'static str,
    action: SetupScriptAction,
    primary: bool,
    disabled: bool,
) -> impl gpui::IntoElement {
    elements::Pill::new(
        id,
        i18n::t(label_key),
        move |_window: &mut Window, cx: &mut App| {
            entity.update(cx, |view, cx| {
                view.request_first_run_action(action, cx);
                cx.notify();
            });
        },
        |_, _, _| {},
    )
    .active(primary)
    .enabled(!disabled)
    .render(theme)
}

/// Render the dialog body from the latest application projection.
pub fn render_first_run(
    theme: &Theme,
    state: &FirstRunUiState,
    entity: Entity<RootView>,
) -> (Div, Div) {
    let Some(info) = state.info.clone() else {
        let failed = matches!(state.phase, FirstRunPhase::Failed(_));
        let close_entity = entity.clone();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(definite_length(tokens::SPACE_12))
            .text_size(font_size(tokens::FONT_13))
            .text_color(hsla(if failed { theme.danger } else { theme.fg_dim }))
            .child(i18n::t(empty_state_message_key(&state.phase)));
        if failed {
            body = body.child(elements::pill(
                theme,
                "first-run-close-error",
                i18n::t("common.close"),
                false,
                false,
                move |_window: &mut Window, cx: &mut App| {
                    close_entity.update(cx, |view, cx| {
                        view.dismiss_window_surface(
                            crate::gpui_app::root::WindowSurfaceKind::FirstRun,
                            crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                        );
                        cx.notify();
                    });
                },
                |_, _, _| {},
            ));
        }
        return (body, div());
    };
    let pending = state.action_pending();
    let run_entity = entity.clone();
    let revert_entity = entity.clone();
    let view_entity = entity.clone();
    let restart_entity = entity.clone();
    let docs_entity = entity.clone();
    let close_entity = entity.clone();
    let path = info.path.display().to_string();
    let run_command = info.run_command.clone();
    let revert_command = info.revert_command.clone();
    let mut body = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_12))
        .child(
            div()
                .text_size(font_size(tokens::FONT_13))
                .text_color(hsla(theme.fg))
                .child(i18n::t("first_run.description")),
        )
        .child(info_row(
            theme,
            "first_run.location",
            path,
            "first-run-copy-location",
            "first_run.copy_location",
        ))
        .child(info_row(
            theme,
            "first_run.run_command",
            run_command,
            "first-run-copy-command",
            "first_run.copy_command",
        ))
        .child(info_row(
            theme,
            "first_run.revert_command",
            revert_command,
            "first-run-copy-revert",
            "first_run.copy_revert_command",
        ));

    let failure = if let FirstRunPhase::Failed(kind) = state.phase {
        body = body.child(
            div()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.danger))
                .child(i18n::t(first_run_failure_key(kind))),
        );
        Some(kind)
    } else {
        None
    };
    let status_key = match state.phase {
        FirstRunPhase::Running => Some("first_run.running"),
        FirstRunPhase::Reverting => Some("first_run.reverting"),
        FirstRunPhase::Restarting => Some("first_run.restarting"),
        FirstRunPhase::RestartRequired => Some("first_run.restart_required"),
        _ => None,
    };
    if let Some(status_key) = status_key {
        body = body.child(
            div()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.accent))
                .child(i18n::t(status_key)),
        );
    }

    let mut actions = div()
        .debug_selector(|| "tm-first-run-actions".to_owned())
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(definite_length(tokens::SPACE_8))
        .child(elements::pill(
            theme,
            "first-run-open-docs",
            i18n::t("first_run.open_docs"),
            false,
            pending,
            move |_window: &mut Window, cx: &mut App| {
                docs_entity.update(cx, |view, cx| {
                    let _ = view.request_open_url(DOCUMENTATION_URL.to_owned(), cx);
                });
            },
            |_, _, _| {},
        ))
        .child(action_button(
            theme,
            view_entity,
            "first-run-view-script",
            "first_run.view_script",
            SetupScriptAction::View,
            false,
            pending,
        ))
        .child(action_button(
            theme,
            run_entity,
            "first-run-run-setup",
            "first_run.run_setup",
            SetupScriptAction::Run,
            true,
            pending,
        ))
        .child(action_button(
            theme,
            revert_entity,
            "first-run-revert-setup",
            "first_run.revert_setup",
            SetupScriptAction::Revert,
            false,
            pending,
        ));
    if let Some(kind) = failure {
        let copy_value = i18n::t(first_run_failure_key(kind)).to_owned();
        let copy_entity = entity.clone();
        actions = actions
            .child(elements::pill(
                theme,
                "first-run-copy-output",
                i18n::t("first_run.copy_output"),
                false,
                false,
                move |_window: &mut Window, cx: &mut App| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_value.clone()));
                },
                |_, _, _| {},
            ))
            .child(elements::pill(
                theme,
                "first-run-close-error",
                i18n::t("common.close"),
                false,
                false,
                move |_window: &mut Window, cx: &mut App| {
                    copy_entity.update(cx, |view, cx| {
                        view.dismiss_window_surface(
                            crate::gpui_app::root::WindowSurfaceKind::FirstRun,
                            crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                        );
                        cx.notify();
                    });
                },
                |_, _, _| {},
            ));
        if let Some(action @ (SetupScriptAction::Run | SetupScriptAction::Revert)) =
            state.last_action
        {
            actions = actions.child(action_button(
                theme,
                entity.clone(),
                "first-run-retry",
                "first_run.retry",
                action,
                true,
                false,
            ));
        }
    }
    if state.phase == FirstRunPhase::RestartRequired {
        actions = actions.child(action_button(
            theme,
            restart_entity,
            "first-run-restart",
            "first_run.restart",
            SetupScriptAction::Restart,
            true,
            false,
        ));
    }
    actions = actions.child(elements::pill(
        theme,
        "first-run-close",
        i18n::t("common.close"),
        false,
        false,
        move |_window: &mut Window, cx: &mut App| {
            close_entity.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::FirstRun,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        },
        |_, _, _| {},
    ));
    (body, actions)
}

/// Render the non-modal entry point for optional setup.
///
/// Discovery is intentionally separate from presentation: startup may learn
/// that the fixed setup asset exists, but that fact must not commandeer the
/// user's current page. The Settings entry is the stable, explicit route back
/// to the full first-run surface.
pub(crate) fn render_settings_row(theme: &Theme, entity: Entity<RootView>) -> Div {
    let open_entity = entity;
    div()
        .debug_selector(|| "first-run-settings-row".to_owned())
        .flex()
        .items_center()
        .justify_between()
        .gap(definite_length(tokens::SPACE_12))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(definite_length(tokens::SPACE_4))
                .child(
                    div()
                        .text_size(font_size(tokens::FONT_13))
                        .text_color(hsla(theme.fg))
                        .child(i18n::t("settings.additional_setup_detail")),
                ),
        )
        .child(elements::pill(
            theme,
            "first-run-open-from-settings",
            i18n::t("settings.additional_setup_open"),
            false,
            false,
            move |_window: &mut Window, cx: &mut App| {
                open_entity.update(cx, |view, cx| {
                    view.show_first_run();
                    cx.notify();
                });
            },
            |_, _, _| {},
        ))
}

impl RootView {
    pub(crate) fn request_first_run_observation(&mut self, _cx: &mut Context<Self>) {
        self.first_run
            .observe(self.platform.as_mut(), platform_submission_time_ms());
    }

    pub(crate) fn request_first_run_action(
        &mut self,
        action: SetupScriptAction,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.first_run_open() {
            return false;
        }
        let accepted = self.first_run.request(
            action,
            self.platform.as_mut(),
            platform_submission_time_ms(),
        );
        cx.notify();
        accepted
    }

    pub(crate) fn apply_first_run_event(
        &mut self,
        event: CorrelatedSetupScriptEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let outcome = self.first_run.complete(&event);
        self.apply_first_run_completion(outcome, cx)
    }

    pub(crate) fn apply_first_run_failure(
        &mut self,
        failure: &OperationFailure,
        cx: &mut Context<Self>,
    ) -> bool {
        let outcome = self.first_run.fail(failure);
        self.apply_first_run_completion(outcome, cx)
    }

    fn apply_first_run_completion(
        &mut self,
        outcome: FirstRunCompletion,
        cx: &mut Context<Self>,
    ) -> bool {
        if outcome == FirstRunCompletion::Unchanged {
            return false;
        }
        if outcome == FirstRunCompletion::Restart {
            cx.spawn(async move |_entity, cx| {
                let _ = cx.update(|app| app.quit());
            })
            .detach();
        }
        if self.first_run.view().info.is_none() {
            self.dismiss_window_surface(
                WindowSurfaceKind::FirstRun,
                WindowSurfaceDismissReason::Completed,
            );
        }
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_first_run_tests.rs"]
mod tests;
