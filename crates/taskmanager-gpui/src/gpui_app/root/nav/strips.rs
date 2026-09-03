//! Navigation strips and controls separated from the tab primitive.

use super::{
    Hover, NavigationPresentation, RootView, TabProps, TopPage, gear_btn, nav_orientation_btn, tab,
};
use gpui::{
    Context, Div, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled, div,
    prelude::FluentBuilder, px,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::IconId;

/// The page-navigation strip: a floating rounded row or column of page tabs.
pub fn nav_strip(
    t: &Theme,
    active: TopPage,
    orientation: super::super::NavOrientation,
    hovered: Option<&Hover>,
    presentation: NavigationPresentation,
    cx: &mut Context<RootView>,
) -> Div {
    match orientation {
        super::super::NavOrientation::Horizontal => {
            nav_strip_horizontal(t, active, hovered, presentation, cx)
        }
        super::super::NavOrientation::Vertical => {
            nav_strip_vertical(t, active, hovered, presentation, cx)
        }
    }
}

pub fn nav_strip_horizontal(
    t: &Theme,
    active: TopPage,
    hovered: Option<&Hover>,
    presentation: NavigationPresentation,
    cx: &mut Context<RootView>,
) -> Div {
    let tabs = div()
        .id("tm-navigation-tabs-horizontal")
        .flex()
        .flex_row()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .flex_1()
        .min_w(px(0.0))
        // Labels are a semantic slot, not a reason to clip the last page. At
        // narrow widths the root budget switches to icons; the scroll
        // fallback still keeps the navigation complete for long locales and
        // future pages.
        .overflow_x_scroll()
        .child(tab(
            TabProps {
                theme: t,
                label: "Performance",
                display: "tab.performance",
                icon: IconId::Performance,
                page: TopPage::Performance,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Apps",
                display: "tab.apps",
                icon: IconId::Applications,
                page: TopPage::Apps,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Services",
                display: "tab.services",
                icon: IconId::Services,
                page: TopPage::Services,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Startup",
                display: "tab.startup",
                icon: IconId::Startup,
                page: TopPage::Startup,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Users",
                display: "tab.users",
                icon: IconId::Users,
                page: TopPage::Users,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "App history",
                display: "tab.apphistory_short",
                icon: IconId::History,
                page: TopPage::AppHistory,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Containers",
                display: "tab.containers",
                icon: IconId::Applications,
                page: TopPage::Containers,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "System",
                display: "tab.system",
                icon: IconId::System,
                page: TopPage::System,
                active,
                hovered,
                presentation,
                horizontal: true,
            },
            cx,
        ));

    div()
        .w_full()
        .min_w(px(0.0))
        .flex_shrink_0()
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .pt(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .pb(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .child(
            div()
                .debug_selector(|| "tm-navigation-strip".to_string())
                .h(px(crate::gpui_app::chrome::titlebar_height(t) + 4.0))
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_row()
                .items_center()
                .bg(taskmanager_ui::theme_binding::fill(t.sidebar_bg))
                .rounded(taskmanager_ui::theme_binding::absolute(
                    tokens::card_radius(t),
                ))
                .border_1()
                .border_color(taskmanager_ui::theme_binding::hsla(t.border))
                .shadow_sm()
                .px(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .child(tabs)
                .child(
                    div()
                        .w(taskmanager_ui::theme_binding::length(tokens::SPACE_6))
                        .flex_shrink_0(),
                )
                .child({
                    let controls = div().flex_shrink_0().flex().items_center().gap(
                        taskmanager_ui::theme_binding::definite_length(tokens::SPACE_2),
                    );
                    let controls =
                        controls.child(super::super::window_capture::current_window_capture_btn(
                            t,
                            hovered,
                            presentation == NavigationPresentation::IconOnly,
                            cx,
                        ));
                    controls
                        .child(nav_orientation_btn(t, hovered, cx))
                        .child(gear_btn(t, hovered, cx))
                }),
        )
}

pub fn nav_strip_vertical(
    t: &Theme,
    active: TopPage,
    hovered: Option<&Hover>,
    presentation: NavigationPresentation,
    cx: &mut Context<RootView>,
) -> Div {
    let tabs = div()
        .id("tm-navigation-tabs-vertical")
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        // Vertical navigation is a real bounded rail. Let it scroll instead
        // of allowing the lower pages to disappear when the window is short.
        .overflow_y_scroll()
        .child(tab(
            TabProps {
                theme: t,
                label: "Performance",
                display: "tab.performance",
                icon: IconId::Performance,
                page: TopPage::Performance,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Apps",
                display: "tab.apps",
                icon: IconId::Applications,
                page: TopPage::Apps,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Services",
                display: "tab.services",
                icon: IconId::Services,
                page: TopPage::Services,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Startup",
                display: "tab.startup",
                icon: IconId::Startup,
                page: TopPage::Startup,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Users",
                display: "tab.users",
                icon: IconId::Users,
                page: TopPage::Users,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "App history",
                display: "tab.apphistory_short",
                icon: IconId::History,
                page: TopPage::AppHistory,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "Containers",
                display: "tab.containers",
                icon: IconId::Applications,
                page: TopPage::Containers,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ))
        .child(tab(
            TabProps {
                theme: t,
                label: "System",
                display: "tab.system",
                icon: IconId::System,
                page: TopPage::System,
                active,
                hovered,
                presentation,
                horizontal: false,
            },
            cx,
        ));

    div()
        .h_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .flex_shrink_0()
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .pl(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .pr(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .child(
            div()
                .debug_selector(|| "tm-navigation-rail".to_string())
                .h_full()
                .min_w(px(0.0))
                .w(px(crate::gpui_app::root::responsive::nav_rail_width(
                    presentation,
                )))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .bg(taskmanager_ui::theme_binding::fill(t.sidebar_bg))
                .rounded(taskmanager_ui::theme_binding::absolute(
                    tokens::card_radius(t),
                ))
                .border_1()
                .border_color(taskmanager_ui::theme_binding::hsla(t.border))
                .shadow_sm()
                .p(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .child(tabs)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_around()
                        .pt(taskmanager_ui::theme_binding::definite_length(
                            tokens::SPACE_6,
                        ))
                        .border_t_1()
                        .border_color(taskmanager_ui::theme_binding::hsla(t.border))
                        .flex_shrink_0()
                        // Vertical controls are stacked at the bottom instead
                        // of sharing a row. Compact icon targets keep both the
                        // 54px rail and localized labels bounded; hover
                        // exposes the full action label.
                        .when(
                            presentation == NavigationPresentation::IconOnly,
                            |controls| {
                                controls
                                    .flex_col()
                                    .justify_center()
                                    .gap(taskmanager_ui::theme_binding::definite_length(
                                        tokens::SPACE_2,
                                    ))
                                    .pt(taskmanager_ui::theme_binding::definite_length(
                                        tokens::SPACE_4,
                                    ))
                            },
                        )
                        .child({
                            // The vertical rail is a column regardless of
                            // whether its page tabs are labeled. The capture
                            // action has a localized label, so a horizontal
                            // controls row cannot satisfy the rail's width
                            // budget even in labeled mode.
                            let controls = div().flex().flex_col().items_center().gap(
                                taskmanager_ui::theme_binding::definite_length(tokens::SPACE_2),
                            );
                            let controls = controls.child(
                                super::super::window_capture::current_window_capture_btn(
                                    t, hovered, true, cx,
                                ),
                            );
                            controls
                                .child(nav_orientation_btn(t, hovered, cx))
                                .child(gear_btn(t, hovered, cx))
                        }),
                ),
        )
}
