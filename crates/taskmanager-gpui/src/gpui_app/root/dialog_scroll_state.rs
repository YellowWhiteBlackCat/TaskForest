//! Per-window scroll ownership for bounded modal bodies.

use gpui::ScrollHandle;

use super::RootView;

/// Every long-form root dialog owns a stable handle while reusing the shared
/// bounded viewport + pinned-rail component for geometry and interaction.
pub(crate) struct DialogScrollState {
    pub settings: ScrollHandle,
    pub help: ScrollHandle,
    pub about: ScrollHandle,
    pub first_run: ScrollHandle,
    pub system_about: ScrollHandle,
    pub dashboard_panel: ScrollHandle,
    pub process_details: ScrollHandle,
    pub service_details: ScrollHandle,
    pub diagnostic_preview: ScrollHandle,
    pub process_batch: ScrollHandle,
}

impl Default for DialogScrollState {
    fn default() -> Self {
        Self {
            settings: ScrollHandle::new(),
            help: ScrollHandle::new(),
            about: ScrollHandle::new(),
            first_run: ScrollHandle::new(),
            system_about: ScrollHandle::new(),
            dashboard_panel: ScrollHandle::new(),
            process_details: ScrollHandle::new(),
            service_details: ScrollHandle::new(),
            diagnostic_preview: ScrollHandle::new(),
            process_batch: ScrollHandle::new(),
        }
    }
}

impl RootView {
    /// Stable per-window scroll owner for the Settings dialog body.
    ///
    /// Returning a clone preserves the shared GPUI handle identity while
    /// keeping the rest of the dialog-scroll aggregate encapsulated.
    #[must_use]
    pub fn settings_scroll_handle(&self) -> ScrollHandle {
        self.dialog_scroll.settings.clone()
    }
}
