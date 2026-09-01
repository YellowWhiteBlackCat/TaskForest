//! Apps page overview and control-band composition.

use std::collections::HashSet;

use gpui::{Context, Div, Entity, InteractiveElement, ParentElement, Styled, div, px};
use taskmanager_shell::ProcessStatusFilter;
use taskmanager_ui::inputs::text_input::TextInputState;
use taskmanager_ui::primitives::toolbar::Toolbar;

use super::action_bar::{ProcessActionBarProps, action_bar};
use super::page_layout::{
    ProcessChromePresentation, ProcessControlPresentation, ProcessOverviewPresentation,
};
use super::{hierarchy_summary, status_filter_row};
use crate::gpui_app::list_view;
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::SortCol;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_theme::tokens::UiSize;

pub(super) struct ProcessOverviewProps<'a> {
    pub theme: &'a Theme,
    pub application_count: usize,
    pub process_count: usize,
    pub search_input: &'a Entity<TextInputState>,
    pub presentation: ProcessChromePresentation,
    pub ui_size: UiSize,
}

pub(super) fn process_overview(props: ProcessOverviewProps<'_>) -> Div {
    let ProcessOverviewProps {
        theme,
        application_count,
        process_count,
        search_input,
        presentation,
        ui_size,
    } = props;
    let title =
        i18n::t("proc.apps_running_title").replace("{count}", &application_count.to_string());
    let mut identity = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_2,
        ))
        .child(
            div()
                .debug_selector(|| "tm-proc-overview-title".to_string())
                .truncate()
                .text_size(taskmanager_ui::theme_binding::absolute(
                    ui_size.page_title_font_size(),
                ))
                .font_weight(taskmanager_ui::theme_binding::font_weight(
                    tokens::FONT_WEIGHT_BOLD,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(title),
        );
    identity = match presentation.overview() {
        ProcessOverviewPresentation::SummaryAndSearch => identity.child(
            div()
                .debug_selector(|| "tm-proc-overview-subtitle".to_string())
                .truncate()
                .text_size(taskmanager_ui::theme_binding::absolute(
                    ui_size.header_font_size(),
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(
                    i18n::t("proc.processes_running_subtitle")
                        .replace("{count}", &process_count.to_string()),
                ),
        ),
        ProcessOverviewPresentation::TitleAndSearch => identity,
    };

    div()
        .flex()
        .items_center()
        .w_full()
        .min_w(px(0.0))
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_16,
        ))
        .debug_selector(|| "tm-proc-overview".to_string())
        .child(identity)
        .child(
            list_view::search_box_sized(
                &theme.palette(),
                search_input,
                presentation.search_width(),
            )
            .flex_shrink_0(),
        )
}

pub(super) struct ProcessControlChromeProps<'a> {
    pub theme: &'a Theme,
    pub selected_identity: Option<ProcessLiveKey>,
    pub application_selected: bool,
    pub selected_target_count: usize,
    pub hidden_cols: &'a HashSet<SortCol>,
    pub swap_auto_hidden: bool,
    pub hovered: Option<&'a Hover>,
    pub batch_history_available: bool,
    pub filter: ProcessStatusFilter,
    pub entity: &'a Entity<RootView>,
    pub presentation: ProcessChromePresentation,
    pub ui_size: UiSize,
}

pub(super) fn process_control_chrome(
    props: ProcessControlChromeProps<'_>,
    cx: &mut Context<RootView>,
) -> Div {
    let ProcessControlChromeProps {
        theme,
        selected_identity,
        application_selected,
        selected_target_count,
        hidden_cols,
        swap_auto_hidden,
        hovered,
        batch_history_available,
        filter,
        entity,
        presentation,
        ui_size,
    } = props;
    let actions = action_bar(
        ProcessActionBarProps {
            theme,
            selected_identity,
            application_selected,
            selected_target_count,
            hidden_cols,
            swap_auto_hidden,
            hovered,
            batch_history_available,
            actions: presentation.actions(),
            surface: presentation.action_surface(),
            ui_size,
        },
        cx,
    );
    let secondary_controls = || {
        Toolbar::new()
            .gap(presentation.control_gap())
            .child(hierarchy_summary(theme, hovered, entity))
            .child(div().flex_1().min_w(px(0.0)))
            .child(status_filter_row(theme, filter, hovered, entity))
            .render()
            .flex_1()
            .min_w(px(0.0))
    };

    match presentation.controls() {
        ProcessControlPresentation::Unified => div()
            .flex()
            .items_center()
            .w_full()
            .min_w(px(0.0))
            .gap(taskmanager_ui::theme_binding::definite_length(
                presentation.control_gap(),
            ))
            .px(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_4,
            ))
            .py(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_2,
            ))
            .rounded(taskmanager_ui::theme_binding::absolute(
                tokens::card_radius(theme),
            ))
            .border_1()
            .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
            .bg(taskmanager_ui::theme_binding::fill(theme.card_surface()))
            .debug_selector(|| "tm-proc-unified-controls".to_string())
            .child(actions)
            .child(secondary_controls()),
        ProcessControlPresentation::Stacked => div()
            .flex()
            .flex_col()
            .w_full()
            .min_w(px(0.0))
            .gap(taskmanager_ui::theme_binding::definite_length(
                presentation.band_gap(),
            ))
            .debug_selector(|| "tm-proc-stacked-controls".to_string())
            .child(actions)
            .child(secondary_controls()),
    }
}
