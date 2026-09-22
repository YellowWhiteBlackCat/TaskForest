//! GPU-page keyboard routing and generation-bound chart selection tests.

use super::super::*;

/// The GPU-page `g` chord cycles the shared shell chart-metric selection
/// (ADR-034 stage 2): availability-gated through the demo GPU's typed
/// facts (power is unobserved, so the cycle skips it), reported in the
/// status bar, and a no-op off the GPU device.
#[test]
fn gpu_page_g_cycles_the_shared_chart_metric_selection() {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;

    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(app.perf_device, crate::PerfDevice::Gpu);
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "the default selection is Utilization"
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('g'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature,
        "the cycle skips the unobserved power family"
    );
    let notice = app
        .shell
        .feedback_notice()
        .map(|notice| notice.text().to_owned())
        .unwrap_or_default();
    assert!(
        notice.contains(taskmanager_application::i18n::t("gpu.graph_temperature")),
        "the cycle must report the family it landed on: {notice}"
    );

    // Off the GPU device the same chord changes nothing.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('1'),
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('g'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature,
        "the chord is scoped to the GPU device"
    );
}

/// The same-wave fold (ADR-034 stage 2): when the next applied batch carries
/// a viewed GPU whose device generation advanced (a confirmed hot-plug), the
/// TUI's per-batch fold resets the shared selection to the Utilization
/// default before the next paint. The store edit stands in for the provider
/// fact; the fold itself is the production `apply_platform_batch` path every
/// live batch drives.
#[test]
fn gpu_chart_metric_selection_resets_when_the_generation_advances() {
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;

    let mut app = crate::demo_app();
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('5'),
            KeyModifiers::NONE,
        ),
    );
    // Bind the selection to the demo GPU's generation first — the production
    // fold does this on the very first batch, long before a user can press g.
    app.apply_platform_batch(taskmanager_application::PlatformEventBatch::default());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('g'),
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature
    );

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        if let Some(snapshot) = snapshot.as_mut()
            && let Some(gpu) = snapshot.gpu.first_mut()
        {
            gpu.device_generation =
                DeviceGeneration::new(gpu.device_generation.get().saturating_add(1));
        }
    });
    app.apply_platform_batch(taskmanager_application::PlatformEventBatch::default());

    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "a generation change must reset the selection to the ADR default"
    );
    let notice = app
        .shell
        .feedback_notice()
        .map(|notice| notice.text().to_owned())
        .unwrap_or_default();
    assert!(
        notice.contains(taskmanager_application::i18n::t("gpu.graph_utilization")),
        "the reset must land in the same wave the fact did: {notice}"
    );
}
