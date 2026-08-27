//! Startup-control state machine for [`super::TuiApp`]: the frozen
//! Enable/Disable menu target, the menu cursor, and the confirmation gate
//! (extracted so the state module stays under the source-size budget).
//!
//! The shared shell owns the gated request (`pending_startup`): selecting a
//! menu action calls `Shell::request_startup_control`, and the platform
//! effect is produced only by `Shell::confirm_startup_control` on explicit y
//! (mirroring the session-action flow).

use taskmanager_application::{AppPage, PlatformEffect};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::ui::startup_menu::{MENU_ACTIONS, StartupMenuTarget};
use crate::{TuiApp, TuiSurface, TuiSurfaceKind};

impl TuiApp {
    /// Open the Enable/Disable menu for the selected Startup-page row.
    /// Returns false (and an honest status line) when no row or no provider
    /// target is available. The cursor indexes the sorted projection the
    /// renderer paints (`sorted_startup_entry_at` is the single row→target
    /// translation).
    pub fn open_startup_menu(&mut self) -> bool {
        if self.page() != AppPage::Startup {
            return false;
        }
        let Some(entry) = self.sorted_startup_entry_at(self.selected) else {
            return false;
        };
        if entry.id.is_empty() {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "This startup entry has no provider target",
            );
            return false;
        }
        self.open_local_surface(TuiSurface::StartupMenu(StartupMenuTarget {
            entry: entry.clone(),
            selection: 0,
        }));
        true
    }

    /// Move the startup menu cursor (clamped).
    pub fn startup_menu_move(&mut self, delta: isize) {
        if let Some(menu) = self.startup_menu_mut() {
            menu.selection = menu
                .selection
                .saturating_add_signed(delta)
                .min(MENU_ACTIONS.len() - 1);
        }
    }

    /// Consume the menu selection: gate the chosen Enable/Disable through the
    /// shell's shared `pending_startup` slot. The platform request is only
    /// emitted by [`Self::confirm_startup_control`]. The frozen entry's id is
    /// checked against the current selection so a refresh cannot redirect the
    /// intent to a different row.
    pub fn startup_menu_select(&mut self) {
        let Some(TuiSurface::StartupMenu(menu)) =
            self.take_local_surface(TuiSurfaceKind::StartupMenu)
        else {
            return;
        };
        let enabled = MENU_ACTIONS.get(menu.selection).copied().unwrap_or(true);
        let still_selected = self
            .sorted_startup_entry_at(self.selected)
            .is_some_and(|entry| entry.id == menu.entry.id);
        if !still_selected {
            self.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                "Startup target changed; action cancelled",
            );
            return;
        }
        let _ = self.shell.request_startup_control(enabled);
    }

    /// Confirm the pending startup-control request through the shell's gated
    /// path: emit the `PlatformEffect::StartupControl` and clear the gate.
    #[must_use]
    pub fn confirm_startup_control(&mut self) -> Option<PlatformEffect> {
        self.shell.confirm_startup_control()
    }

    /// Dismiss the pending startup-control gate without submitting (n / Esc).
    pub fn dismiss_startup_control(&mut self) {
        self.shell.dismiss_overlay();
    }
}
