//! Window lifecycle, capture-frame and virtual-scroll viewport messages.

use iced::Task;
use taskmanager_application::AppPage;
use taskmanager_shell::QuitReason;

use super::super::{IcedApp, Message};
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
                            if self.shell.page() == AppPage::Performance {
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
                // The same per-frame pump also drives frontend-local motion:
                // the eased modal entrance and the warm-up spinner advance on
                // the frame timestamp (the tick stays as a coarse fallback).
                self.advance_motion(now);
                None
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
}
