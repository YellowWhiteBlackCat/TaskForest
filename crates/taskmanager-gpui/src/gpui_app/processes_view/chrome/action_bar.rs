//! Process actions and the selection summary.

use std::collections::HashSet;

use gpui::{
    AnyElement, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div,
};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::ProcessTerminationAction;
use taskmanager_application::i18n;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::process::{PriorityTier, ProcessBatchAction};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{self, UiSize};

use super::action_button::{ActionBtnProps, action_btn};
use super::columns::columns_dropdown;
use super::page_layout::{ProcessActionPresentation, ProcessActionSurface};
use taskmanager_shell::{ProcessControlAvailability, ProcessControlScope, SortCol};

/// Inputs for the process action strip. The caller supplies the already
/// projected column visibility, including the provider-confirmed no-swap
/// policy, so this component cannot make a second availability decision.
pub(super) struct ProcessActionBarProps<'a> {
    pub(super) theme: &'a Theme,
    pub(super) selected_identity: Option<ProcessLiveKey>,
    pub(super) control: ProcessControlAvailability,
    pub(super) hidden_cols: &'a HashSet<SortCol>,
    pub(super) swap_auto_hidden: bool,
    pub(super) hovered: Option<&'a Hover>,
    pub(super) batch_history_available: bool,
    pub(super) actions: ProcessActionPresentation,
    pub(super) surface: ProcessActionSurface,
    pub(super) ui_size: UiSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessToolbarAction {
    RunNewTask,
    End,
    ForceKill,
    Suspend,
    Resume,
    SetPriority(PriorityTier),
    Affinity,
    ExportBatchHistory,
}

#[derive(Clone, Copy)]
struct ProcessActionAvailability {
    control: ProcessControlAvailability,
    batch_history: bool,
}

impl ProcessToolbarAction {
    fn label(self) -> &'static str {
        match self {
            Self::RunNewTask => i18n::t("proc.run_new_task"),
            Self::End => i18n::t("proc.end_task"),
            Self::ForceKill => i18n::t("proc.kill"),
            Self::Suspend => i18n::t("proc.suspend"),
            Self::Resume => i18n::t("proc.resume"),
            Self::SetPriority(tier) => i18n::t(tier.i18n_key()),
            Self::Affinity => i18n::t("proc.affinity"),
            Self::ExportBatchHistory => i18n::t("proc.batch_history_export"),
        }
    }

    const fn tooltip(self) -> &'static str {
        match self {
            Self::RunNewTask => "tooltip.proc_run_new",
            Self::End => "tooltip.proc_end_task",
            Self::ForceKill => "tooltip.proc_kill",
            Self::Suspend => "tooltip.proc_suspend",
            Self::Resume => "tooltip.proc_resume",
            Self::SetPriority(PriorityTier::High) => "tooltip.proc_high",
            Self::SetPriority(PriorityTier::Normal) => "tooltip.proc_normal",
            Self::SetPriority(PriorityTier::Low) => "tooltip.proc_low",
            Self::Affinity => "tooltip.proc_affinity",
            Self::ExportBatchHistory => "tooltip.proc_batch_export",
        }
    }

    const fn icon(self) -> Option<IconId> {
        match self {
            Self::RunNewTask => Some(IconId::Process),
            Self::End => Some(IconId::Close),
            Self::ForceKill => Some(IconId::EndTask),
            Self::Affinity => Some(IconId::Settings),
            Self::ExportBatchHistory => Some(IconId::Export),
            Self::Suspend | Self::Resume | Self::SetPriority(_) => None,
        }
    }

    const fn enabled(self, availability: ProcessActionAvailability) -> bool {
        match self {
            Self::RunNewTask => true,
            Self::End | Self::ForceKill | Self::Suspend | Self::Resume | Self::SetPriority(_) => {
                availability.control.is_ready()
            }
            Self::Affinity => availability.control.is_single_process(),
            Self::ExportBatchHistory => availability.batch_history,
        }
    }

    fn perform(self, view: &mut RootView, cx: &mut Context<RootView>) {
        let availability = ProcessActionAvailability {
            control: view.process_control_availability(),
            batch_history: !view.process_batch_history.is_empty(),
        };
        if !self.enabled(availability) {
            return;
        }
        match self {
            Self::RunNewTask => {
                view.show_run_task();
                view.run_error = None;
                cx.notify();
            }
            Self::End => submit_batch_or_single(
                view,
                ProcessBatchAction::End,
                Some(ProcessTerminationAction::EndTask),
                cx,
            ),
            Self::ForceKill => submit_batch_or_single(
                view,
                ProcessBatchAction::Kill,
                Some(ProcessTerminationAction::ForceKill),
                cx,
            ),
            Self::Suspend => submit_batch_or_single(view, ProcessBatchAction::Suspend, None, cx),
            Self::Resume => submit_batch_or_single(view, ProcessBatchAction::Resume, None, cx),
            Self::SetPriority(tier) => {
                submit_batch_or_single(view, ProcessBatchAction::SetPriority(tier), None, cx)
            }
            Self::Affinity => {
                if view.process_control_availability().is_single_process()
                    && let Some(identity) = view.selected_process_identity()
                {
                    view.show_process_affinity(identity);
                    view.processes_state.affinity_editor.cpus.clear();
                    view.processes_state.affinity_editor.hover = None;
                    view.request_process_affinity(identity, cx);
                    cx.notify();
                }
            }
            Self::ExportBatchHistory => view.copy_process_batch_history(cx),
        }
    }
}

