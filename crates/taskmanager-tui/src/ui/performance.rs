//! Performance detail composition over the cached device identity and preferences.

use super::*;

pub(super) fn render_performance(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    plan: &TuiFramePlan,
) {
    let TuiPageLayout::Performance { selector, content } = plan.page else {
        return;
    };
    if app.history_replay_open() {
        history_replay::render(frame, app, theme, content);
        return;
    }
    let Some(snapshot) = app.projection().snapshot.as_ref() else {
        render_loading(frame, theme, content, t("common.collecting_telemetry"));
        return;
    };
    // The compact resource selector row sits above the selected resource's
    // detail; the area below shows ONLY that resource, reusing the existing
    // per-resource renderers (gauges + history graph for Cpu/Memory, the
    // dedicated perf_gpu/perf_disks/perf_networks panels for the device views).
    render_perf_selector(frame, app, theme, selector);
    if let Some(key) = app.performance_device_key.as_ref() {
        let entries = app.sidebar_entries();
        match entries.iter().find(|entry| &entry.key == key) {
            None => {
                render_empty_panel(
                    frame,
                    theme,
                    content,
                    t("device.disconnected"),
                    t("device.reconnect_hint"),
                );
                return;
            }
            Some(entry) if !entry.visible => {
                render_empty_panel(
                    frame,
                    theme,
                    content,
                    t("sidebar.devices"),
                    t("sidebar.hide_device"),
                );
                return;
            }
            Some(_) => {}
        }
    }
    match app.perf_device {
        PerfDevice::Cpu | PerfDevice::Memory => {
            perf_overview::render_perf_overview(frame, app, theme, content, snapshot);
        }
        PerfDevice::Gpu => perf_gpu::render_gpu_section(frame, app, theme, content, &snapshot.gpu),
        PerfDevice::Disk => {
            // The directory-usage projection panel (render-only) rides under
            // the per-disk detail. Adaptive height: a projected snapshot needs
            // room for root + entries + totals + status; the common idle slot
            // (no scan projected yet) stays a slim 3-line panel so the disk
            // detail keeps nearly the whole content area. The data comes from
            // the SHARED `SystemProjectionStore::directory_usage` slot (latest-wins from
            // the platform batch fold).
            let usage_height: u16 =
                match (app.projection().directory_usage.is_some(), content.height) {
                    (true, height) if height >= 20 => 12,
                    (false, height) if height >= 11 => 3,
                    _ => 0,
                };
            let [disk_area, usage_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(usage_height)])
                    .areas(content);
            perf_disks::render_disk_section(frame, app, theme, disk_area, &snapshot.disks);
            perf_disks::render_directory_usage(frame, app, theme, usage_area);
        }
        PerfDevice::Network => {
            perf_networks::render_network_section(frame, app, theme, content, &snapshot.networks)
        }
        PerfDevice::Battery => perf_battery::render_battery_section(
            frame,
            app,
            theme,
            content,
            app.projection().power_supplies.as_ref(),
        ),
        PerfDevice::Fan => perf_fan::render_fan_section(
            frame,
            app,
            theme,
            content,
            app.projection().sensors.as_ref(),
        ),
        PerfDevice::Npu => perf_npu::render_npu_section(
            frame,
            app,
            theme,
            content,
            app.projection().npu_inventory.as_ref(),
        ),
    }
}
