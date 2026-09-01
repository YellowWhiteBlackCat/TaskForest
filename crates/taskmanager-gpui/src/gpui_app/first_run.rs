//! Mission Center-compatible first-run setup dialog.
//!
//! The dialog is driven by the application `first-run.setup` capability. It
//! never interprets or launches the displayed command strings; View/Run/Revert
//!/Restart each submit their own typed action to the native provider.

use gpui::{
    App, ClipboardItem, Context, Div, Entity, InteractiveElement, ParentElement, Styled, Window,
    div, px,
};

use crate::gpui_app::elements;
use crate::gpui_app::root::{RootView, platform_submission_time_ms};
use taskmanager_application::i18n;
use taskmanager_application::{CorrelatedSetupScriptEvent, SetupScriptRequest};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptEvent, SetupScriptInfo};
use taskmanager_platform_contract::{OperationFailure, SubmissionErrorKind};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

/// The upstream First Run dialog's explicit wiki destination. Opening it still
/// goes through the ordinary typed URL-open port; this module never launches a
/// browser command directly.
pub const WIKI_URL: &str = "https://gitlab.com/mission-center-devs/mission-center/-/wikis/home";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FirstRunPhase {
    #[default]
    Hidden,
    Discovering,
    Available,
    Running,
    Reverting,
    RestartRequired,
    Restarting,
    Failed(FailureKind),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstRunUiState {
    pub phase: FirstRunPhase,
    pub info: Option<SetupScriptInfo>,
    pub last_action: Option<SetupScriptAction>,
}

fn failure_key(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unsupported => "first_run.failure_unsupported",
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            "first_run.failure_permission"
        }
        FailureKind::MissingDependency => "first_run.failure_missing_dependency",
        FailureKind::TimedOut => "first_run.failure_timeout",
        FailureKind::IdentityChanged => "first_run.failure_identity",
        FailureKind::TemporarilyUnavailable => "first_run.failure_unavailable",
        FailureKind::Rejected => "first_run.failure_rejected",
        FailureKind::ProviderFault => "first_run.failure_provider",
    }
}

