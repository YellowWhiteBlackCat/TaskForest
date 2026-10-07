//! GPU chart-metric selection wiring for the Performance page (ADR-034).
//!
//! The selection authority stays in the shell; this module only folds the
//! viewed device's gate into it and forwards selector-button activations.

use bevy::app::App;
use bevy::ecs::observer::On;
use bevy::ecs::system::{NonSendMut, Query, Res};
use bevy::ui_widgets::Activate;
use taskmanager_shell::ShellApp;
use taskmanager_shell::gpu_chart_metric_gate;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;

use super::metrics::gpu_devices;
use super::{GpuMetricButton, PerformanceDeviceFocus, PerformanceDeviceTarget};

pub(super) fn register(app: &mut App) {
    app.add_observer(reconcile_gpu_metric);
    app.add_observer(gpu_metric_button_activated);
}

/// The GPU a window is currently viewing, for the shared chart-metric gate:
/// the focused device when it is a GPU, else the first projected adapter.
fn viewed_gpu_id(shell: &ShellApp, focus: &PerformanceDeviceTarget) -> Option<String> {
    if let PerformanceDeviceTarget::Gpu(id) = focus {
        return Some(id.clone());
    }
    gpu_devices(shell).and_then(|devices| devices.first().map(|gpu| gpu.device_id.clone()))
}

/// Fold the viewed GPU's gate into the shared chart-metric selection whenever
/// the projection folds: a device-generation advance resets the selection to
/// the ADR default. The selection authority stays in the shell; the frontend
/// holds no second copy.
pub(super) fn reconcile_gpu_metric(
    _fold: On<ShellProjectionFolded>,
    focus: Res<PerformanceDeviceFocus>,
    mut track: NonSendMut<FrontendTrack>,
) {
    let shell = &mut track.shell;
    let gate = {
        let gpu = viewed_gpu_id(shell, &focus.0).and_then(|id| {
            gpu_devices(shell).and_then(|devices| devices.iter().find(|gpu| gpu.device_id == id))
        });
        gpu_chart_metric_gate(gpu)
    };
    shell.reconcile_gpu_chart_metric(&gate);
}

/// Select one GPU chart-metric family through the shared gate on button
/// activation. Unavailable families are rejected by the shell with no state
/// change.
pub(crate) fn gpu_metric_button_activated(
    activate: On<Activate>,
    buttons: Query<&GpuMetricButton>,
    focus: Res<PerformanceDeviceFocus>,
    mut track: NonSendMut<FrontendTrack>,
) {
    let Ok(button) = buttons.get(activate.entity) else {
        return;
    };
    let shell = &mut track.shell;
    let gate = {
        let gpu = viewed_gpu_id(shell, &focus.0).and_then(|id| {
            gpu_devices(shell).and_then(|devices| devices.iter().find(|gpu| gpu.device_id == id))
        });
        gpu_chart_metric_gate(gpu)
    };
    shell.select_gpu_chart_metric(button.0, &gate);
}