fn submit_batch_or_single(
    view: &mut RootView,
    batch_action: ProcessBatchAction,
    termination: Option<ProcessTerminationAction>,
    cx: &mut Context<RootView>,
) {
    let availability = view.process_control_availability();
    if !availability.is_ready() {
        return;
    }
    let submitted = match availability.scope() {
        Some(ProcessControlScope::Tree | ProcessControlScope::Batch) => {
            view.request_process_batch(batch_action);
            true
        }
        Some(ProcessControlScope::Single) => {
            let Some(identity) = view.selected_process_identity() else {
                return;
            };
            if let Some(termination) = termination {
                view.request_process_termination(termination, identity);
            } else if !view.submit_process_batch_immediate(batch_action, identity, cx) {
                return;
            }
            true
        }
        None => false,
    };
    if submitted {
        cx.notify();
    }
}

fn toolbar_action(
    action: ProcessToolbarAction,
    theme: &Theme,
    hovered: Option<&Hover>,
    availability: ProcessActionAvailability,
    cx: &mut Context<RootView>,
) -> AnyElement {
    action_btn(
        ActionBtnProps {
            theme,
            label: action.label(),
            tip: action.tooltip(),
            icon: action.icon(),
            hovered,
            enabled: action.enabled(availability),
            action: move |view: &mut RootView, cx: &mut Context<RootView>| action.perform(view, cx),
        },
        cx,
    )
    .into_any_element()
}

fn overflow_entry(
    action: ProcessToolbarAction,
    availability: ProcessActionAvailability,
    entity: &Entity<RootView>,
) -> taskmanager_ui::overlays::popup::MenuEntry {
    use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem};

    let entity = entity.clone();
    let mut item = MenuItem::new(action.label(), move |_, cx| {
        entity.update(cx, |view, cx| action.perform(view, cx));
    })
    .disabled(!action.enabled(availability));
    if let Some(icon) = action.icon() {
        item = item.icon(icon);
    }
    MenuEntry::Item(item)
}

fn actions_dropdown(
    theme: &Theme,
    hovered: Option<&Hover>,
    availability: ProcessActionAvailability,
    actions: ProcessActionPresentation,
    ui_size: UiSize,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    use taskmanager_ui::overlays::dropdown_menu::DropdownMenu;
    use taskmanager_ui::overlays::popup::{MenuEntry, PopupMenuState};

    let label = i18n::t("proc.actions");
    let background = if hovered == Some(&Hover::Static(label)) {
        theme.hover_bg()
    } else {
        theme.sidebar_card_bg
    };
    let trigger = div()
        .id("proc-actions-trigger")
        .debug_selector(|| "tm-proc-actions-trigger".to_string())
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(theme),
        ))
        .bg(taskmanager_ui::theme_binding::fill(background))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_14))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
        .focusable()
        .tab_stop(true)
        .focus(crate::gpui_app::elements::focus_ring(theme))
        .cursor_pointer()
        .on_hover(cx.listener(move |view, is_hovered: &bool, _, cx| {
            view.set_hover(is_hovered.then_some(Hover::Static(label)), cx);
        }))
        .child(
            taskmanager_ui::icons_binding::icon(IconId::More)
                .size(taskmanager_ui::theme_binding::length(ui_size.icon_size())),
        )
        .child(label);
    let entity = cx.entity();
    DropdownMenu::new(
        "proc-actions-menu",
        trigger,
        theme.palette(),
        move |_state, cx| {
            let mut entries = Vec::new();
            match actions {
                ProcessActionPresentation::Essential => entries.push(overflow_entry(
                    ProcessToolbarAction::ForceKill,
                    availability,
                    &entity,
                )),
                ProcessActionPresentation::Primary => {}
            }
            entries.extend([
                overflow_entry(ProcessToolbarAction::Suspend, availability, &entity),
                overflow_entry(ProcessToolbarAction::Resume, availability, &entity),
                MenuEntry::Separator,
                MenuEntry::Label(i18n::t("proc.priority").into()),
                overflow_entry(
                    ProcessToolbarAction::SetPriority(PriorityTier::High),
                    availability,
                    &entity,
                ),
                overflow_entry(
                    ProcessToolbarAction::SetPriority(PriorityTier::Normal),
                    availability,
                    &entity,
                ),
                overflow_entry(
                    ProcessToolbarAction::SetPriority(PriorityTier::Low),
                    availability,
                    &entity,
                ),
                MenuEntry::Separator,
                overflow_entry(ProcessToolbarAction::Affinity, availability, &entity),
                overflow_entry(
                    ProcessToolbarAction::ExportBatchHistory,
                    availability,
                    &entity,
                ),
            ]);
            PopupMenuState::new(entries, cx)
        },
    )
}

