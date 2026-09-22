//! Window lifecycle, capture-frame and virtual-scroll viewport messages.

use iced::Task;
use taskmanager_application::AppPage;
use taskmanager_shell::QuitReason;

use super::super::{IcedApp, LocalSurfaceKind, Message};
use crate::app::viewport_state::ViewportRegion;

pub(super) fn close_latest_window() -> Task<Message> {
    iced::window::latest().then(|id| match id {
        Some(id) => iced::window::close(id),
        None => Task::none(),
    })
}

pub(super) fn restore_latest_window() -> Task<Message> {
    iced::window::latest().then(|id| match id {
        Some(id) => iced::window::minimize(id, false)
            .chain(iced::window::gain_focus(id))
            .chain(iced::window::request_user_attention(
                id,
                Some(iced::window::UserAttention::Informational),
            )),
        None => Task::none(),
    })
}

fn minimize_latest_window() -> Task<Message> {
    iced::window::latest().then(|id| match id {
        Some(id) => iced::window::minimize(id, true),
        None => Task::none(),
    })
}

impl IcedApp {
    /// Apply a window-local message. Any returned task joins the common finish
    /// envelope; a real quit is recorded in the shell and projected to the
    /// single close task by that finish system.
    pub(super) fn handle_window_message(&mut self, message: Message) -> Option<Task<Message>> {
        match message {
            Message::Frame(now) => {
                if !self.capture.emitted {
                    self.capture.emitted = true;
                    if let Some(path) = self.capture.marker.as_deref() {
                        crate::capture::append_marker(
                            path,
                            "frame_ready",
                            if self.is_demo() { "demo" } else { "live" },
                            crate::capture::page_name(self.shell.page()),
                        );
                        if self.is_demo() {
                            let page = crate::capture::page_name(self.shell.page());
                            if self.local_surface_kind() == Some(LocalSurfaceKind::Health) {
                                // The health modal rides the Performance page:
                                // its target token names the local surface, the
                                // same shape `service-details` uses on Services.
                                crate::capture::append_target_marker(
                                    path,
                                    page,
                                    crate::capture::HEALTH_TARGET,
                                );
                            } else if self.shell.page() == AppPage::Performance {
                                crate::capture::append_device_marker(
                                    path,
                                    self.performance.selected_device,
                                );
                            } else {
                                let target = std::env::var("TM_ICED_CAPTURE_DEVICE")
                                    .ok()
                                    .filter(|target| target == "service-details")
                                    .unwrap_or_else(|| page.to_owned());
                                crate::capture::append_target_marker(path, page, &target);
                            }
                        }
                    }
                }
                // A health-modal evidence frame bounds the modal body to its
                // end: the body is a fixed 430px scrollable and the
                // thermal-zone panel sits below the summary, so the unbounded
                // first frame would cut its rows at the viewport edge. Iced
                // clamps the offset to the real content, and the marker gate
                // keeps every production modal on the user's own scroll.
                let health_capture = self.health_capture_frame();
                // The same per-frame pump also drives frontend-local motion:
                // the eased modal entrance and the warm-up spinner advance on
                // the frame timestamp (the tick stays as a coarse fallback).
                self.advance_motion(now);
                health_capture.then(crate::ui::health::bound_body_to_end)
            }
            Message::ApplicationsScrolled(viewport) => {
                self.viewport.update(ViewportRegion::Applications, viewport);
                None
            }
            Message::AppHistoryScrolled(viewport) => {
                self.viewport.update(ViewportRegion::AppHistory, viewport);
                None
            }
            Message::PerformanceRailScrolled(viewport) => {
                self.viewport
                    .update(ViewportRegion::PerformanceRail, viewport);
                None
            }
            Message::ServicesScrolled(viewport) => {
                self.viewport.update(ViewportRegion::Services, viewport);
                None
            }
            Message::StartupScrolled(viewport) => {
                self.viewport.update(ViewportRegion::Startup, viewport);
                None
            }
            Message::UsersScrolled(viewport) => {
                self.viewport.update(ViewportRegion::Users, viewport);
                None
            }
            Message::WindowResized(size) => {
                let _ = self.viewport.resize(size);
                None
            }
            Message::WindowCloseRequested => {
                // Quit is an unconditional flush point for a deferred
                // stepper commit: the final width must not die with the
                // window (the poll flush would only land after the
                // coalescing window).
                self.process_column_sizing.note_direct_persist();
                self.persist_process_column_widths();
                if self.tray_available() {
                    Some(minimize_latest_window())
                } else {
                    self.shell.request_quit(QuitReason::WindowClose);
                    None
                }
            }
            _ => None,
        }
    }

    /// True while a capture lane owns a health-modal frame: the evidence
    /// runner set a marker path, this is the demo shape it launches, and the
    /// health surface is the active one. The same gate as the health target
    /// marker; a production launch never carries the marker, so the modal body
    /// is only bounded for evidence (see `ui::health::bound_body_to_end`).
    fn health_capture_frame(&self) -> bool {
        self.capture.marker.is_some()
            && self.is_demo()
            && self.local_surface_kind() == Some(LocalSurfaceKind::Health)
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/app/window_events_tests.rs"]
mod tests;
