//! Typed RootView mutation for Mission Center Performance graph preferences.

use super::RootView;
use crate::core::alerts::{NotificationPolicy, QuietBound, apply_quiet_hour_bound};

use crate::gpui_app::formatting::PerformanceSettings;
use crate::gpui_app::graph::GraphSettings;
use gpui::Context;

pub(crate) fn normalize_graph_data_points(value: u32) -> u32 {
    GraphSettings::from_config(value, false, true, 0).data_points_as_config()
}

impl RootView {
    /// Snapshot units and graph preferences once at render entry so all
    /// Performance consumers share the same projection for a frame.
    pub(crate) fn performance_settings(&self) -> PerformanceSettings {
        let graphs = self.presentation.graphs();
        PerformanceSettings {
            units: self.display_units(),
            graph: GraphSettings::from_config(
                graphs.data_points,
                graphs.sliding,
                graphs.network_dynamic_scaling,
                self.projection()
                    .system_telemetry
                    .as_ref()
                    .map_or(0, |telemetry| telemetry.revision.get()),
            ),
        }
    }

    pub(crate) fn set_graph_data_points(&mut self, value: u32, cx: &mut Context<Self>) {
        let mut graphs = self.presentation.graphs();
        graphs.data_points = normalize_graph_data_points(value);
        self.presentation.set_graphs(graphs);
        // The narrow suggestion window follows the same persisted preference;
        // live charts use the correlated telemetry store's bounded history.
        self.smart_history.set_capacity(graphs.data_points as usize);
        cx.notify();
    }

    pub(crate) fn set_sliding_graphs(&mut self, value: bool, cx: &mut Context<Self>) {
        let mut graphs = self.presentation.graphs();
        graphs.sliding = value;
        self.presentation.set_graphs(graphs);
        cx.notify();
    }

    pub(crate) fn set_network_dynamic_scaling(&mut self, value: bool, cx: &mut Context<Self>) {
        let mut graphs = self.presentation.graphs();
        graphs.network_dynamic_scaling = value;
        self.presentation.set_graphs(graphs);
        cx.notify();
    }

    /// Flip desktop-notification delivery for fired alerts (BN-07). The pure
    /// [`NotificationGate`] policy lives on RootView; persistence flows through
    /// the regular Config save path (`notify_enabled`).
    pub(crate) fn set_notify_enabled(&mut self, value: bool, cx: &mut Context<Self>) {
        let current = self.projection().alert_center.policy().clone();
        self.shell.set_alert_policy(NotificationPolicy {
            enabled: value,
            ..current
        });
        cx.notify();
    }

    /// Set one quiet-hours bound (start or end, hours 0..=23; BN-07). Equal
    /// bounds mean "no quiet hours" (the gate treats them as
    /// never-suppressing), so setting either bound to the other's value
    /// clears the window — the same semantics the TUI/Iced pickers use.
    pub(crate) fn set_quiet_hour_bound(
        &mut self,
        bound: QuietBound,
        hour: u8,
        cx: &mut Context<Self>,
    ) {
        let current = self.projection().alert_center.policy().quiet_hours;
        let policy = self.projection().alert_center.policy().clone();
        self.shell.set_alert_policy(NotificationPolicy {
            quiet_hours: apply_quiet_hour_bound(current, bound, hour),
            ..policy
        });
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_graph_options_tests.rs"]
mod tests;
