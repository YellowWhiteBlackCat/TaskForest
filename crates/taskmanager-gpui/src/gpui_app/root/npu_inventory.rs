//! NPU accelerator inventory apply path on `RootView` (the typed
//! `accelerator.npu` lane).
//!
//! Discovery-first: each answer is a sorted device list or a typed failure.
//! The System page renders the section only when real devices exist — an
//! empty list (honest no-NPU host) and typed failures (registered-pending
//! adapters) leave the page unchanged, never a placeholder or fabricated
//! card. Requests are paced by the hardware-inventory refresh chain: every
//! hardware snapshot arrives on the same slow static-facts cadence, so the
//! accelerator enumeration rides it without a new timer.

use taskmanager_application::NpuInventoryRequest;

use crate::gpui_app::root::RootView;

impl RootView {
    /// Submit one accelerator inventory read. A typed submission failure
    /// (absent lane on this runtime) is stored as the honest failed snapshot
    /// instead of a hang; no platform is a no-op.
    pub(crate) fn submit_npu_inventory_refresh(&mut self) {
        let Some(platform) = self.platform.as_mut() else {
            return;
        };
        let _ = platform
            .submit_npu_inventory(NpuInventoryRequest {}, super::platform_submission_time_ms());
    }
}
