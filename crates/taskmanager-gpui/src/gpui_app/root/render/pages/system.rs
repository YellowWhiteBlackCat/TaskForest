//! System page body: section header and the selected section.
//!
//! Renders the dashboard/hardware/health section header plus the currently
//! selected `SystemSection` body, from the frame-local render context.

use gpui::{AnyElement, Context, Div, IntoElement, ParentElement, Styled, div, px};
use taskmanager_theme::tokens;
use taskmanager_ui::layout::PageScaffold;

use super::{PageRenderContext, RootView, SelectedDevice};
use crate::gpui_app::dashboard::SystemSection;
use crate::gpui_app::root::{
    SystemHealthCallbacks, dashboard, responsive, system_health_view, system_view,
};

impl RootView {
    /// Render the System page: the dashboard/hardware/health section header and
    /// its selected section body.
    pub(super) fn render_system_page(
        &mut self,
        cx: &mut Context<Self>,
        context: PageRenderContext<'_>,
    ) -> Div {
        let PageRenderContext {
            theme: t,
            snapshot: snap,
            frame,
            page_padding,
            ..
        } = context;
        let graph_cache = std::rc::Rc::clone(&self.graph_cache);
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
                    graph_cache: graph_cache.clone(),
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
                    units: self.display_units(),
                    memory_inventory: system_view::MemoryInventoryInputs {
                        state: self.shell.smbios_memory_state(),
                        capability: self.projection().capability_status(
                            &taskmanager_platform_contract::CapabilityId::TELEMETRY_MEMORY_SMBIOS,
                        ),
                    },
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
                            crate::gpui_app::root::system_health::smart_report_for_device(
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
                        units: self.display_units(),
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
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
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
}
