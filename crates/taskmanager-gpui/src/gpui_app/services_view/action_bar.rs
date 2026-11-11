//! Services action strip, split out of the page projection to keep one owner
//! per responsibility and one source file below the structural line ceiling.

use gpui::{App, Context, Div, Window};
use taskforest_core::core::services::ServiceAction;
use taskforest_core::core::target::ServiceId;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_ui::primitives::toolbar::Toolbar;

use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::{elements, list_view};

use super::ActionFeedback;

pub(super) struct ActionBarProps<'a> {
    pub theme: &'a Theme,
    pub selected: Option<&'a ServiceId>,
    pub selected_name: Option<&'a str>,
    pub hovered: Option<&'a Hover>,
    pub feedback: Option<ActionFeedback>,
}

struct ActionButtonProps<'a> {
    theme: &'a Theme,
    id: &'static str,
    label_key: &'static str,
    action: ServiceAction,
    confirm: bool,
    selected: Option<&'a ServiceId>,
    hovered: Option<&'a Hover>,
    entity: &'a gpui::Entity<RootView>,
}

fn button(props: ActionButtonProps<'_>) -> impl gpui::IntoElement {
    let ActionButtonProps {
        theme,
        id,
        label_key,
        action,
        confirm,
        selected,
        hovered,
        entity,
    } = props;
    let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
    let hovered_bool = enabled && hovered == Some(&Hover::Static(id));
    let click_entity = entity.clone();
    let hover_entity = entity.clone();
    elements::tool_btn(
        theme,
        id,
        i18n::t(label_key),
        enabled,
        hovered_bool,
        move |_win: &mut Window, cx: &mut App| {
            click_entity.update(cx, |view, cx| {
                if let Some(target) = view.selected_service.clone() {
                    if confirm {
                        view.request_service_control_confirmation(target, action);
                    } else {
                        view.request_service_action(target, action);
                    }
                    cx.notify();
                }
            });
        },
        move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
            hover_entity.update(cx, |view, cx| {
                view.set_hover(
                    if *is_hov {
                        Some(Hover::Static(id))
                    } else {
                        None
                    },
                    cx,
                );
            });
        },
    )
}

fn details_button(
    theme: &Theme,
    selected: Option<&ServiceId>,
    hovered: Option<&Hover>,
    entity: &gpui::Entity<RootView>,
) -> impl gpui::IntoElement {
    let enabled = selected.is_some_and(|id| !id.as_str().is_empty());
    let hovered_bool = enabled && hovered == Some(&Hover::Static("Details"));
    let click_entity = entity.clone();
    let hover_entity = entity.clone();
    elements::tool_btn(
        theme,
        "Details",
        i18n::t("common.details"),
        enabled,
        hovered_bool,
        move |_win: &mut Window, cx: &mut App| {
            click_entity.update(cx, |view, cx| {
                if let Some(target) = view.selected_service.clone() {
                    view.open_service_details(target);
                    cx.notify();
                }
            });
        },
        move |is_hov: &bool, _win: &mut Window, cx: &mut App| {
            hover_entity.update(cx, |view, cx| {
                view.set_hover(
                    if *is_hov {
                        Some(Hover::Static("Details"))
                    } else {
                        None
                    },
                    cx,
                );
            });
        },
    )
}

pub(super) fn action_bar(props: ActionBarProps<'_>, cx: &mut Context<RootView>) -> Div {
    let ActionBarProps {
        theme,
        selected,
        selected_name,
        hovered,
        feedback,
    } = props;
    let hint = match selected_name {
        Some(name) => format!("{} {}", i18n::t("hint.selected"), name),
        None => i18n::t("hint.select_service").to_string(),
    };
    let entity = cx.entity();
    Toolbar::new()
        .debug_selector("tm-svc-action-bar")
        .child(button(ActionButtonProps {
            theme,
            id: "Start",
            label_key: "svc.start",
            action: ServiceAction::Start,
            confirm: false,
            selected,
            hovered,
            entity: &entity,
        }))
        .child(button(ActionButtonProps {
            theme,
            id: "Stop",
            label_key: "svc.stop",
            action: ServiceAction::Stop,
            confirm: true,
            selected,
            hovered,
            entity: &entity,
        }))
        .child(button(ActionButtonProps {
            theme,
            id: "Restart",
            label_key: "svc.restart",
            action: ServiceAction::Restart,
            confirm: true,
            selected,
            hovered,
            entity: &entity,
        }))
        .child(button(ActionButtonProps {
            theme,
            id: "Enable",
            label_key: "common.enable",
            action: ServiceAction::Enable,
            confirm: false,
            selected,
            hovered,
            entity: &entity,
        }))
        .child(button(ActionButtonProps {
            theme,
            id: "Disable",
            label_key: "common.disable",
            action: ServiceAction::Disable,
            confirm: true,
            selected,
            hovered,
            entity: &entity,
        }))
        .child(details_button(theme, selected, hovered, &entity))
        .child(list_view::feedback_status_line(
            theme,
            feedback.as_ref(),
            &hint,
        ))
        .render()
}
