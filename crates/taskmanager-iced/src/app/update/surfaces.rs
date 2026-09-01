//! Iced-owned primary-surface message reducer.
//!
//! This system is the only update-domain entry that opens or closes a local
//! primary surface. Payload editing remains in its domain state (affinity,
//! run-task, first-run); visibility and input ownership never do.

use taskmanager_application::{PlatformEffect, RefreshRequest};

use super::super::{IcedApp, LocalSurface, LocalSurfaceKind, Message};

impl IcedApp {
    pub(super) fn handle_surface_message(&mut self, message: Message) -> Option<PlatformEffect> {
        match message {
            Message::DismissOverlay => {
                self.close_context_menus();
                self.shell.close_service_log();
                self.close_local_modals();
                self.shell.dismiss_overlay();
                None
            }
            message @ (Message::OpenProcessAffinity
            | Message::ToggleProcessAffinityCpu(_)
            | Message::SelectAllProcessAffinity
            | Message::ClearAllProcessAffinity
            | Message::InvertProcessAffinity
            | Message::SelectProcessAffinityPCores
            | Message::SelectProcessAffinityECores
            | Message::ApplyProcessAffinity) => self.handle_process_affinity_message(message),
            Message::OpenSettings => {
                self.open_local_surface(LocalSurface::Settings);
                None
            }
            Message::CloseSettings => {
                self.dismiss_local_surface_kind(LocalSurfaceKind::Settings);
                None
            }
            Message::OpenAbout => {
                self.open_local_surface(LocalSurface::About);
                None
            }
            Message::OpenHealth => {
                self.open_local_surface(LocalSurface::Health);
                None
            }
            Message::OpenContainers => {
                self.open_local_surface(LocalSurface::Containers);
                Some(PlatformEffect::Refresh(RefreshRequest::Containers))
            }
            Message::OpenDiskSmart { index } => {
                self.open_local_surface(LocalSurface::DiskSmart { index });
                None
            }
            Message::OpenAlertCenter => {
                self.open_local_surface(LocalSurface::AlertCenter);
                None
            }
            Message::CloseAlertCenter => {
                self.dismiss_local_surface_kind(LocalSurfaceKind::AlertCenter);
                None
            }
            message @ (Message::OpenRunTask
            | Message::CloseRunTask
            | Message::UpdateRunTaskCommand(_)
            | Message::SubmitRunTask) => self.handle_run_task_message(message),
            _ => None,
        }
    }
}
