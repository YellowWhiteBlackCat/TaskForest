//! GPUI adapter for background taskforest-daemon streaming integration.

use crossbeam_channel::Receiver;
use taskmanager_app_host::DaemonSubscriptionWorker;
use taskmanager_ipc::DaemonMessage;

use super::RootView;

impl RootView {
    /// Ingest ready incoming daemon messages and apply to the GPUI view state.
    pub fn drain_daemon_telemetry(&mut self) -> bool {
        let mut had_messages = false;
        if let Some(rx) = &self.daemon_rx {
            let mut messages = Vec::new();
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
            }
            if !messages.is_empty() {
                had_messages = true;
                for msg in messages {
                    if let DaemonMessage::TelemetryBatch {
                        timestamp_ms,
                        payload_json,
                        ..
                    } = msg
                    {
                        self.last_daemon_telemetry_ms = timestamp_ms;
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&payload_json) {
                            let cpu = val
                                .get("cpu_usage_pct")
                                .and_then(|v| v.as_f64())
                                .map(|v| v as f32);
                            let mem_used = val.get("memory_used_bytes").and_then(|v| v.as_u64());
                            let mem_total = val.get("memory_total_bytes").and_then(|v| v.as_u64());
                            self.shell.apply_daemon_telemetry_snapshot(
                                timestamp_ms,
                                cpu,
                                mem_used,
                                mem_total,
                            );
                            if let Some(snap) = self.shell.projection().snapshot.clone() {
                                self.materialized.replace_snapshot(
                                    self.shell.projection().system_revision,
                                    snap,
                                );
                            }
                        }
                    }
                }
            }
        }
        had_messages
    }

    /// Install the active background daemon subscription worker and streaming channel.
    pub fn install_daemon_subscriber(
        &mut self,
        worker: DaemonSubscriptionWorker,
        rx: Receiver<DaemonMessage>,
    ) {
        self.is_daemon_connected = true;
        self.daemon_worker = Some(worker);
        self.daemon_rx = Some(rx);
    }
}
