//! Page-navigation strip: a horizontal tab row (Performance / Apps / Services /
//! Startup / Users / Containers / System) rendered BELOW the titlebar.
//!
//! This is APP CONTENT, not window chrome — it renders in BOTH decoration modes:
//! * Server decorations granted (KDE/KWin, macOS, Windows): below the native
//!   titlebar the compositor/OS drew.
//! * Compositor-forced CSD (GNOME/Mutter, some tiling WMs): below our own
//!   `chrome::top_bar` CSD titlebar.
//!
//! Navigation was extracted out of `chrome::top_bar` so the titlebar holds ONLY
//! window chrome (drag + title + window controls) and page switching is decoupled
//! from the decoration negotiation. Clicking a tab still flips `RootView::page`
//! through the same `on_click` wiring the in-titlebar tabs used.
//!
//! The settings (gear) affordance lives at the RIGHT end of this strip: it used
//! to sit in the CSD titlebar, but in native-decorations mode there is no
//! app-painted titlebar, so the strip is the always-present home that keeps
//! Settings reachable in both modes.

use super::{Hover, RootView, TopPage};
use crate::gpui_app::elements;
use crate::gpui_app::icons;
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::{Color, Theme};
use crate::i18n;
use gpui::{
    AnimationExt, Context, DefiniteLength, Div, InteractiveElement, IntoElement, Length,
    ParentElement, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use taskmanager_theme::color::mix;
use taskmanager_ui::primitives::motion::{hover_animation, hover_state_key};
use taskmanager_ui_contract::IconId;

use super::responsive::NavigationPresentation;

/// One nav-strip tab. `label` is the **identity** (English) string reused as the
/// gpui element id, the focus target, and the [`Hover::Static`] identity that
/// [`super::chrome::static_label`] matches on to produce the tooltip — it must
/// stay stable across languages so hover/tooltip wiring is locale-independent.
/// `display` is the i18n message key ([`i18n::t`]) rendered as the visible text;
/// passing it separately (rather than deriving it from `label`) keeps the
/// key/identity coupling explicit at the call site and avoids a brittle
/// string→key mapper.
pub struct TabProps<'a> {
    pub theme: &'a Theme,
    pub label: &'static str,
    pub display: &'static str,
    pub icon: IconId,
    pub page: TopPage,
    pub active: TopPage,
    pub hovered: Option<&'a Hover>,
    pub presentation: NavigationPresentation,
    /// Horizontal tabs share the available row width; vertical tabs retain
    /// their intrinsic height and fill the rail width.
    pub horizontal: bool,
}

