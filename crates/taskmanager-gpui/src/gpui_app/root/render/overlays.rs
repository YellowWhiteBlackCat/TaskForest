//! Composition of root-owned dashboard, Settings, and run-task overlays.

use super::super::{
    RootView, Theme, dashboard, elements, i18n, init_run_entity, responsive, settings_view,
};
use crate::gpui_app::help_overlay;
use crate::gpui_app::root::window_surface::WindowSurface;
use crate::gpui_app::system_about;
use crate::gpui_app::{about, first_run};
use gpui::{
    AnyElement, App, AppContext, Context, Div, IntoElement, ParentElement, Stateful, Styled,
    Window, div, px,
};
use taskmanager_theme::tokens;
use taskmanager_ui::inputs::switch::SwitchState;
use taskmanager_ui::inputs::text_input::TextInput;
use taskmanager_ui::layout::{
    BoundedScrollRailSpec, bounded_scroll_column_with_fixed_header, bounded_scroll_region_with_rail,
};
use taskmanager_ui::primitives::button::{Button, ButtonVariant};
use taskmanager_ui::primitives::spinner::Spinner;
use taskmanager_ui_contract::IconId;
pub(super) fn compose_primary_dialogs(
    view: &mut RootView,
    root: Stateful<Div>,
    surface: &WindowSurface,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let close_entity = cx.entity();
    let root = if let WindowSurface::DashboardPanel(panel) = surface {
        root.child(dashboard::render_panel_overlay(
            dashboard::DashboardPanelOverlayProps {
                theme,
                panel: *panel,
                state: &view.dashboard,
                events: view.projection().alert_center.event_history(),
                rules: view.projection().alert_center.managed_rules(),
                entity: close_entity.clone(),
                scroll: view.dialog_scroll.dashboard_panel.clone(),
            },
            window,
            cx,
        ))
    } else {
        root
    };

    let root = if matches!(surface, WindowSurface::Settings) {
        let presentation = view.presentation_snapshot();
        let appearance = presentation.appearance;
        let devices = presentation.devices;
        let close_ent = close_entity.clone();
        let on_close = move |_win: &mut Window, cx: &mut App| {
            close_ent.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::Settings,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        };
        // Build content before the Dialog so the Context borrow ends first.
        // The refresh slider's persistent SliderState is owned per window on
        // `RootView::settings_slider`, created lazily on the first Settings
        // render (see `settings_view::refresh::init_slider_entity`) — the shared
        // `thread_local` it replaced leaked drag state across windows.
        let refresh_secs = view
            .telemetry_refresh_policy
            .interval()
            .duration()
            .as_secs_f32();
        let slider_entity = view
            .settings_slider
            .get_or_insert_with(|| {
                settings_view::refresh::init_slider_entity(refresh_secs, &mut *cx)
            })
            .clone();
        let graph_points = view.performance_settings().graph.data_points;
        let graph_points_slider = if let Some(slider) = view.graph_points_slider.clone() {
            slider
        } else {
            let slider = settings_view::init_data_points_slider(graph_points, &mut *cx);
            view.graph_points_slider = Some(slider.clone());
            slider
        };
        for id in [
            "hc-switch",
            "device-cpu",
            "device-memory",
            "device-disks",
            "device-network",
            "network-wired",
            "network-wireless",
            "network-vpn",
            "network-virtual",
            "network-other",
            "device-gpus",
            "gray-zero-values",
            "smooth-graphs",
            "sliding-graphs",
            "network-dynamic-scaling",
            "desktop-notifications",
            "continuous-history",
        ] {
            view.settings_switches
                .entry(id)
                .or_insert_with(|| cx.new(|cx| SwitchState::new(cx)));
        }
        let content = settings_view::render_settings(
            settings_view::SettingsViewProps {
                theme,
                hovered: view.hovered.as_ref(),
                refresh_secs,
                show_cpu: devices.cpu,
                show_memory: devices.memory,
                show_disks: devices.disks,
                show_network: devices.network,
                show_network_wired: devices.network_wired,
                show_network_wireless: devices.network_wireless,
                show_network_vpn: devices.network_vpn,
                show_network_virtual: devices.network_virtual,
                show_network_other: devices.network_other,
                show_gpus: devices.gpus,
                units: presentation.units,
                graph_settings: view.performance_settings().graph,
                graph_points_slider,
                gray_zero_values: presentation.gray_zero_values,
                notify_enabled: view.projection().alert_center.policy().enabled,
                history_persistence: view.history_runtime.enabled_next_start(),
                first_run: &view.first_run,
                notify_quiet_start: view
                    .projection()
                    .alert_center
                    .policy()
                    .quiet_hours
                    .map_or(0, |hours| (hours.start_minutes / 60) as u8),
                notify_quiet_end: view
                    .projection()
                    .alert_center
                    .policy()
                    .quiet_hours
                    .map_or(0, |hours| (hours.end_minutes / 60) as u8),
                font_pref: appearance.font,
                font_availability: &view.font_availability,
                density: appearance.density,
                ui_size: appearance.ui_size,
                text_rendering: appearance.text_rendering,
                color_scheme: appearance.color_scheme,
                startup_page: presentation.startup_page,
                slider_entity,
                switches: &view.settings_switches,
            },
            cx,
        );
        if view.capture_evidence.settings_zero_gray_enabled() {
            view.dialog_scroll.settings.scroll_to_bottom();
        }
        let settings_max_height = responsive::settings_content_max_height(window.viewport_size());
        // Keep the scroll affordance in the dialog viewport instead of letting
        // the native overflow hint disappear into the panel. Settings is long
        // enough to scroll even at 1180×780, and the compact 720×480 contract
        // must make that fact discoverable without relying on a wheel gesture.
        let settings_scroll = view.dialog_scroll.settings.clone();
        let content: AnyElement = bounded_scroll_region_with_rail(
            BoundedScrollRailSpec {
                id: "settings-scroll-viewport",
                viewport_selector: "tm-settings-scroll-viewport",
                scrollbar_id: "settings-scrollbar",
                scrollbar_selector: "tm-settings-scrollbar",
                track_selector: "tm-settings-scrollbar-track",
                width: None,
                max_height: px(settings_max_height),
                scroll: settings_scroll,
                palette: theme.palette(),
            },
            content,
        )
        .into_any_element();
        root.child(elements::dialog_overlay(
            theme,
            &mut *window,
            &mut *cx,
            i18n::t("chrome.settings"),
            on_close,
            content,
        ))
    } else {
        root
    };

    let root = if matches!(surface, WindowSurface::Help) {
        let close_ent = close_entity.clone();
        let on_close = move |_win: &mut Window, cx: &mut App| {
            close_ent.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::Help,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        };
        root.child(help_overlay::render_help_overlay(
            theme,
            &mut *window,
            &mut *cx,
            view.dialog_scroll.help.clone(),
            on_close,
        ))
    } else {
        root
    };

    let root = if matches!(surface, WindowSurface::About) {
        let close_ent = close_entity.clone();
        let on_close = move |_win: &mut Window, cx: &mut App| {
            close_ent.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::About,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        };
        let viewport = window.viewport_size();
        let max_dialog_width = (f32::from(viewport.width) - 48.0).max(300.0);
        let dialog_width = max_dialog_width.min(520.0);
        let content_width = (dialog_width - 50.0).max(250.0);
        let content: AnyElement = bounded_scroll_region_with_rail(
            BoundedScrollRailSpec {
                id: "about-scroll",
                viewport_selector: "tm-about-scroll",
                scrollbar_id: "about-scrollbar",
                scrollbar_selector: "tm-about-scrollbar",
                track_selector: "tm-about-scrollbar-track",
                width: Some(px(content_width)),
                max_height: px((f32::from(viewport.height) - 130.0).max(220.0)),
                scroll: view.dialog_scroll.about.clone(),
                palette: theme.palette(),
            },
            about::render_about(theme, close_entity.clone()),
        )
        .into_any_element();
        root.child(elements::dialog_overlay_width(
            theme,
            &mut *window,
            &mut *cx,
            px(dialog_width),
            i18n::t("about.title"),
            on_close,
            content,
        ))
    } else {
        root
    };

    let root = if matches!(surface, WindowSurface::FirstRun) {
        let close_ent = close_entity.clone();
        let on_close = move |_win: &mut Window, cx: &mut App| {
            close_ent.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::FirstRun,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        };
        let viewport = window.viewport_size();
        let max_dialog_width = (f32::from(viewport.width) - 48.0).max(320.0);
        let dialog_width = max_dialog_width.min(620.0);
        let content_width = (dialog_width - 50.0).max(280.0);
        let content: AnyElement = bounded_scroll_region_with_rail(
            BoundedScrollRailSpec {
                id: "first-run-scroll",
                viewport_selector: "tm-first-run-scroll",
                scrollbar_id: "first-run-scrollbar",
                scrollbar_selector: "tm-first-run-scrollbar",
                track_selector: "tm-first-run-scrollbar-track",
                width: Some(px(content_width)),
                max_height: px((f32::from(viewport.height) - 130.0).max(240.0)),
                scroll: view.dialog_scroll.first_run.clone(),
                palette: theme.palette(),
            },
            first_run::render_first_run(theme, &view.first_run, close_entity.clone()),
        )
        .into_any_element();
        root.child(elements::dialog_overlay_width(
            theme,
            &mut *window,
            &mut *cx,
            px(dialog_width),
            i18n::t("first_run.title"),
            on_close,
            content,
        ))
    } else {
        root
    };

    let root = if matches!(surface, WindowSurface::SystemAbout) {
        let close_ent = close_entity.clone();
        let on_close = move |_win: &mut Window, cx: &mut App| {
            close_ent.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::SystemAbout,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        };
        let viewport = window.viewport_size();
        let max_dialog_width = (f32::from(viewport.width) - 64.0).max(300.0);
        let dialog_width = max_dialog_width.min(620.0);
        let content_width = (dialog_width - 32.0).max(260.0);
        let body_height = (f32::from(viewport.height) - 130.0).max(220.0);
        let scroll_height = (body_height - 36.0).max(184.0);
        let system_about = system_about::render_system_about(
            theme,
            view.hardware(),
            view.desktop_appearance,
            close_entity.clone(),
            cx,
        );
        let content: AnyElement = bounded_scroll_column_with_fixed_header(
            BoundedScrollRailSpec {
                id: "system-about-scroll",
                viewport_selector: "tm-system-about-scroll",
                scrollbar_id: "system-about-scrollbar",
                scrollbar_selector: "tm-system-about-scrollbar",
                track_selector: "tm-system-about-scrollbar-track",
                width: Some(px(content_width)),
                max_height: px(scroll_height),
                scroll: view.dialog_scroll.system_about.clone(),
                palette: theme.palette(),
            },
            tokens::SPACE_12,
            system_about.actions,
            system_about.groups,
        )
        .into_any_element();
        root.child(elements::dialog_overlay_width(
            theme,
            &mut *window,
            &mut *cx,
            px(dialog_width),
            i18n::t("system_about.title"),
            on_close,
            content,
        ))
    } else {
        root
    };

    // The run-task dialog remains the sole execution path for this root-owned
    // modal; extracting composition does not change its callbacks.
    if matches!(surface, WindowSurface::RunTask) {
        let close_ent = close_entity.clone();
        let on_close = move |_win: &mut Window, cx: &mut App| {
            close_ent.update(cx, |view, cx| {
                view.dismiss_window_surface(
                    crate::gpui_app::root::WindowSurfaceKind::RunTask,
                    crate::gpui_app::root::WindowSurfaceDismissReason::CloseButton,
                );
                cx.notify();
            });
        };
        let ent_cancel = close_entity.clone();
        let ent_run = close_entity;
        let run_error = view.run_error.clone();
        let content: AnyElement = {
            let entity = view.run_input.get_or_insert_with(|| init_run_entity(cx));
            let input = div()
                .w(px(360.0))
                .child(TextInput::new(entity.clone(), theme.palette()).height(30.0));
            let mut column = div().flex().flex_col().gap(tokens::SPACE_12).child(input);
            if let Some(error) = &run_error {
                column = column.child(
                    div()
                        .w(px(360.0))
                        .text_size(tokens::FONT_12)
                        .text_color(theme.danger)
                        .child(error.clone()),
                );
            }
            column
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(tokens::SPACE_8)
                        .justify_end()
                        .child(elements::pill(
                            theme,
                            "run-cancel",
                            i18n::t("common.cancel"),
                            false,
                            false,
                            move |_window, cx| {
                                ent_cancel.update(cx, |view, cx| {
                                    view.dismiss_window_surface(
                                        crate::gpui_app::root::WindowSurfaceKind::RunTask,
                                        crate::gpui_app::root::WindowSurfaceDismissReason::Completed,
                                    );
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        ))
                        .child(elements::pill(
                            theme,
                            "run-confirm",
                            i18n::t("common.run"),
                            true,
                            false,
                            move |_window, cx| {
                                ent_run.update(cx, |view, cx| {
                                    view.request_run_command(cx);
                                    cx.notify();
                                });
                            },
                            |_, _, _| {},
                        )),
                )
                .into_any_element()
        };
        root.child(elements::dialog_overlay(
            theme,
            &mut *window,
            &mut *cx,
            i18n::t("proc.run_new_task"),
            on_close,
            content,
        ))
    } else {
        root
    }
}

/// Cold-start loading placeholder: a centered spinner + status line shown
/// until the first complete telemetry frame is committed.
pub(super) fn cold_start_placeholder(
    theme: &Theme,
    _window: &mut Window,
    _cx: &mut Context<RootView>,
) -> Div {
    div().flex_1().flex().items_center().justify_center().child(
        div()
            .flex()
            .items_center()
            .gap(tokens::SPACE_10)
            .text_color(theme.fg_dim)
            .text_size(tokens::FONT_13)
            .child(Spinner::new(theme.palette()).size(16.0))
            .child(taskmanager_application::i18n::t(
                "common.collecting_telemetry",
            )),
    )
}

pub(super) fn warmup_retry_button(
    view: &mut RootView,
    theme: &Theme,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Button {
    let (state, focus_on_mount) = match view.telemetry_warmup_retry_button.as_ref() {
        Some(state) => (state.clone(), false),
        None => {
            let state = cx.new(|cx| taskmanager_ui::primitives::button::ButtonState::new(cx));
            view.telemetry_warmup_retry_button = Some(state.clone());
            (state, true)
        }
    };

    if focus_on_mount {
        let focus = state.read(cx).focus_handle().clone();
        window.defer(cx, move |window, _cx| focus.focus(window));
    }

    let root = cx.entity().downgrade();
    Button::new(state, theme.palette())
        .variant(ButtonVariant::Secondary)
        .icon(IconId::Refresh)
        .label(i18n::t("common.refresh"))
        .on_activate(move |_event, _window, cx| {
            let _ = root.update(cx, |view, cx| view.retry_telemetry_warmup(cx));
        })
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_root_render_overlays_tests.rs"]
mod tests;
