//! Per-page body rendering for the root application shell (line split).

use super::super::{Hover, RootView, SelectedDevice, SystemHealthCallbacks, TopPage};
use super::init_search_entity;
use crate::gpui_app::app_history_view;
use crate::gpui_app::dashboard::SystemSection;
use crate::gpui_app::root::{
    containers_view, cpu_view, dashboard, elements, i18n, perf_views, processes_view, responsive,
    sidebar, system_health_view, system_view,
};
use gpui::{
    AnyElement, AppContext, Context, Div, InteractiveElement, IntoElement, ParentElement, Styled,
    Window, div, px,
};
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_telemetry_store::TelemetryStore;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::{PageFrame, PageScaffold};

mod inventory;
mod vm;

pub(crate) struct PageBodyFrame<'a> {
    pub theme: &'a Theme,
    pub snapshot: &'a SystemSnapshot,
    pub telemetry: &'a TelemetryStore,
    pub hovered: Option<&'a Hover>,
    pub selected: SelectedDevice,
    pub frame: responsive::FrameBudget,
    pub corner_radius_factor: f32,
    pub selected_pid: Option<u32>,
}

impl RootView {
    /// Render the active page from one immutable, frame-local projection.
    /// `PageBodyFrame` is stack-owned; it is neither an allocation nor cache.
    pub(crate) fn render_page_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        frame: PageBodyFrame<'_>,
    ) -> Div {
        let PageBodyFrame {
            theme: t,
            snapshot: snap,
            telemetry,
            hovered,
            selected,
            frame,
            corner_radius_factor,
            selected_pid: sel_pid,
        } = frame;
        let layout = frame.page_layout();
        let performance = self.performance_settings();
        let page_padding = frame.content.page_padding;
        let presentation = self.presentation_snapshot();
        let devices = presentation.devices;
        let sidebar_preferences = &presentation.sidebar;
        let appearance = presentation.appearance;
        // This width already excludes navigation and page padding. Every
        // non-Performance page consumes the same root-owned slot instead of
        // subtracting shell regions a second time.
        let content_width = px(f32::from(frame.content.size.width));
        let source_retry_button = self
            .source_retry_button
            .get_or_insert_with(|| {
                cx.new(|cx| taskmanager_ui::primitives::button::ButtonState::new(cx))
            })
            .clone();
        match self.page {
            TopPage::Performance => {
                let hardware = self.hardware_rc().clone();
                let performance_layout = responsive::PerformancePageBudget::from_frame(
                    frame,
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
                        cx.entity(),
                    )
                } else if self.selected_device_missing {
                    responsive::disconnected_device(t, self.stable_device_selection.selected_id())
                        .into_any_element()
                } else {
                    match selected {
                        SelectedDevice::Cpu => cpu_view::render_cpu(
                            cpu_view::CpuViewProps {
                                theme: t,
                                stats_scroll: self.performance_stats_scroll.clone(),
                                snap,
                                telemetry,
                                hardware: &hardware,
                                hover_slot: &self.graph_hover,
                                graph_settings: performance.graph,
                                layout: performance_layout,
                            },
                            &mut self.cpu_core_history,
                        ),
                        SelectedDevice::Memory => {
                            perf_views::render_memory(perf_views::MemoryViewProps {
                                theme: t,
                                snap,
                                telemetry,
                                performance,
                                stats_scroll: self.performance_stats_scroll.clone(),
                                hover_slot: &self.graph_hover,
                                memory_history: &mut self.memory_history,
                                budget: performance_layout,
                            })
                        }
                        SelectedDevice::Disk(i) => perf_views::render_disk(
                            perf_views::DiskViewProps {
                                theme: t,
                                stats_scroll: self.performance_stats_scroll.clone(),
                                snap,
                                telemetry,
                                index: i,
                                performance,
                                directory_usage: self.directory_usage(),
                                hover_slot: &self.graph_hover,
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
                                stats_scroll: self.performance_stats_scroll.clone(),
                                hover_slot: &self.graph_hover,
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
                                    stats_scroll: self.performance_stats_scroll.clone(),
                                    budget: performance_layout,
                                },
                                cx,
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
                                stats_scroll: self.performance_stats_scroll.clone(),
                                hover_slot: &self.graph_hover,
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
                                stats_scroll: self.performance_stats_scroll.clone(),
                                hover_slot: &self.graph_hover,
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
                            .text_size(tokens::FONT_11)
                            .text_color(t.fg_dim)
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
                            taskmanager_application::i18n::t(
                                if self.history_replay_state().is_open() {
                                    "perf.replay.back_to_live"
                                } else {
                                    "perf.replay.toggle"
                                },
                            ),
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
                if performance_layout.device_navigation
                    == responsive::DeviceNavigationPresentation::Strip
                {
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
                    let sidebar_memory_usage =
                        std::rc::Rc::clone(self.memory_history.refresh(telemetry).0);
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
                            corner_factor: corner_radius_factor,
                        },
                        cx,
                    ));
                } else {
                    body = body.flex_row();
                }
                body.child(content)
            }
            TopPage::Apps => {
                let page_metrics = vm::process_page_metrics(snap);
                let process_count = self.processes().len();
                let hidden_cols = processes_view::effective_process_hidden_cols(
                    &self.processes_state.hidden_cols,
                    page_metrics.swap_total_bytes,
                );
                let (sort_column, sort_direction) = self.process_sort();
                let sort_col =
                    processes_view::effective_process_sort_col(sort_column, &hidden_cols);
                let sort_asc = matches!(sort_direction, taskmanager_shell::SortDir::Asc);
                let (rows, _pids, query) = self.processes_projection();
                let selected_target_count = self.selected_process_pids().len();
                let application_count = self.process_application_count();
                // Own TextInput backed by this window's persistent per-window
                // state (lazily created on the first Apps render). The field
                // owns its focus, caret blink, and key handling; an
                // InputEvent::Change subscription (see init_search_entity)
                // mirrors its value into the shell-owned process query so the
                // shared match grammar keeps filtering.
                let search_input = self
                    .search_input
                    .get_or_insert_with(|| init_search_entity(cx))
                    .clone();
                PageScaffold::new(
                    processes_view::render_processes(
                        processes_view::ProcessesViewProps {
                            theme: t,
                            application_count,
                            process_count,
                            search_input: &search_input,
                            rows: &rows,
                            query: &query,
                            selected: sel_pid,
                            selected_row: self.selected_process_row(),
                            selected_target_count: self.selected_process_count(),
                            selected_identities: self.selected_process_identities(),
                            hovered: hovered.cloned(),
                            sort_col,
                            sort_asc,
                            filter: self.process_status_filter(),
                            affinity_pid: self.process_affinity_pid(),
                            affinity_state: self.shell.process_affinity_state(),
                            affinity_cpus: &self.processes_state.affinity_editor.cpus,
                            affinity_hover: self.processes_state.affinity_editor.hover,
                            hidden_cols: &hidden_cols,
                            swap_auto_hidden: page_metrics.swap_auto_hidden,
                            batch_history_available: !self.process_batch_history.is_empty(),
                            col_widths: &self.processes_state.col_widths,
                            viewport_width: content_width,
                            processes_scroll: &self.processes_scroll.vertical,
                            horizontal_scroll: &self.processes_scroll.horizontal,
                            column_cursor: self.processes_state.column_cursor,
                            gray_zero_values: presentation.gray_zero_values,
                            density: appearance.density,
                            ui_size: appearance.ui_size,
                            presentation:
                                processes_view::ProcessChromePresentation::from_page_layout(layout),
                        },
                        window,
                        cx,
                    ),
                    px(page_padding),
                )
                .footer(elements::status_bar(
                    t,
                    &[
                        format!("{}: {}", i18n::t("proc.total"), process_count),
                        format!(
                            "{}: {}",
                            i18n::t("proc.running"),
                            self.running_process_count()
                        ),
                    ],
                    &[
                        format!("{}: {}", i18n::t("common.cpu"), page_metrics.cpu_usage),
                        format!(
                            "{}: {}",
                            i18n::t("common.memory"),
                            page_metrics.memory_usage
                        ),
                    ],
                ))
                .render()
            }
            TopPage::Services => self.render_services_page(
                window,
                cx,
                t,
                hovered,
                source_retry_button.clone(),
                page_padding,
            ),
            TopPage::System => {
                let entity = cx.entity();
                let hardware = self.hardware_rc().clone();
                let processes = self.processes_arc().clone();
                let system_layout = responsive::SystemPageBudget::from_frame(frame);
                let content: AnyElement = match self.dashboard.section {
                    SystemSection::Dashboard => {
                        dashboard::render_dashboard(dashboard::DashboardViewProps {
                            theme: t,
                            scroll: &self.dashboard_scroll,
                            snapshot: snap,
                            history: &self.telemetry.system_history,
                            process_count: processes.len(),
                            active_alert_count: self.active_alerts().len(),
                            state: &self.dashboard,
                            layout: system_layout,
                            entity: entity.clone(),
                            hover_slot: self.graph_hover.clone(),
                        })
                        .into_any_element()
                    }
                    SystemSection::Hardware => system_view::render_system(
                        t,
                        system_view::SystemViewData {
                            hardware: &hardware,
                            snapshot: snap,
                            npu_inventory: self.npu_inventory(),
                            processes,
                            scroll: &self.system_scroll,
                        },
                        entity.clone(),
                    )
                    .into_any_element(),
                    SystemSection::Health => {
                        let health_entity = entity.clone();
                        let callbacks = SystemHealthCallbacks::new(move |request, _window, cx| {
                            health_entity.update(cx, |view, cx| {
                                view.request_system_health_self_test_confirmation(request);
                                cx.notify();
                            });
                        });
                        let selected_disk = match self.selected {
                            SelectedDevice::Disk(index) => snap.disks.get(index),
                            _ => snap.disks.first(),
                        };
                        let smart_report = selected_disk.and_then(|disk| {
                            self.capture_evidence
                                .system_health_report_for(&disk.device_id, disk.device_generation)
                                .or_else(|| {
                                    let (projection, _) = self.projection().smart_projection();
                                    super::super::system_health::smart_report_for_device(
                                        projection,
                                        &disk.device_id,
                                        disk.device_generation,
                                    )
                                })
                        });
                        system_health_view::render_system_health(
                            system_health_view::SystemHealthViewProps {
                                theme: t,
                                scroll: &self.system_health_scroll,
                                filesystems: self.storage_health(),
                                sensors: self.sensors(),
                                selected_disk,
                                smart_report,
                                layout: system_layout,
                                copy: &system_health_view::localized_text,
                                callbacks: &callbacks,
                            },
                        )
                        .into_any_element()
                    }
                };
                PageScaffold::new(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.0))
                        .gap(tokens::SPACE_6)
                        .child(dashboard::render_system_header(
                            t,
                            &self.dashboard,
                            self.projection().alert_center.event_history(),
                            system_layout,
                            entity,
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h(px(0.0))
                                .child(content),
                        ),
                    px(page_padding),
                )
                .render()
            }
            TopPage::Startup => self.render_startup_page(
                window,
                cx,
                t,
                hovered,
                source_retry_button.clone(),
                layout,
            ),
            TopPage::Users => {
                self.render_users_page(window, cx, t, hovered, source_retry_button, page_padding)
            }
            TopPage::AppHistory => {
                let history = self
                    .history_runtime
                    .replay()
                    .application_history_projection(
                        self.history_runtime.application_history_capability(),
                    );
                let history_rows = self.app_history_rows(&history);
                let history_count = history.rows.len();
                PageScaffold::new(
                    div().flex().flex_col().flex_1().min_h(px(0.0)).child(
                        app_history_view::render_app_history(
                            app_history_view::AppHistoryViewProps {
                                theme: t,
                                projection: history,
                                rows: history_rows,
                                scroll: &self.app_history_scroll,
                                entity: cx.entity(),
                                ui_size: appearance.ui_size,
                                columns: app_history_view::AppHistoryColumns::from_page_layout(
                                    layout,
                                ),
                            },
                        ),
                    ),
                    px(page_padding),
                )
                .footer(elements::status_bar(
                    t,
                    &[format!(
                        "{}: {}",
                        i18n::t("history.application.title"),
                        history_count
                    )],
                    &[],
                ))
                .render()
            }
            TopPage::Containers => PageScaffold::new(
                containers_view::render_containers(t, self.containers()),
                px(page_padding),
            )
            .render(),
        }
    }
}