pub fn tab(props: TabProps<'_>, cx: &mut Context<RootView>) -> impl IntoElement {
    let TabProps {
        theme: t,
        label,
        display,
        icon,
        page,
        active,
        hovered,
        presentation,
        horizontal,
    } = props;
    let is_act = page == active;
    let is_hov = !is_act && hovered == Some(&Hover::Static(label));
    let fg: Color = if is_act {
        t.accent_text
    } else if is_hov {
        t.fg
    } else {
        t.fg_dim
    };
    // The tab background is a keyed 120ms transition painted by an absolute
    // overlay UNDER the content (idle → base, hovered → translucent accent
    // tint eased in, active → solid accent). The overlay must stay a
    // DESCENDANT of the focusable shell: a keyed animation id that changes
    // between frames on a focused element's ancestor path breaks gpui 0.2.2
    // key dispatch, so the animation never wraps the shell (same pattern as
    // the process-row selection rail: absolute child, paints beneath).
    let base = t.sidebar_card_bg;
    let hover = t.accent.with_alpha(0.12);
    let accent = t.accent;
    div()
        .id(label)
        // Zero-cost debug tag (no-op in release builds) so render tests can
        // locate a tab by its stable English identity and read/click its bounds.
        .debug_selector(move || label.to_string())
        .on_click(cx.listener(move |v, _ev, _win, cx| {
            v.page = page;
            cx.notify();
        }))
        .on_hover(cx.listener(move |v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static(label))
                } else {
                    None
                },
                cx,
            );
        }))
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(t))
        .px(match presentation {
            NavigationPresentation::IconOnly => tokens::SPACE_8,
            NavigationPresentation::Labeled => tokens::SPACE_14,
        })
        .py(tokens::SPACE_7)
        .rounded(tokens::control_radius(t))
        .bg(base)
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(tokens::control_radius(t))
                    .with_animation(
                        ("tab-bg", hover_state_key(is_act, is_hov)),
                        hover_animation(),
                        move |el, delta| {
                            let bg = if is_act {
                                accent
                            } else if is_hov {
                                mix(base, hover, delta)
                            } else {
                                base
                            };
                            el.bg(bg)
                        },
                    ),
            ),
        )
        // Mission-Center-style selected indicator: a 3px accent underline
        // that grows across the tab when it becomes active and shrinks back
        // when it loses the selection. Painted by the same absolute-overlay
        // child pattern as the background (never wraps the focusable shell).
        .child(
            div()
                .absolute()
                .bottom(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .h(px(3.0))
                .child(
                    div()
                        .h_full()
                        .rounded(tokens::xsmall_radius(t))
                        .bg(accent)
                        .with_animation(
                            ("tab-indicator", hover_state_key(is_act, false)),
                            hover_animation(),
                            move |el, delta| {
                                let width = if is_act { delta } else { 1.0 - delta };
                                el.w(Length::Definite(DefiniteLength::Fraction(width)))
                            },
                        ),
                ),
        )
        .flex()
        .items_center()
        // Horizontal tabs divide the strip evenly, so their icon+label group
        // must be centered in that allocated cell. Compact tabs contain only
        // the icon; vertical labeled tabs keep the rail's leading alignment.
        .when(
            horizontal || presentation == NavigationPresentation::IconOnly,
            |tab| tab.justify_center(),
        )
        .gap(tokens::SPACE_6)
        // Elastic shrink: the tab is a flex child of the nav strip's tabs row.
        // min_w(0) overrides flex's default min-width:auto (= the content's
        // natural width) so this tab can shrink below content width when the
        // window narrows; the label below uses elements::truncated_text so it
        // ellipses cleanly instead of pushing the gear off-screen. The icon
        // stays visible at every width so the tab remains identifiable even once
        // the text truncates to "Performa…".
        .min_w(px(0.0))
        .when(horizontal, |tab| tab.flex_1())
        .when(!horizontal, |tab| tab.w_full().flex_shrink_0())
        .font_weight(if is_act {
            tokens::FONT_WEIGHT_BOLD.into()
        } else {
            tokens::FONT_WEIGHT_NORMAL.into()
        })
        .text_color(fg)
        .child(icons::icon(icon).size(px(18.0)))
        // Label wraps min_w(0)+truncate so it shrinks/ellipses inside the tab's
        // flex row instead of forcing the tab to its natural text width. Text
        // styling is inherited from the tab div above. A hover tooltip (the full
        // label + Alt+N shortcut) is already wired via `Hover::Static(label)` +
        // `static_label`, rendered at the cursor by root.rs — so a truncated tail
        // is recoverable on hover. The visible text resolves through [`i18n::t`]
        // against the active language; the hover/tooltip identity stays `label`
        // (English) so it round-trips through `static_label` regardless of locale.
        .when(presentation == NavigationPresentation::Labeled, |tab| {
            // The label is the only shrinking child inside a tab. Its zero
            // minimum width lets a long locale ellipsize without pushing the
            // icon or trailing controls outside the bounded navigation region.
            tab.child(elements::truncated_text(i18n::t(display)).flex_shrink())
        })
}

/// The settings (gear) affordance; opens the Settings modal. The gear is a crisp
/// PathBuilder glyph (not an emoji), themed fg_dim → accent on hover.
pub fn gear_btn(
    t: &Theme,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let is_hov = hovered == Some(&Hover::Static("settings-btn"));
    let color = if is_hov { t.accent } else { t.fg_dim };
    // The gear background is a keyed 120ms transition painted by an absolute
    // overlay under the icon (transparent → translucent accent tint, eased in
    // on hover) — a descendant of the focusable shell, never wrapping it (see
    // `tab` for the gpui 0.2.2 key-dispatch constraint).
    let transparent = Color::TRANSPARENT;
    let hover_bg = t.accent.with_alpha(0.12);
    div()
        .id("settings-btn")
        // Zero-cost debug tag (no-op in release) so render tests can confirm the
        // gear is reachable in both decoration modes (it moved out of the CSD
        // titlebar into this always-present strip).
        .debug_selector(|| "settings-btn".to_string())
        .on_click(cx.listener(|v, _ev, _win, cx| {
            v.toggle_settings();
            // Clear hover so a stale tooltip doesn't linger over the modal (FIX 2/2).
            v.hovered = None;
            cx.notify();
        }))
        .on_hover(cx.listener(|v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static("settings-btn"))
                } else {
                    None
                },
                cx,
            );
        }))
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(t))
        .px(tokens::SPACE_8)
        .py(tokens::SPACE_6)
        .rounded(tokens::control_radius(t))
        .bg(transparent)
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(tokens::control_radius(t))
                    .with_animation(
                        ("gear-bg", hover_state_key(false, is_hov)),
                        hover_animation(),
                        move |el, delta| {
                            if is_hov {
                                el.bg(mix(transparent, hover_bg, delta))
                            } else {
                                el.bg(transparent)
                            }
                        },
                    ),
            ),
        )
        .flex()
        .items_center()
        .justify_center()
        .child(
            icons::icon(IconId::Settings)
                .size(px(16.0))
                .text_color(color),
        )
}

