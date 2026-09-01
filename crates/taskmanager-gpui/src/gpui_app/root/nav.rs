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
use gpui::{
    AnimationExt, Context, DefiniteLength, InteractiveElement, IntoElement, Length, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};
use taskmanager_application::i18n;
use taskmanager_theme::Color;
use taskmanager_theme::color::mix;
use taskmanager_theme::tokens;
use taskmanager_ui::primitives::motion::{hover_animation, hover_state_key};
use taskmanager_ui_contract::IconId;

use super::responsive::NavigationPresentation;
use taskmanager_theme::Theme;

mod strips;
pub use strips::{nav_strip, nav_strip_horizontal, nav_strip_vertical};

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
            v.select_page(page);
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
        .px(taskmanager_ui::theme_binding::definite_length(
            match presentation {
                NavigationPresentation::IconOnly => tokens::SPACE_8,
                NavigationPresentation::Labeled => tokens::SPACE_14,
            },
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_7,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(t),
        ))
        .bg(taskmanager_ui::theme_binding::fill(base))
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(taskmanager_ui::theme_binding::absolute(
                        tokens::control_radius(t),
                    ))
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
                            el.bg(taskmanager_ui::theme_binding::fill(bg))
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
                        .rounded(taskmanager_ui::theme_binding::absolute(
                            tokens::xsmall_radius(t),
                        ))
                        .bg(taskmanager_ui::theme_binding::fill(accent))
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
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
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
            taskmanager_ui::theme_binding::font_weight(tokens::FONT_WEIGHT_BOLD)
        } else {
            taskmanager_ui::theme_binding::font_weight(tokens::FONT_WEIGHT_NORMAL)
        })
        .text_color(taskmanager_ui::theme_binding::hsla(fg))
        .child(taskmanager_ui::icons_binding::icon(icon).size(px(18.0)))
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
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(t),
        ))
        .bg(taskmanager_ui::theme_binding::fill(transparent))
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(taskmanager_ui::theme_binding::absolute(
                        tokens::control_radius(t),
                    ))
                    .with_animation(
                        ("gear-bg", hover_state_key(false, is_hov)),
                        hover_animation(),
                        move |el, delta| {
                            if is_hov {
                                el.bg(taskmanager_ui::theme_binding::fill(mix(
                                    transparent,
                                    hover_bg,
                                    delta,
                                )))
                            } else {
                                el.bg(taskmanager_ui::theme_binding::fill(transparent))
                            }
                        },
                    ),
            ),
        )
        .flex()
        .items_center()
        .justify_center()
        .child(
            taskmanager_ui::icons_binding::icon(IconId::Settings)
                .size(px(16.0))
                .text_color(taskmanager_ui::theme_binding::hsla(color)),
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
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(t),
        ))
        .bg(taskmanager_ui::theme_binding::fill(transparent))
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(taskmanager_ui::theme_binding::absolute(
                        tokens::control_radius(t),
                    ))
                    .with_animation(
                        ("nav-orientation-bg", hover_state_key(false, is_hov)),
                        hover_animation(),
                        move |el, delta| {
                            if is_hov {
                                el.bg(taskmanager_ui::theme_binding::fill(mix(
                                    transparent,
                                    hover_bg,
                                    delta,
                                )))
                            } else {
                                el.bg(taskmanager_ui::theme_binding::fill(transparent))
                            }
                        },
                    ),
            ),
        )
        .flex()
        .items_center()
        .justify_center()
        .child(
            taskmanager_ui::icons_binding::icon(IconId::Sidebar)
                .size(px(16.0))
                .text_color(taskmanager_ui::theme_binding::hsla(color)),
        )
}
