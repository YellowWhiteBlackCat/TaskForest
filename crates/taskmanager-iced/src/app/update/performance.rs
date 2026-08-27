//! Performance options, on-demand capability and replay message reducer.

use taskmanager_application::DeviceId;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, present_gpu_engine_rows,
};

use super::super::{IcedApp, Message, PerfDevice};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    pub(super) fn reduce_performance_message(&mut self, message: Message) -> UpdateDispatch {
        let effect = match message {
            Message::SelectPerformanceGraphPoints(points) => {
                let mut config = self.config_draft();
                config.graph_data_points = points;
                self.commit_config_draft(config);
                None
            }
            Message::SettingsChanged(change) => {
                self.apply_settings_change(change);
                None
            }
            Message::ToggleGpuEngines => {
                let device_id = self
                    .shell
                    .projection()
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| match self.performance.selected_device {
                        PerfDevice::Gpu(index) => snapshot.gpu.get(index),
                        _ => None,
                    })
                    .and_then(|gpu| {
                        let id = gpu.device_id.trim();
                        (!id.is_empty()).then(|| DeviceId::new(id.to_owned()))
                    });
                let action = device_id.as_ref().map_or(GpuEngineRowsAction::None, |id| {
                    present_gpu_engine_rows(
                        self.shell.gpu_engine_rows_state(),
                        id,
                        self.shell.projection().capability_status(
                            &taskmanager_application::CapabilityId::TELEMETRY_GPU_ENGINES,
                        ),
                    )
                    .action()
                });
                match action {
                    GpuEngineRowsAction::Disable => {
                        self.shell.close_gpu_engine_rows_request();
                        None
                    }
                    GpuEngineRowsAction::Enable
                    | GpuEngineRowsAction::Reauthorize
                    | GpuEngineRowsAction::Recheck => device_id.map(|id| {
                        self.runtime.reset_gpu_engine_rows_cadence();
                        ShellApp::request_gpu_engine_rows(id)
                    }),
                    GpuEngineRowsAction::None => None,
                }
            }
            Message::SelectGpuChartMetric(metric) => {
                // The shell owns the selection and its availability gate
                // (ADR-034): the message only names the family; the gate
                // derived from the viewed device accepts or rejects it.
                let gate = taskmanager_shell::gpu_chart_metric_gate(self.viewed_gpu());
                self.shell.select_gpu_chart_metric(metric, &gate);
                None
            }
            Message::ToggleDirectoryUsageScan => {
                let selected = self.perf_device();
                self.shell
                    .projection()
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| match selected {
                        PerfDevice::Disk(index) => snapshot.disks.get(index),
                        _ => snapshot.disks.first(),
                    })
                    .and_then(|disk| {
                        crate::ui::directory_usage::toggle_request(
                            disk,
                            self.shell.projection().directory_usage.as_ref(),
                        )
                    })
                    .map(ShellApp::request_directory_usage)
            }
            Message::ToggleHistoryReplay => {
                self.toggle_history_replay();
                None
            }
            Message::SelectHistoryReplayWindow(window) => {
                self.select_history_replay_window(window);
                None
            }
            Message::RefreshHistoryReplay => {
                self.refresh_history_replay();
                None
            }
            // Frontend-local dashboard window selection (no shell effect):
            // the pills only re-project the System-page dashboard segment.
            Message::SystemDashboard(
                crate::ui::system_dashboard::SystemDashboardMessage::SelectWindow(window),
            ) => {
                self.system_dashboard_window = window;
                None
            }
            _ => None,
        };
        UpdateDispatch::effect(effect)
    }
}