/// The navigation orientation affordance (toggles horizontal vs vertical nav).
pub fn nav_orientation_btn(
    t: &Theme,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let is_hov = hovered == Some(&Hover::Static("nav-orientation-btn"));
    let color = if is_hov { t.accent } else { t.fg_dim };
    let transparent = Color::TRANSPARENT;
    let hover_bg = t.accent.with_alpha(0.12);
    div()
        .id("nav-orientation-btn")
        .debug_selector(|| "nav-orientation-btn".to_string())
        .on_click(cx.listener(|v, _ev, _win, cx| {
            v.nav_orientation = match v.nav_orientation {
                super::NavOrientation::Horizontal => super::NavOrientation::Vertical,
                super::NavOrientation::Vertical => super::NavOrientation::Horizontal,
            };
            v.hovered = None;
            cx.notify();
        }))
        .on_hover(cx.listener(|v, is_hov: &bool, _win, cx| {
            v.set_hover(
                if *is_hov {
                    Some(Hover::Static("nav-orientation-btn"))
                } else {
                    None
                },
                cx,
            );
        }))
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(t))
        .px(tokens::SPACE_8)
        .py(tokens::SPACE_6)
        .rounded(tokens::control_radius(t))
        .bg(transparent)
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(tokens::control_radius(t))
                    .with_animation(
                        ("nav-orientation-bg", hover_state_key(false, is_hov)),
                        hover_animation(),
                        move |el, delta| {
                            if is_hov {
                                el.bg(mix(transparent, hover_bg, delta))
                            } else {
                                el.bg(transparent)
                            }
                        },
                    ),
            ),
        )
        .flex()
        .items_center()
        .justify_center()
        .child(
            icons::icon(IconId::Sidebar)
                .size(px(16.0))
                .text_color(color),
        )
}

/// The page-navigation strip: a floating rounded row or column of page tabs.
pub fn nav_strip(
    t: &Theme,
    active: TopPage,
    orientation: super::NavOrientation,
    hovered: Option<&Hover>,
    presentation: NavigationPresentation,
    cx: &mut Context<RootView>,
) -> Div {
    match orientation {
        super::NavOrientation::Horizontal => {
            nav_strip_horizontal(t, active, hovered, presentation, cx)
        }
        super::NavOrientation::Vertical => nav_strip_vertical(t, active, hovered, presentation, cx),
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
        .gap(tokens::SPACE_4)
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
        .px(tokens::SPACE_12)
        .pt(tokens::SPACE_6)
        .pb(tokens::SPACE_4)
        .child(
            div()
                .debug_selector(|| "tm-navigation-strip".to_string())
                .h(px(crate::gpui_app::chrome::titlebar_height(t) + 4.0))
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_row()
                .items_center()
                .bg(t.sidebar_bg)
                .rounded(tokens::card_radius(t))
                .border_1()
                .border_color(t.border)
                .shadow_sm()
                .px(tokens::SPACE_6)
                .child(tabs)
                .child(div().w(tokens::SPACE_6).flex_shrink_0())
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(tokens::SPACE_2)
                        .child(nav_orientation_btn(t, hovered, cx))
                        .child(gear_btn(t, hovered, cx)),
                ),
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
        .gap(tokens::SPACE_4)
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
        .py(tokens::SPACE_6)
        .pl(tokens::SPACE_8)
        .pr(tokens::SPACE_4)
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
                .bg(t.sidebar_bg)
                .rounded(tokens::card_radius(t))
                .border_1()
                .border_color(t.border)
                .shadow_sm()
                .p(tokens::SPACE_6)
                .child(tabs)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_around()
                        .pt(tokens::SPACE_6)
                        .border_t_1()
                        .border_color(t.border)
                        .flex_shrink_0()
                        // An icon-only rail is intentionally only
                        // 54px wide; two 32px controls cannot share that row.
                        // Stack them at the bottom instead of letting the
                        // settings hit target escape the rail.
                        .when(
                            presentation == NavigationPresentation::IconOnly,
                            |controls| {
                                controls
                                    .flex_col()
                                    .justify_center()
                                    .gap(tokens::SPACE_2)
                                    .pt(tokens::SPACE_4)
                            },
                        )
                        .child(nav_orientation_btn(t, hovered, cx))
                        .child(gear_btn(t, hovered, cx)),
                ),
        )
}
