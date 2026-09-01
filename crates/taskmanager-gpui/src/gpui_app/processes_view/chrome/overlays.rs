//! CPU-affinity dialog and process-column picker overlays.

use gpui::{Context, Div, Entity, IntoElement, ParentElement, Styled, Window, div};
use std::collections::HashSet;

use super::{ActionBtnProps, action_btn};
use crate::gpui_app::elements;
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

pub(super) fn logical_cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(8)
        .min(128)
}

pub(super) struct AffinityOverlayProps<'a> {
    pub theme: &'a Theme,
    pub hovered: Option<&'a Hover>,
    pub identity: ProcessLiveKey,
    pub state: &'a taskmanager_application::ProcessAffinityState,
    pub cpus: &'a HashSet<u32>,
    pub hover_chip: Option<usize>,
}

pub(super) fn affinity_overlay(
    props: AffinityOverlayProps<'_>,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let AffinityOverlayProps {
        theme,
        hovered,
        identity,
        state,
        cpus,
        hover_chip,
    } = props;
    let entity = cx.entity();
    let count = logical_cpu_count();
    let close_entity = entity.clone();
    let content = match state {
        taskmanager_application::ProcessAffinityState::Ready(ready)
            if ready.target.live_key() == Some(identity) =>
        {
            affinity_content(theme, count, cpus, hover_chip, hovered, &entity, cx)
        }
        taskmanager_application::ProcessAffinityState::Failed { failure, .. } => {
            affinity_status_content(theme, identity, Some(*failure), hovered, cx)
        }
        taskmanager_application::ProcessAffinityState::Closed
        | taskmanager_application::ProcessAffinityState::Loading { .. }
        | taskmanager_application::ProcessAffinityState::Ready(_) => {
            affinity_status_content(theme, identity, None, hovered, cx)
        }
    };
    elements::dialog_overlay(
        theme,
        window,
        cx,
        format!(
            "{} \u{2014} PID {}",
            i18n::t("dialog.cpu_affinity"),
            identity.pid()
        ),
        move |_, cx| {
            close_entity.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::ProcessAffinity,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        },
        content,
    )
}

fn affinity_status_content(
    theme: &Theme,
    identity: ProcessLiveKey,
    failure: Option<taskmanager_core::core::failure::FailureKind>,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    let text = failure.map_or_else(
        || i18n::t("common.collecting_telemetry").to_owned(),
        |failure| taskmanager_shell::presentation::control_error_detail(failure).to_owned(),
    );
    let mut actions = div()
        .flex()
        .flex_row()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .justify_end();
    if failure.is_some() {
        actions = actions.child(action_btn(
            ActionBtnProps {
                theme,
                label: i18n::t("common.refresh"),
                tip: "tooltip.refresh",
                icon: None,
                hovered,
                enabled: true,
                action: move |view: &mut RootView, cx: &mut Context<RootView>| {
                    view.request_process_affinity(identity, cx);
                    cx.notify();
                },
            },
            cx,
        ));
    }
    actions = actions.child(action_btn(
        ActionBtnProps {
            theme,
            label: i18n::t("common.cancel"),
            tip: "tooltip.cancel",
            icon: None,
            hovered,
            enabled: true,
            action: move |view: &mut RootView, cx: &mut Context<RootView>| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::ProcessAffinity,
                    crate::gpui_app::root::WindowSurfaceDismissReason::Cancel,
                );
                cx.notify();
            },
        },
        cx,
    ));
    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(text),
        )
        .child(actions)
}

fn affinity_content(
    theme: &Theme,
    logical_cpus: usize,
    cpus: &HashSet<u32>,
    hover_chip: Option<usize>,
    hovered: Option<&Hover>,
    entity: &Entity<RootView>,
    cx: &mut Context<RootView>,
) -> Div {
    let selected = cpus.len().min(logical_cpus);
    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(format!(
                    "{} {} {} {} {}",
                    selected,
                    i18n::t("common.of"),
                    logical_cpus,
                    i18n::t("proc.logical_cpus"),
                    i18n::t("proc.selected_mark"),
                )),
        )
        .child(cpu_chip_grid(theme, logical_cpus, cpus, hover_chip, entity))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .justify_end()
                .child(action_btn(
                    ActionBtnProps {
                        theme,
                        label: i18n::t("common.cancel"),
                        tip: "tooltip.cancel",
                        icon: None,
                        hovered,
                        enabled: true,
                        action: |view: &mut RootView, cx: &mut Context<RootView>| {
                            view.dismiss_window_surface(
                                crate::gpui_app::root::WindowSurfaceKind::ProcessAffinity,
                                crate::gpui_app::root::WindowSurfaceDismissReason::Cancel,
                            );
                            cx.notify();
                        },
                    },
                    cx,
                ))
                .child(action_btn(
                    ActionBtnProps {
                        theme,
                        label: i18n::t("common.apply"),
                        tip: "tooltip.apply",
                        icon: None,
                        hovered,
                        enabled: true,
                        action: |view: &mut RootView, cx: &mut Context<RootView>| {
                            let mut cpus: Vec<_> = view
                                .processes_state
                                .affinity_editor
                                .cpus
                                .iter()
                                .copied()
                                .collect();
                            cpus.sort_unstable();
                            if let Some(identity) = view.process_affinity_identity() {
                                view.submit_process_affinity(identity, cpus, cx);
                            }
                            view.dismiss_window_surface(
                                crate::gpui_app::root::WindowSurfaceKind::ProcessAffinity,
                                crate::gpui_app::root::WindowSurfaceDismissReason::Completed,
                            );
                            cx.notify();
                        },
                    },
                    cx,
                )),
        )
}

fn cpu_chip_grid(
    theme: &Theme,
    logical_cpus: usize,
    cpus: &HashSet<u32>,
    hover_chip: Option<usize>,
    entity: &Entity<RootView>,
) -> Div {
    let mut grid =
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_6,
            ));
    for index in 0..logical_cpus {
        let active = cpus.contains(&(index as u32));
        let click_entity = entity.clone();
        let hover_entity = entity.clone();
        grid = grid.child(elements::pill(
            theme,
            ("aff-cpu", index),
            &index.to_string(),
            active,
            hover_chip == Some(index) && !active,
            move |_, cx| {
                let cpu = index as u32;
                click_entity.update(cx, |view, cx| {
                    if !view.processes_state.affinity_editor.cpus.insert(cpu) {
                        view.processes_state.affinity_editor.cpus.remove(&cpu);
                    }
                    cx.notify();
                });
            },
            move |hovered, _, cx| {
                let next = hovered.then_some(index);
                hover_entity.update(cx, |view, cx| {
                    if view.processes_state.affinity_editor.hover != next {
                        view.processes_state.affinity_editor.hover = next;
                        cx.notify();
                    }
                });
            },
        ));
    }
    grid
}
