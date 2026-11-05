//! Strict context-menu capture state machine.
//!
//! A real context menu cannot be opened by mutating view state: its only
//! production entry is the row's right-click listener. This state machine
//! records the injected pointer, the host's open/close transition, and the
//! two-frame presentation gate that certifies the painted frame.

use super::super::CaptureEvidence;
#[cfg(any(test, feature = "test-support"))]
use super::super::CaptureMode;
use super::super::state::{ContextMenuCaptureProgress, ContextMenuPresence};
use super::CaptureScenario;
use gpui::{Pixels, Point};

impl CaptureEvidence {
    /// Strict context-menu evidence starts from the ordinary live process
    /// projection. It does not synthesize a second menu widget: the row's
    /// production right-click listener opens the same `PopupMenuState` a user
    /// would get.
    pub fn process_context_menu_requested(&self) -> bool {
        self.is_enabled()
            && self.scenario == Some(CaptureScenario::ProcessContextMenu)
            && self.telemetry_ready()
            && self.ui_data_ready()
            && self.context_menu_capture == ContextMenuCaptureProgress::AwaitingRow
    }

    pub(crate) fn begin_process_context_menu_capture(&mut self) -> bool {
        if !self.process_context_menu_requested() {
            return false;
        }
        self.context_menu_capture = ContextMenuCaptureProgress::AwaitingPointer;
        true
    }

    pub(crate) fn process_context_menu_pointer_requested(&self) -> bool {
        self.process_context_menu_active()
            && self.context_menu_capture == ContextMenuCaptureProgress::AwaitingPointer
    }

    pub(crate) fn process_context_menu_awaiting_menu(&self) -> bool {
        self.process_context_menu_active()
            && self.context_menu_capture == ContextMenuCaptureProgress::AwaitingMenu
    }

    pub(crate) fn process_context_menu_awaiting_presentation(&self) -> bool {
        self.process_context_menu_active()
            && self.context_menu_capture == ContextMenuCaptureProgress::AwaitingPresentation
    }

    pub(crate) fn process_context_menu_is_open(&self) -> bool {
        self.context_menu_presence.is_open()
    }

    pub fn process_context_menu_active(&self) -> bool {
        self.is_enabled() && self.scenario == Some(CaptureScenario::ProcessContextMenu)
    }

    pub(crate) fn arm_context_menu_pointer(&mut self, anchor: Point<Pixels>) -> bool {
        if !self.process_context_menu_active()
            || self.context_menu_capture != ContextMenuCaptureProgress::AwaitingPointer
        {
            return false;
        }
        self.context_menu_anchor = Some(anchor);
        self.context_menu_capture = ContextMenuCaptureProgress::AwaitingMenu;
        true
    }

    pub(crate) fn process_context_menu_pointer_anchor(&self) -> Option<Point<Pixels>> {
        self.context_menu_anchor
    }

    pub(crate) fn note_context_menu_open(&mut self) -> bool {
        if !self.process_context_menu_active()
            || self.context_menu_capture != ContextMenuCaptureProgress::AwaitingMenu
        {
            return false;
        }
        self.context_menu_capture = ContextMenuCaptureProgress::AwaitingPresentation;
        self.context_menu_presence = ContextMenuPresence::Open;
        true
    }

    pub(crate) fn note_context_menu_closed(&mut self) {
        self.context_menu_presence = ContextMenuPresence::Closed;
        if self.process_context_menu_active()
            && matches!(
                self.context_menu_capture,
                ContextMenuCaptureProgress::AwaitingPointer
                    | ContextMenuCaptureProgress::AwaitingMenu
                    | ContextMenuCaptureProgress::AwaitingPresentation
            )
        {
            self.context_menu_capture = ContextMenuCaptureProgress::AwaitingPointer;
            self.context_menu_presentation_armed = false;
            self.context_menu_anchor = None;
        }
    }

    pub(crate) fn note_context_menu_presentation_armed(&mut self) -> bool {
        if !self.process_context_menu_active()
            || self.context_menu_capture != ContextMenuCaptureProgress::AwaitingPresentation
            || self.context_menu_presentation_armed
        {
            return false;
        }
        self.context_menu_presentation_armed = true;
        true
    }

    pub(crate) fn mark_context_menu_presented(&mut self, menu_open: bool) {
        if self.process_context_menu_active()
            && self.context_menu_capture == ContextMenuCaptureProgress::AwaitingPresentation
            && self.context_menu_presentation_armed
            && menu_open
        {
            self.context_menu_capture = ContextMenuCaptureProgress::Presented;
            self.mark_scenario_ready();
            super::super::marker::emit_marker("surface_presented", self.scenario);
        }
    }
}

impl crate::gpui_app::root::RootView {
    #[cfg(any(test, feature = "test-support"))]
    pub fn install_process_context_menu_capture_for_test(&mut self) {
        self.capture_evidence = CaptureEvidence::from_environment();
        self.capture_evidence.mode = CaptureMode::Enabled;
        self.capture_evidence.scenario = Some(CaptureScenario::ProcessContextMenu);
        self.capture_evidence.context_menu_capture = ContextMenuCaptureProgress::AwaitingRow;
        self.capture_evidence.context_menu_presence = ContextMenuPresence::Closed;
        self.capture_evidence.context_menu_presentation_armed = false;
        self.capture_evidence.context_menu_anchor = None;
        self.capture_evidence.data_readiness =
            super::super::state::CaptureDataReadiness::UiDataReady;
    }

    pub(crate) fn note_context_menu_open_for_capture(&mut self) -> bool {
        self.capture_evidence.note_context_menu_open()
    }

    pub(crate) fn note_context_menu_closed_for_capture(&mut self) {
        self.capture_evidence.note_context_menu_closed();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn process_context_menu_capture_open(&self) -> bool {
        self.capture_evidence.process_context_menu_is_open()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn process_context_menu_capture_ready(&self) -> bool {
        self.capture_evidence.scenario_ready()
    }

    /// The pointer position the coordinator actually dispatched. Tests compare
    /// it against the target row's painted bounds so a wrong-row anchor cannot
    /// pass merely because the popup opened somewhere else on screen.
    #[cfg(any(test, feature = "test-support"))]
    pub fn process_context_menu_capture_anchor(&self) -> Option<Point<Pixels>> {
        self.capture_evidence.process_context_menu_pointer_anchor()
    }
}
