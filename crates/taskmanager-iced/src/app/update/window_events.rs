//! Window lifecycle, capture-frame and virtual-scroll viewport messages.

use crate::app::capture_state::{CaptureDataTarget, CapturePresentationFrame};
use crate::ui::system_table::bound_system_body_to_end;
use iced::Task;
use taskmanager_application::SmbiosMemoryState;
use taskmanager_application::system_timeline::SystemPageSection;
use taskmanager_application::{AppPage, KeyCode, Modifiers};
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_shell::QuitReason;
use taskmanager_shell::ShellKeyEvent;
use taskmanager_shell::fixture::process_insights::process_properties_capture_data_ready;
use taskmanager_shell::presentation::effective_smart_status;
use taskmanager_ui_contract::navigation::NavOrientation;

use super::super::{IcedApp, IcedKey, LocalSurfaceKind, Message};
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
            Message::CaptureFocusPresented(visible) => {
                if self.capture.focus_scheduled {
                    self.capture.focus_presented = visible;
                }
                Some(Task::none())
            }
            Message::Frame(now) => {
                if let Some(target) = self.capture.focus_target
                    && !self.capture.focus_scheduled
                {
                    self.capture.focus_scheduled = true;
                    let id = crate::focus::focus_id(target);
                    let keyboard = self.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
                        KeyCode::Tab,
                        Modifiers::NONE,
                    ))));
                    return Some(
                        keyboard
                            .chain(iced::widget::operation::focus(id.clone()))
                            .chain(
                                crate::focus::reveal::reveal_focused(Some(id.into()))
                                    .map(Message::CaptureFocusPresented),
                            ),
                    );
                }
                if self.capture.data_target == CaptureDataTarget::MemoryInventory
                    && matches!(
                        self.shell.smbios_memory_state(),
                        SmbiosMemoryState::Ready(_)
                    )
                    && !self.capture.scroll_requested
                {
                    self.capture.scroll_requested = true;
                    return Some(bound_system_body_to_end());
                }
                let history_presented = if self.capture.data_target != CaptureDataTarget::General {
                    let ready = if self.capture.data_target == CaptureDataTarget::Focus {
                        self.capture.focus_presented
                    } else if self.capture.data_target == CaptureDataTarget::VerticalNavigation {
                        self.nav_orientation == NavOrientation::Vertical
                            && self.shell.page() == AppPage::Applications
                    } else if self.capture.data_target == CaptureDataTarget::SavedViews {
                        self.local_surface_kind() == Some(LocalSurfaceKind::SavedViews)
                            && self.saved_views.iter().any(|preset| preset.is_user_saved())
                    } else if self.capture.data_target == CaptureDataTarget::SidebarEditor {
                        self.local_surface_kind() == Some(LocalSurfaceKind::SidebarEditor)
                            && self
                                .sidebar_entries()
                                .iter()
                                .any(|entry| entry.key == "memory")
                    } else if self.capture.data_target == CaptureDataTarget::Health {
                        self.system_section == SystemPageSection::Health
                            && self.shell.page() == AppPage::System
                            && self
                                .shell
                                .projection()
                                .storage_health_projection()
                                .is_some_and(|(snapshot, _)| snapshot.filesystems.len() == 3)
                            && self
                                .shell
                                .projection()
                                .sensors
                                .as_ref()
                                .is_some_and(|snapshot| snapshot.readings.len() == 4)
                    } else if self.capture.data_target == CaptureDataTarget::StartupFailure {
                        self.shell.page() == AppPage::Startup
                            && self
                                .shell
                                .projection()
                                .startup_boot_evidence
                                .as_ref()
                                .is_some_and(|evidence| {
                                    evidence.failed_units.len() >= 3
                                        && evidence.critical_chain.len() >= 2
                                })
                    } else if self.capture.data_target == CaptureDataTarget::ActiveAlerts {
                        self.alerts_page_open() && !self.shell.projection().alert_active.is_empty()
                    } else if self.capture.data_target == CaptureDataTarget::AlertRules {
                        self.alerts_page_open() && !self.alerts_rules().is_empty()
                    } else if self.capture.data_target == CaptureDataTarget::Battery {
                        self.shell
                            .projection()
                            .power_supplies
                            .as_ref()
                            .is_some_and(|power| {
                                power.batteries.first().is_some_and(|battery| {
                                    self.cached_battery_series(&battery.id).len() >= 2
                                        && self.cached_battery_power_series(&battery.id).len() >= 2
                                })
                            })
                    } else if let CaptureDataTarget::Smart(scenario) = self.capture.data_target {
                        self.local_surface_kind() == Some(LocalSurfaceKind::DiskSmart)
                            && self
                                .shell
                                .projection()
                                .snapshot
                                .as_ref()
                                .is_some_and(|snapshot| {
                                    snapshot.disks.first().is_some_and(|disk| {
                                        effective_smart_status(disk)
                                            == if scenario == "smart-missing-tool" {
                                                DeviceStatus::MissingTool
                                            } else {
                                                DeviceStatus::PermissionDenied
                                            }
                                    })
                                })
                    } else if self.capture.data_target == CaptureDataTarget::SystemDashboard {
                        self.shell.page() == AppPage::System
                            && self
                                .shell
                                .system_timeline_series(self.system_dashboard_window)
                                .covered_ms
                                >= self.system_dashboard_window.minutes() * 60_000
                    } else if self.capture.data_target == CaptureDataTarget::MemoryInventory {
                        self.shell.page() == AppPage::System
                            && matches!(
                                self.shell.smbios_memory_state(),
                                SmbiosMemoryState::Ready(_)
                            )
                    } else if let CaptureDataTarget::ProcessProperties(scenario) =
                        self.capture.data_target
                    {
                        process_properties_capture_data_ready(&self.shell, scenario)
                            && (scenario != "process-properties-performance"
                                || self.process_perf_history().is_some_and(|history| {
                                    history
                                        .cpu_samples()
                                        .iter()
                                        .filter(|value| value.is_finite())
                                        .count()
                                        >= 2
                                }))
                    } else if self.shell.page() == AppPage::AppHistory {
                        !self.application_history_projection().rows.is_empty()
                    } else {
                        self.history_runtime.replay().is_open()
                            && !self.history_runtime.replay().rows().is_empty()
                    };
                    if ready {
                        match self.capture.presentation_frame {
                            CapturePresentationFrame::WaitingForData => {
                                self.capture.presentation_frame =
                                    CapturePresentationFrame::WaitingForPresentation;
                                false
                            }
                            CapturePresentationFrame::WaitingForPresentation => {
                                self.capture.presentation_frame =
                                    CapturePresentationFrame::Presented;
                                true
                            }
                            CapturePresentationFrame::Presented => true,
                        }
                    } else {
                        false
                    }
                } else {
                    true
                };
                if !self.capture.emitted && history_presented {
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
                            if let Ok(target) = std::env::var("TM_ICED_CAPTURE_DEVICE") {
                                crate::capture::append_target_marker(path, page, &target);
                            } else if self.local_surface_kind() == Some(LocalSurfaceKind::Health) {
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
                                crate::capture::append_target_marker(path, page, page);
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
