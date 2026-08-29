use super::gpu_stats;
use crate::gpui_app::formatting::DisplayUnits;
use taskmanager_application::i18n;
use taskmanager_core::GpuGraphicsApi;
use taskmanager_core::core::metrics::{GpuMetrics, GpuScalarObservations, ScalarObservation};

/// A default GPU (no sysfs facts at all) keeps only the status row and
/// the headline utilization row — the utilization stays an honest `None`
/// (panel dash), and every optional fact (clocks, power, temperature,
/// VRAM, idle residency) omits its row instead of parking a dash.
#[test]
fn default_gpu_renders_status_and_utilization_only() {
    let rows = gpu_stats(&GpuMetrics::default(), DisplayUnits::default());
    assert_eq!(
        rows.len(),
        2,
        "only status + utilization rows, got {rows:?}"
    );
    assert_eq!(rows[0].label(), i18n::t("device.status"));
    assert_eq!(rows[1].label(), i18n::t("common.utilization"));
    assert_eq!(rows[1].value(), None, "first sample is a gap, not 0%");
}

/// A measured zero utilization and a real temperature both render —
/// measured zeros stay visible, only absence hides.
#[test]
fn measured_zero_utilization_renders_and_temperature_row_appears_with_data() {
    let gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(0.0, 1),
        temperature_c: ScalarObservation::available(42.0, 1),
        ..Default::default()
    });
    let rows = gpu_stats(&gpu, DisplayUnits::default());
    let utilization = rows
        .iter()
        .find(|row| row.label() == i18n::t("common.utilization"))
        .expect("utilization row must exist");
    assert_eq!(utilization.value(), Some("0%"));
    let temperature = rows
        .iter()
        .find(|row| row.label() == i18n::t("common.temperature"))
        .expect("temperature row must exist with a reading");
    assert_eq!(temperature.value(), Some("42 °C"));
}

#[test]
fn marketing_name_is_projected_as_a_real_gpu_identity_fact() {
    let mut gpu = GpuMetrics::new("", "Intel Xe Graphics");
    gpu.marketing_name = Some("Arc B390".into());
    let rows = gpu_stats(&gpu, DisplayUnits::default());
    let row = rows
        .iter()
        .find(|row| row.label() == i18n::t("gpu.marketing_name"))
        .expect("marketing name row");
    assert_eq!(row.value(), Some("Arc B390"));
}

#[test]
fn available_gpu_identity_and_clock_facts_render_as_detail_rows() {
    let mut gpu = GpuMetrics::new("gpu:pci:0000:01:00.0", "Intel Arc Graphics");
    gpu.driver = Some("xe".into());
    gpu.pci_slot = Some("0000:01:00.0".into());
    gpu.graphics_api = Some(GpuGraphicsApi {
        opengl_version: Some("4.6".into()),
        vulkan_version: Some("1.4.354".into()),
    });
    gpu.apply_scalar_observations(GpuScalarObservations {
        frequency_mhz: ScalarObservation::available(2_080, 1),
        max_frequency_mhz: ScalarObservation::available(2_500, 1),
        idle_residency_pct: ScalarObservation::available(62.0, 1),
        ..Default::default()
    });

    let rows = gpu_stats(&gpu, DisplayUnits::default());
    let row = |label| {
        rows.iter()
            .find(|row| row.label() == i18n::t(label))
            .unwrap_or_else(|| panic!("missing GPU detail row {label:?}: {rows:?}"))
    };

    assert_eq!(row("common.clock").value(), Some("2080 MHz"));
    assert_eq!(row("gpu.max_clock").value(), Some("2500 MHz"));
    assert_eq!(row("gpu.idle_residency").value(), Some("62%"));
    assert_eq!(row("common.driver").value(), Some("xe"));
    assert_eq!(row("gpu.pci_slot").value(), Some("0000:01:00.0"));
    assert_eq!(row("gpu.opengl_version").value(), Some("4.6"));
    assert_eq!(row("gpu.vulkan_version").value(), Some("1.4.354"));
}
