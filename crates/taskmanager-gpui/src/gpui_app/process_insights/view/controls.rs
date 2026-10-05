//! Normal facet and bounded list-page controls for the properties review.
use super::MAX_INSIGHT_CARD_ROWS;
use crate::gpui_app::{elements, root::RootView};
use gpui::{Div, Entity, ParentElement, Styled, div};
use taskmanager_application::{ProcessInsightFacet, i18n::t};
use taskmanager_shell::presentation::process_insight_facet_label;
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui::theme_binding::definite_length;

pub(super) fn facet_controls(
    theme: &Theme,
    entity: &Entity<RootView>,
    active: ProcessInsightFacet,
) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(definite_length(tokens::SPACE_4));
    for facet in ProcessInsightFacet::ALL {
        let id = match facet {
            ProcessInsightFacet::Network => "properties-facet-network",
            ProcessInsightFacet::Gpu => "properties-facet-gpu",
            ProcessInsightFacet::Resources => "properties-facet-resources",
            ProcessInsightFacet::Isolation => "properties-facet-isolation",
            ProcessInsightFacet::Threads => "properties-facet-threads",
            ProcessInsightFacet::OpenFiles => "properties-facet-files",
            ProcessInsightFacet::Environment => "properties-facet-environment",
        };
        let entity = entity.clone();
        row = row.child(elements::pill(
            theme,
            id,
            process_insight_facet_label(facet),
            active == facet,
            false,
            move |_, cx| {
                entity.update(cx, |view, cx| {
                    view.select_process_insight_facet(facet);
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    }
    let entity = entity.clone();
    row.child(elements::pill(
        theme,
        "properties-insights-refresh",
        t("common.refresh"),
        false,
        false,
        move |_, cx| {
            entity.update(cx, |view, cx| {
                view.refresh_process_insights();
                cx.notify();
            });
        },
        |_, _, _| {},
    ))
}
pub(super) fn page_controls(
    theme: &Theme,
    entity: &Entity<RootView>,
    first: usize,
    total: usize,
) -> Div {
    let row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(definite_length(tokens::SPACE_6));
    if total <= MAX_INSIGHT_CARD_ROWS {
        return row;
    }
    let previous = entity.clone();
    let next = entity.clone();
    let end = first.saturating_add(MAX_INSIGHT_CARD_ROWS).min(total);
    row.child(elements::pill(
        theme,
        "properties-list-previous",
        t("prop.previous_page"),
        false,
        first == 0,
        move |_, cx| {
            previous.update(cx, |view, cx| {
                view.page_process_insights(first.saturating_sub(MAX_INSIGHT_CARD_ROWS));
                cx.notify();
            });
        },
        |_, _, _| {},
    ))
    .child(format!("{}–{} / {total}", first + 1, end))
    .child(elements::pill(
        theme,
        "properties-list-next",
        t("prop.next_page"),
        false,
        end == total,
        move |_, cx| {
            next.update(cx, |view, cx| {
                view.page_process_insights(end);
                cx.notify();
            });
        },
        |_, _, _| {},
    ))
}