fn empty_state_message_key(phase: &FirstRunPhase) -> &'static str {
    match phase {
        FirstRunPhase::Failed(kind) => failure_key(*kind),
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t(label_key)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
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
pub fn render_first_run(theme: &Theme, state: &FirstRunUiState, entity: Entity<RootView>) -> Div {
    let Some(info) = state.info.clone() else {
        let failed = matches!(state.phase, FirstRunPhase::Failed(_));
        let close_entity = entity.clone();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_12,
            ))
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
            .text_color(taskmanager_ui::theme_binding::hsla(if failed {
                theme.danger
            } else {
                theme.fg_dim
            }))
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
        return body;
    };
    let pending = matches!(
        state.phase,
        FirstRunPhase::Running | FirstRunPhase::Reverting | FirstRunPhase::Restarting
    );
    let run_entity = entity.clone();
    let revert_entity = entity.clone();
    let view_entity = entity.clone();
    let restart_entity = entity.clone();
    let wiki_entity = entity.clone();
    let close_entity = entity.clone();
    let path = info.path.display().to_string();
    let run_command = info.run_command.clone();
    let revert_command = info.revert_command.clone();
    let mut body = div()
        .w(px(520.0))
        .max_w(px(520.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
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
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.danger))
                .child(i18n::t(failure_key(kind))),
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
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.accent))
                .child(i18n::t(status_key)),
        );
    }

    let mut actions = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(elements::pill(
            theme,
            "first-run-open-wiki",
            i18n::t("first_run.open_wiki"),
            false,
            pending,
            move |_window: &mut Window, cx: &mut App| {
                wiki_entity.update(cx, |view, cx| {
                    let _ = view.request_open_url(WIKI_URL.to_owned(), cx);
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
        let copy_value = i18n::t(failure_key(kind)).to_owned();
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
    body.child(actions)
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_4,
                ))
                .child(
                    div()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
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

fn submission_failure_kind(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::UnsupportedCapability => FailureKind::Unsupported,
        SubmissionErrorKind::Busy | SubmissionErrorKind::RuntimeStopped => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::InvalidRequest => FailureKind::Rejected,
    }
}

impl RootView {
    pub(crate) fn request_first_run_observation(&mut self, _cx: &mut Context<Self>) {
        self.first_run.phase = FirstRunPhase::Discovering;
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_setup_script(
                        SetupScriptRequest {
                            action: SetupScriptAction::Observe,
                        },
                        platform_submission_time_ms(),
                    )
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => {
                self.first_run_requests
                    .insert(request_id, SetupScriptAction::Observe);
            }
            Err(_) => {
                self.first_run.phase = FirstRunPhase::Hidden;
            }
        }
    }

    pub(crate) fn request_first_run_action(
        &mut self,
        action: SetupScriptAction,
        cx: &mut Context<Self>,
    ) -> bool {
        if action == SetupScriptAction::Observe
            || (self.first_run.info.is_none() && action != SetupScriptAction::Restart)
            || (action == SetupScriptAction::Restart
                && self.first_run.phase != FirstRunPhase::RestartRequired)
        {
            self.first_run.phase = FirstRunPhase::Failed(if action == SetupScriptAction::Restart {
                FailureKind::Rejected
            } else {
                FailureKind::Unsupported
            });
            self.show_first_run();
            cx.notify();
            return false;
        }
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_setup_script(
                        SetupScriptRequest { action },
                        platform_submission_time_ms(),
                    )
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => {
                self.first_run_requests.insert(request_id, action);
                self.first_run.last_action = Some(action);
                self.first_run.phase = match action {
                    SetupScriptAction::Run => FirstRunPhase::Running,
                    SetupScriptAction::Revert => FirstRunPhase::Reverting,
                    SetupScriptAction::Restart => FirstRunPhase::Restarting,
                    SetupScriptAction::Observe => FirstRunPhase::Discovering,
                    SetupScriptAction::View => FirstRunPhase::Available,
                };
                true
            }
            Err(kind) => {
                self.first_run.phase = FirstRunPhase::Failed(submission_failure_kind(kind));
                self.show_first_run();
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn apply_first_run_event(
        &mut self,
        correlated: CorrelatedSetupScriptEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(requested) = self.first_run_requests.remove(&correlated.request_id) else {
            return false;
        };
        match (requested, correlated.event) {
            (SetupScriptAction::Observe, SetupScriptEvent::Observed(info)) => {
                self.first_run.info = info;
                let available = self.first_run.info.is_some();
                self.first_run.phase = if available {
                    FirstRunPhase::Available
                } else {
                    FirstRunPhase::Hidden
                };
                // Observation is a background capability check. An optional
                // setup must never become a startup modal; the Settings entry
                // is the explicit discovery route.
                if !available {
                    self.dismiss_window_surface(
                        crate::gpui_app::root::WindowSurfaceKind::FirstRun,
                        crate::gpui_app::root::WindowSurfaceDismissReason::Completed,
                    );
                }
                true
            }
            (action, SetupScriptEvent::ActionCompleted { action: completed })
                if action == completed =>
            {
                self.first_run.phase = match action {
                    SetupScriptAction::Run => FirstRunPhase::RestartRequired,
                    SetupScriptAction::Revert
                    | SetupScriptAction::View
                    | SetupScriptAction::Observe => FirstRunPhase::Available,
                    SetupScriptAction::Restart => {
                        cx.spawn(async move |_entity, cx| {
                            let _ = cx.update(|app| app.quit());
                        })
                        .detach();
                        FirstRunPhase::Restarting
                    }
                };
                self.show_first_run();
                true
            }
            _ => {
                self.first_run.phase = FirstRunPhase::Failed(FailureKind::ProviderFault);
                self.show_first_run();
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn apply_first_run_failure(
        &mut self,
        failure: &OperationFailure,
        cx: &mut Context<Self>,
    ) -> bool {
        if failure.capability != taskmanager_platform_contract::CapabilityId::FIRST_RUN_SETUP {
            return false;
        }
        let Some(action) = self.first_run_requests.remove(&failure.request_id) else {
            return false;
        };
        if action == SetupScriptAction::Observe {
            self.first_run.phase = FirstRunPhase::Hidden;
            self.dismiss_window_surface(
                crate::gpui_app::root::WindowSurfaceKind::FirstRun,
                crate::gpui_app::root::WindowSurfaceDismissReason::Completed,
            );
            return true;
        }
        self.first_run.phase = FirstRunPhase::Failed(failure.kind);
        self.show_first_run();
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_first_run_tests.rs"]
mod tests;
