//! Performance page body: replay control row, device view, and navigation.
//!
//! Renders the live/replay device view and the sidebar or device-strip
//! navigation for the active width class, from the frame-local render context.

use gpui::{Context, Div, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use taskmanager_theme::tokens;
use taskmanager_ui::layout::PageFrame;

use super::{PageRenderContext, RootView, SelectedDevice};
use crate::gpui_app::root::{cpu_view, elements, perf_views, responsive, sidebar};

const PERFORMANCE_REPLAY_ENTRY_HEIGHT: f32 = 30.0;

impl RootView {
    /// Render the Performance page: the replay control row, the device view,
    /// and the sidebar/device-strip navigation for the active width class.
    pub(super) fn render_performance_page(
        &mut self,
        cx: &mut Context<Self>,
        context: PageRenderContext<'_>,
    ) -> Div {
        let PageRenderContext {
            theme: t,
            snapshot: snap,
            telemetry,
            hovered,
            selected,
            frame,
            corner_radius_factor,
            presentation,
            ..
        } = context;
        let performance = self.performance_settings();
        let graph_cache = std::rc::Rc::clone(&self.graph_cache);
        let page_padding = frame.content.page_padding;
        let devices = presentation.devices;
        let sidebar_preferences = &presentation.sidebar;
        let hardware = self.hardware_rc().clone();
        // The replay action occupies a fixed row inside the same
        // Performance PageFrame as the device view. Account for it
        // before deriving the page budget; otherwise a lower graph
        // band could pass its fit check against pixels already owned
        // by the replay control and clip at the bottom.
        let replay_entry_visible =
            self.history_replay_startup_unavailable() || self.history_replay_entry_available();
        let mut performance_frame = frame;
        if replay_entry_visible {
            performance_frame.content.size.height =
                px((f32::from(performance_frame.content.size.height)
                    - PERFORMANCE_REPLAY_ENTRY_HEIGHT)
                    .max(0.0));
        }
        let performance_layout = responsive::PerformancePageBudget::from_frame(
            performance_frame,
            self.sidebar_visible,
            f32::from(sidebar_preferences.width),
        );
        let main = if self.history_replay_visible() {
            // Read-only history replay (roadmap #4): the persisted
            // series replace the live graphs while the panel is open.
            perf_views::history_replay::render_history_replay(
                t,
                self.history_replay_state(),
                &self.local_time_rules,
                performance_layout.content_height,
                cx.entity(),
                graph_cache.clone(),
            )
        } else if self.selected_device_missing {
            responsive::disconnected_device(t, self.stable_device_selection.selected_id())
                .into_any_element()
        } else {
            match selected {
                SelectedDevice::Cpu => cpu_view::render_cpu(
                    cpu_view::CpuViewProps {
                        theme: t,
                        snap,
                        telemetry,
                        hardware: &hardware,
                        hover_slot: &self.graph_hover,
                        graph_settings: performance.graph,
                        graph_cache: graph_cache.clone(),
                        layout: performance_layout,
                        units: self.display_units(),
                        package_power: cpu_view::PackagePowerInputs {
                            state: self.shell.rapl_power_state(),
                            capability: self.projection().capability_status(
                                &taskmanager_platform_contract::CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
                            ),
                        },
                        msr_readouts: cpu_view::MsrReadoutsInputs {
                            state: self.shell.msr_readout_state(),
                            capability: self.projection().capability_status(
                                &taskmanager_platform_contract::CapabilityId::TELEMETRY_CPU_MSR,
                            ),
                        },
                        details_scroll: &self.cpu_details_scroll,
                    },
                    &mut self.cpu_core_history,
                ),
                SelectedDevice::Memory => {
                    perf_views::render_memory(perf_views::MemoryViewProps {
                        theme: t,
                        snap,
                        telemetry,
                        performance,
                        hover_slot: &self.graph_hover,
                        graph_cache: graph_cache.clone(),
                        memory_history: &mut self.memory_history,
                        budget: performance_layout,
                    })
                }
                SelectedDevice::Disk(i) => perf_views::render_disk(
                    perf_views::DiskViewProps {
                        theme: t,
                        snap,
                        telemetry,
                        index: i,
                        performance,
                        directory_usage: self.directory_usage(),
                        hover_slot: &self.graph_hover,
                        graph_cache: graph_cache.clone(),
                        budget: performance_layout,
                    },
                    cx,
                ),
                SelectedDevice::Nic(i) => {
                    perf_views::render_network(perf_views::NetworkViewProps {
                        theme: t,
                        snap,
                        telemetry,
                        index: i,
                        performance,
                        hover_slot: &self.graph_hover,
                        graph_cache: graph_cache.clone(),
                        budget: performance_layout,
                    })
                }
                SelectedDevice::Gpu(i) => {
                    let engine_device_id = self.gpu_engine_rows_device_id(i);
                    perf_views::render_gpu(
                        t,
                        snap,
                        &self.live_graph_history,
                        i,
                        perf_views::GpuRenderState {
                            engine_session: self.shell.gpu_engine_rows_state(),
                            engine_capability_status: self.projection().capability_status(
                                &taskmanager_platform_contract::CapabilityId::TELEMETRY_GPU_ENGINES,
                            ),
                            engine_device_id,
                            chart_layout: perf_views::GpuChartLayout::for_chart_inventory(
                                performance_layout.chart_inventory,
                            ),
                            performance,
                            budget: performance_layout,
                            graph_cache: graph_cache.clone(),
                        },
                        &self.graph_hover,
                    )
                }
                SelectedDevice::Battery(i) => {
                    perf_views::render_battery(perf_views::BatteryViewProps {
                        theme: t,
                        power_supplies: self.power_supplies(),
                        telemetry,
                        index: i,
                        performance,
                        hover_slot: &self.graph_hover,
                        graph_cache: graph_cache.clone(),
                        budget: performance_layout,
                    })
                }
                SelectedDevice::Fan(i) => {
                    perf_views::render_fan(perf_views::FanViewProps {
                        theme: t,
                        sensors: self.sensors(),
                        telemetry,
                        index: i,
                        performance,
                        hover_slot: &self.graph_hover,
                        graph_cache: graph_cache.clone(),
                        budget: performance_layout,
                    })
                }
            }
            .into_any_element()
        };
        // History-replay entry point (roadmap #4): one toggle above
        // the graphs, present ONLY when persistence supplied a query —
        // disabled persistence shows nothing, never a dead button.
        let replay_toggle = if self.history_replay_startup_unavailable() {
            Some(
                div()
                    .id("tm-replay-locked")
                    .h(px(30.0))
                    .w_full()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .items_center()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                    .text_color(taskmanager_ui::theme_binding::hsla(t.fg_dim))
                    .child(taskmanager_application::i18n::t(
                        "perf.replay.startup_unavailable",
                    ))
                    .into_any_element(),
            )
        } else {
            self.history_replay_entry_available().then(|| {
                let ent = cx.entity();
                elements::tool_btn(
                    t,
                    "tm-replay-toggle",
                    taskmanager_application::i18n::t(if self.history_replay_state().is_open() {
                        "perf.replay.back_to_live"
                    } else {
                        "perf.replay.toggle"
                    }),
                    true,
                    self.history_replay_state().is_open(),
                    move |_win: &mut gpui::Window, cx: &mut gpui::App| {
                        ent.update(cx, |view, cx| {
                            view.toggle_history_replay(cx);
                            cx.notify();
                        });
                    },
                    move |_hovered: &bool, _win: &mut gpui::Window, _cx: &mut gpui::App| {},
                )
                .into_any_element()
            })
        };
        let content = PageFrame::new(
            div()
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .flex()
                // The replay entry renders BEFORE the graphs: block
                // layout gives the device view the remaining height,
                // so the toggle can never be pushed out of view.
                .children(replay_toggle.map(|button| {
                    div()
                        .id("tm-replay-entry")
                        .h(px(30.0))
                        .w_full()
                        .flex()
                        .flex_row()
                        .justify_end()
                        // The button keeps its intrinsic width: a
                        // squeezed toolbar control reads as a bare
                        // "…" box. Text slots yield; controls don't.
                        .child(div().flex_none().child(button))
                }))
                .child(main),
            px(page_padding),
        )
        // Performance owns a pinned right-edge rail inside every
        // device view. Keeping a second outer trailing inset leaves a
        // conspicuous empty strip beyond that rail.
        .right_padding(px(0.0))
        .render()
        .debug_selector(|| "tm-performance-page-frame".to_string());
        let mut body = div().flex_1().min_h(px(0.0)).min_w(px(0.0)).w_full().flex();
        if performance_layout.device_navigation == responsive::DeviceNavigationPresentation::Strip {
            body = body.flex_col();
            // When the persistent sidebar is hidden, the strip becomes
            // the accessible device switcher rather than disappearing
            // with it. The selected device must remain reachable at
            // every width.
            body = body.child(responsive::device_strip::device_strip(
                responsive::device_strip::DeviceStripProps {
                    theme: t,
                    snapshot: snap,
                    power_supplies: self.power_supplies(),
                    sensors: self.sensors(),
                    selected,
                    show_cpu: devices.cpu,
                    show_memory: devices.memory,
                    show_disks: devices.disks,
                    network_visibility: self.network_visibility(),
                    show_gpus: devices.gpus,
                    sidebar_order: &sidebar_preferences.order,
                    sidebar_device_overrides: &sidebar_preferences.device_overrides,
                },
                cx,
            ));
        } else if self.sidebar_visible {
            // Render-entry projection: the sidebar's CPU sparkline
            // shares the generation-keyed headline cache instead of
            // re-extracting the correlated history every frame (the
            // sidebar renders on every page).
            let sidebar_cpu_usage = self.cpu_core_history.aggregate(telemetry).usage;
            let sidebar_memory_usage = std::rc::Rc::clone(self.memory_history.refresh(telemetry).0);
            body = body.flex_row().child(sidebar::render_sidebar(
                sidebar::SidebarProps {
                    theme: t,
                    scroll: &self.sidebar_scroll,
                    width: px(performance_layout.sidebar_width),
                    snap,
                    telemetry,
                    cpu_usage_samples: sidebar_cpu_usage,
                    memory_usage_samples: sidebar_memory_usage,
                    power_supplies: self.power_supplies(),
                    sensors: self.sensors(),
                    selected,
                    show_cpu: devices.cpu,
                    show_memory: devices.memory,
                    show_disks: devices.disks,
                    network_visibility: self.network_visibility(),
                    show_gpus: devices.gpus,
                    performance,
                    sidebar_order: &sidebar_preferences.order,
                    sidebar_device_overrides: &sidebar_preferences.device_overrides,
                    edit_mode: self.sidebar_edit_mode,
                    hovered,
                    graph_cache: graph_cache.clone(),
                    corner_factor: corner_radius_factor,
                },
                cx,
            ));
        } else {
            body = body.flex_row();
        }
        body.child(content)
    }
}
