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
                    if let DaemonMessage::TelemetryBatch { timestamp_ms, .. } = msg {
                        self.last_daemon_telemetry_ms = timestamp_ms;
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