/// Keep separators scoped to one action row. `Divider`'s generic vertical
/// variant intentionally fills its parent's cross-axis; that is correct for
/// panel chrome, but a wrapping action strip has more than one possible line.
/// A fixed control-height hairline prevents a wrapped line from stretching
/// into the mode/filter row below it.
fn action_divider(theme: &Theme) -> Div {
    div()
        .w_px()
        .h(taskmanager_ui::theme_binding::length(tokens::SPACE_24))
        .flex_shrink_0()
        .bg(taskmanager_ui::theme_binding::fill(theme.palette().border))
        .debug_selector(|| "tm-proc-action-divider".to_string())
}

pub(super) fn action_bar(props: ProcessActionBarProps<'_>, cx: &mut Context<RootView>) -> Div {
    let ProcessActionBarProps {
        theme,
        selected_identity,
        control,
        hidden_cols,
        swap_auto_hidden,
        hovered,
        batch_history_available,
        actions,
        surface,
        ui_size,
    } = props;
    let selected_count = control.target_count();
    let hint = if selected_count > 1 {
        i18n::t("proc.batch_selected").replace("{count}", &selected_count.to_string())
    } else {
        match selected_identity {
            Some(identity) => format!("{} {}", i18n::t("hint.selected_pid"), identity.pid()),
            None => i18n::t("hint.select_process").to_string(),
        }
    };
    let availability = ProcessActionAvailability {
        control,
        batch_history: batch_history_available,
    };
    let mut content = vec![
        toolbar_action(
            ProcessToolbarAction::RunNewTask,
            theme,
            hovered,
            availability,
            cx,
        ),
        action_divider(theme).into_any_element(),
        toolbar_action(ProcessToolbarAction::End, theme, hovered, availability, cx),
    ];
    match actions {
        ProcessActionPresentation::Essential => {}
        ProcessActionPresentation::Primary => content.push(toolbar_action(
            ProcessToolbarAction::ForceKill,
            theme,
            hovered,
            availability,
            cx,
        )),
    }
    content.extend([
        action_divider(theme).into_any_element(),
        columns_dropdown(theme, hovered, hidden_cols, swap_auto_hidden, cx).into_any_element(),
        actions_dropdown(theme, hovered, availability, actions, ui_size, cx).into_any_element(),
    ]);
    match surface {
        ProcessActionSurface::Standalone => content.push(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(hint)
                .into_any_element(),
        ),
        ProcessActionSurface::Embedded => {}
    }

    // This priority strip is deliberately single-line. Secondary commands
    // live in the anchored menu, so the table's height cannot depend on label
    // wrapping or on how many process commands the platform supports.
    let content = div()
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .children(content)
        .debug_selector(|| "tm-proc-action-bar".to_string());
    let mut action_bar = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .debug_selector(|| "tm-proc-action-surface".to_string())
        .child(content);
    action_bar = match (surface, actions) {
        (ProcessActionSurface::Standalone, ProcessActionPresentation::Essential) => action_bar
            .w_full()
            .py(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_2,
            ))
            .rounded(taskmanager_ui::theme_binding::absolute(
                tokens::card_radius(theme),
            ))
            .border_1()
            .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
            .bg(taskmanager_ui::theme_binding::fill(theme.card_surface())),
        (ProcessActionSurface::Standalone, ProcessActionPresentation::Primary) => action_bar
            .w_full()
            .py(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_4,
            ))
            .rounded(taskmanager_ui::theme_binding::absolute(
                tokens::card_radius(theme),
            ))
            .border_1()
            .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
            .bg(taskmanager_ui::theme_binding::fill(theme.card_surface())),
        (ProcessActionSurface::Embedded, _) => action_bar.py(
            taskmanager_ui::theme_binding::definite_length(tokens::SPACE_2),
        ),
    };
    action_bar
}
