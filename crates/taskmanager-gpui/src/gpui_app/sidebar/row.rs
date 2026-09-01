//! GPUI rendering inputs and row projection for one sidebar device.

use gpui::{
    AnimationExt, App, AppContext, Context, ElementId, Empty, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use std::rc::Rc;
use taskmanager_theme::color::mix;
use taskmanager_ui::primitives::motion::{hover_animation, hover_state_key};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use crate::gpui_app::graph::{GraphOpts, GraphSettings, graph_element};
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_theme::{Color, Theme};

use super::SelectedDevice;
use taskmanager_theme::tokens;

/// Per-device sidebar row inputs (design-debt #1 props consolidation).
pub(super) struct DeviceRowProps<'a> {
    pub(super) theme: &'a Theme,
    pub(super) selected: SelectedDevice,
    pub(super) dev: SelectedDevice,
    pub(super) heading: String,
    pub(super) cap1: String,
    pub(super) cap2: String,
    /// Shared, generation-cached series (device rings or the CPU headline
    /// cache) — a UI-only frame pays one `Rc` clone.
    pub(super) samples: Rc<[f32]>,
    pub(super) base: Color,
    pub(super) max: f32,
    pub(super) graph_settings: GraphSettings,
    pub(super) hovered: Option<&'a Hover>,
    pub(super) id: ElementId,
    pub(super) icon: IconId,
    pub(super) key: String,
    pub(super) visible: bool,
    pub(super) edit_mode: bool,
}

#[derive(Clone)]
pub(super) struct SidebarDeviceDrag {
    pub(super) key: String,
    pub(super) order: Vec<String>,
}

impl Render for SidebarDeviceDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub(super) fn device_row(
    props: DeviceRowProps<'_>,
    rendered_order: Rc<[String]>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let DeviceRowProps {
        theme,
        selected,
        dev,
        heading,
        cap1,
        cap2,
        samples,
        base,
        max,
        graph_settings,
        hovered,
        id,
        icon,
        key,
        visible,
        edit_mode,
    } = props;
    let is_sel = selected == dev;
    let is_hov = !is_sel && hovered == Some(&Hover::Device(dev));
    let idle_bg = Color::TRANSPARENT;
    let hover_bg = theme.hover_bg();
    // Selected = the shared soft accent tint (`selection_bg`, same token the
    // processes table uses) — flat and unambiguous now that the raised-card
    // shadow is gone.
    let selected_bg = theme.selection_bg();
    let debug_key = key.clone();
    let opts = GraphOpts {
        max,
        ..GraphOpts::default()
    }
    .with_settings(graph_settings);
    let mut row = div()
        .id(id)
        .debug_selector(move || format!("sidebar-device:{debug_key}"))
        // WCAG 2.4.7 (Focus Visible): keyboard tab-stop + 2px accent outset ring while
        // focused. Additive to the selection/hover bg below (outset shadow, no layout
        // perturbation) — same pattern root/chrome.rs uses on tabs + gear.
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .on_click(cx.listener(move |view, _ev, _win, cx| {
            view.select_device(dev);
            cx.notify();
        }))
        .on_hover(cx.listener(move |view, is_hov: &bool, _win, cx| {
            view.set_hover(
                if *is_hov {
                    Some(Hover::Device(dev))
                } else {
                    None
                },
                cx,
            );
        }))
        .mx(taskmanager_ui::theme_binding::length(tokens::SPACE_8))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_7,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::card_radius(theme),
        ))
        // Static idle fill; the hover/selection fills are painted by an
        // animated overlay UNDER the content (keyed 120ms transition) — a
        // descendant of the focusable shell, never wrapping it (a keyed
        // animation id that changes between frames on a focused element's
        // ancestor path breaks gpui 0.2.2 key dispatch; the same absolute-
        // child pattern as the process-row selection rail).
        .bg(taskmanager_ui::theme_binding::fill(Color::TRANSPARENT))
        .relative()
        .child(
            div().absolute().inset_0().child(
                div()
                    .size_full()
                    .rounded(taskmanager_ui::theme_binding::absolute(
                        tokens::card_radius(theme),
                    ))
                    .with_animation(
                        ("sidebar-row-bg", hover_state_key(is_sel, is_hov)),
                        hover_animation(),
                        move |el, delta| {
                            let bg = if is_sel {
                                selected_bg
                            } else if is_hov {
                                mix(idle_bg, hover_bg, delta)
                            } else {
                                idle_bg
                            };
                            el.bg(taskmanager_ui::theme_binding::fill(bg))
                        },
                    ),
            ),
        )
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .child(
            taskmanager_ui::icons_binding::icon(icon)
                .size(px(16.0))
                .text_color(taskmanager_ui::theme_binding::hsla(if is_sel {
                    base
                } else {
                    theme.fg_dim
                })),
        )
        .child(
            div()
                .w(px(58.0))
                .h(px(34.0))
                // The props carry the cached projection as an `Rc`, so the element's
                // `Rc<[f32]>` conversion is the only copy on this path.
                // Move the shared `Rc` (NOT `.as_ref()`, which re-collects
                // into a fresh allocation and breaks the scene store's
                // identity key — every frame would rebuild the tessellation).
                .child(graph_element(
                    (ElementId::from("tm-sidebar-graph"), key.clone()),
                    samples,
                    taskmanager_ui::theme_binding::rgba(base),
                    opts,
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                // min_w(0) here is required so this flex_1 sibling can shrink beside
                // the fixed 58px sparkline (without it the row would overflow on a
                // narrow sidebar). The children below must NOT also set min_w(0) +
                // flex_1: that combo collapses their cross-axis (width) to ~0 in a
                // flex_col, which ellipsifies EVERY label to "..." (even "CPU"). The
                // children render at natural height + the column's stretched width,
                // and truncate() only fires for genuinely over-long device names.
                .min_w(px(0.0))
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_1,
                ))
                .child(
                    div()
                        .truncate()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                        .font_weight(taskmanager_ui::theme_binding::font_weight(
                            tokens::FONT_WEIGHT_STRONG,
                        ))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                        .child(heading),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                        .child(cap1),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                        .child(cap2),
                ),
        );

    if edit_mode {
        let accent = theme.accent;
        let drag_key = key.clone();
        let drop_key = key.clone();
        let rendered_order_for_drag = rendered_order;
        let override_key = key.clone();
        let debug_key = override_key.clone();
        row = row
            .on_drag(
                SidebarDeviceDrag {
                    key: drag_key,
                    order: Vec::new(),
                },
                move |drag, _, _, cx: &mut App| {
                    cx.stop_propagation();
                    cx.new(|_| SidebarDeviceDrag {
                        key: drag.key.clone(),
                        order: rendered_order_for_drag.to_vec(),
                    })
                },
            )
            .drag_over(move |mut style, _: &SidebarDeviceDrag, _, _| {
                style.border_color = Some(taskmanager_ui::theme_binding::hsla(accent));
                style
            })
            .on_drop(cx.listener(move |view, drag: &SidebarDeviceDrag, _, cx| {
                view.move_sidebar_device(&drag.key, &drop_key, &drag.order, cx);
            }))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "sidebar-device-toggle-{override_key}"
                    )))
                    .debug_selector(move || format!("sidebar-device-toggle-{debug_key}"))
                    .focusable()
                    .tab_stop(true)
                    .focus(elements::focus_ring(theme))
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _event, _window, cx| {
                        cx.stop_propagation();
                        view.set_sidebar_device_override(&override_key, !visible, cx);
                    }))
                    .text_color(taskmanager_ui::theme_binding::hsla(if visible {
                        theme.accent
                    } else {
                        theme.fg_dim
                    }))
                    .child(
                        taskmanager_ui::icons_binding::icon(if visible {
                            IconId::CircleCheck
                        } else {
                            IconId::CircleX
                        })
                        .size(px(14.0)),
                    ),
            );
    }
    // Selection/hover stays FLAT (owner-directed 2026-08-15): the earlier
    // raised-card shadow on every hovered/selected row read as a heavy blur
    // lifting the whole column while sweeping the list — hierarchy now comes
    // from the accent tint alone, matching the flat modern-task-manager
    // sidebar. The dense process table never shadowed rows and stays that way.
    row
}
