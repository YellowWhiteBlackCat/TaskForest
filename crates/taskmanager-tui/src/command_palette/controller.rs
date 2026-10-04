//! Command-palette and surface-protocol execution over the declared registry.

use super::{
    CommandPalette, CommandPaletteRow, PALETTE_SHARED_COMMANDS, PaletteLocalAction,
    TUI_LOCAL_COMMANDS, TuiSurfaceAction, terminal_local_action,
};
use crate::{TuiApp, TuiSurface, TuiSurfaceKind};
use taskmanager_application::{AppPage, PlatformEffect, UrlOpenRequest};
use taskmanager_assets::product::REPOSITORY_URL;
use taskmanager_core::core::text::contains_ascii_ci;
use taskmanager_shell::presentation::command_help;
use taskmanager_shell::{InfoTable, QuitReason, shell_local_bindings};

impl TuiApp {
    /// The command-palette rows: the executable shared commands (page
    /// switches, refresh, properties, pause, system about, search focus, and
    /// the Home/End jumps) followed by the discoverable local bindings (quit /
    /// help / sort / overlays). Executable rows carry their
    /// [`taskmanager_application::AppAction`]; local rows carry `None` and are shown for discovery only.
    #[must_use]
    pub fn palette_rows() -> Vec<CommandPaletteRow> {
        let mut rows: Vec<CommandPaletteRow> = command_help()
            .into_iter()
            .filter(|help| PALETTE_SHARED_COMMANDS.contains(&help.command))
            .map(|help| CommandPaletteRow {
                shortcut: help.shortcut,
                label: help.label,
                action: Some(help.command.action()),
                local_action: None,
            })
            .collect();
        // Local rows are executable from the palette too: each terminal-only
        // chord and TUI-local binding maps to the [`PaletteLocalAction`] the
        // TUI runs itself, so the palette is a true command entry point for
        // the whole keyboard surface. The prefix jump and the F1 alias are
        // discoverable only (`None`).
        for binding in shell_local_bindings() {
            rows.push(CommandPaletteRow {
                shortcut: binding.shortcut,
                label: binding.label,
                action: None,
                local_action: terminal_local_action(binding.shortcut),
            });
        }
        for command in TUI_LOCAL_COMMANDS {
            rows.push(CommandPaletteRow {
                shortcut: command.binding.shortcut,
                label: command.binding.label,
                action: None,
                local_action: command.palette_action,
            });
        }
        rows.push(CommandPaletteRow {
            shortcut: "a",
            label: "Process affinity (Applications)",
            action: None,
            local_action: Some(PaletteLocalAction::OpenProcessAffinity),
        });
        rows.push(CommandPaletteRow {
            shortcut: "F1",
            label: "Toggle keyboard reference",
            action: None,
            local_action: Some(PaletteLocalAction::ToggleHelp),
        });
        rows.push(CommandPaletteRow {
            shortcut: "letter",
            label: "Jump by name prefix (Applications)",
            action: None,
            local_action: None,
        });
        rows
    }

    /// The palette rows narrowed by the current filter (case-insensitive
    /// match on the shortcut or label).
    #[must_use]
    pub fn filtered_palette_rows(&self) -> Vec<CommandPaletteRow> {
        let filter = self.command_palette().map_or("", |p| p.filter.trim());
        if filter.is_empty() {
            return Self::palette_rows();
        }
        Self::palette_rows()
            .into_iter()
            .filter(|row| {
                contains_ascii_ci(row.shortcut, filter) || contains_ascii_ci(row.label, filter)
            })
            .collect()
    }

    /// Open the searchable command palette (`?`), replacing the static help
    /// overlay.
    pub fn open_command_palette(&mut self) {
        self.help_scroll = 0;
        self.open_local_surface(TuiSurface::CommandPalette(CommandPalette::default()));
    }

    /// Close the command palette (and the help overlay it rendered through).
    pub fn close_command_palette(&mut self) {
        self.dismiss_local_surface_kind(TuiSurfaceKind::CommandPalette);
        self.help_scroll = 0;
    }

    /// Toggle the plain help overlay through the shell state machine, resetting
    /// the overlay's scroll so a freshly opened reference starts at the top.
    /// Shadows the `Deref`-provided `ShellApp::toggle_help` for the TUI surface
    /// only; the shell's own callers are unaffected.
    pub fn toggle_help(&mut self) {
        self.shell.toggle_help();
        self.help_scroll = 0;
    }

    /// Scroll the plain help overlay by `delta` rows (positive = down). The
    /// renderer clamps the stored offset to the binding-list length, so this
    /// only stores the user's intent; ↑/↓ move one row, PageUp/PageDown a page.
    pub fn help_scroll_by(&mut self, delta: isize) {
        if delta >= 0 {
            self.help_scroll = self.help_scroll.saturating_add(delta as usize);
        } else {
            self.help_scroll = self.help_scroll.saturating_sub(delta.unsigned_abs());
        }
    }

    /// Append a character to the palette filter and reset the cursor to the
    /// first filtered row.
    pub fn palette_push_char(&mut self, character: char) {
        if let Some(palette) = self.command_palette_mut() {
            palette.filter.push(character);
            palette.selection = 0;
        }
    }

    /// Pop the last filter character and reset the cursor.
    pub fn palette_backspace(&mut self) {
        if let Some(palette) = self.command_palette_mut() {
            palette.filter.pop();
            palette.selection = 0;
        }
    }

    /// Move the palette cursor over the filtered rows (clamped).
    pub fn palette_move(&mut self, delta: isize) {
        let count = self.filtered_palette_rows().len();
        if count == 0 {
            return;
        }
        if let Some(palette) = self.command_palette_mut() {
            palette.selection = palette
                .selection
                .saturating_add_signed(delta)
                .min(count - 1);
        }
    }

    /// Run the shared action of the selected filtered row, then close the
    /// palette. Returns the platform effect the action produced (e.g. a
    /// Refresh request), routed through the shared seam like every key.
    #[must_use]
    pub fn palette_select(&mut self) -> Option<PlatformEffect> {
        let row = self
            .filtered_palette_rows()
            .get(self.command_palette()?.selection)
            .copied()?;
        self.close_command_palette();
        if let Some(action) = row.action {
            return self.apply_action(action);
        }
        self.run_palette_local_action(row.local_action);
        None
    }

    /// Execute one declared [`TuiSurfaceAction`] (layer 3 of the registry).
    /// The overlay toggles are the same methods the direct arms and the
    /// palette run, so a protocol chord can never diverge from its command
    /// twin; the service-log transitions stay owned by the shell. Mirrors
    /// [`Self::run_palette_local_action`] as the single execution site of
    /// its lane.
    pub(crate) fn run_surface_protocol_action(
        &mut self,
        action: TuiSurfaceAction,
    ) -> Option<PlatformEffect> {
        match action {
            TuiSurfaceAction::OpenRepository => {
                return Some(PlatformEffect::OpenUrl(UrlOpenRequest {
                    url: REPOSITORY_URL.into(),
                }));
            }
            TuiSurfaceAction::OpenSystemInformation => self.open_system_information(),
            TuiSurfaceAction::CopyInformation => self.copy_information_to(&mut std::io::stdout()),
            TuiSurfaceAction::ToggleSettings => self.toggle_settings(),
            TuiSurfaceAction::ToggleAbout => self.toggle_about(),
            TuiSurfaceAction::ToggleHealth => self.toggle_health(),
            TuiSurfaceAction::ToggleContainers => self.toggle_containers(),
            TuiSurfaceAction::ToggleServiceLogFollow => self.shell.toggle_service_log_follow(),
            TuiSurfaceAction::ToggleServiceLogPaused => self.shell.toggle_service_log_paused(),
            TuiSurfaceAction::CycleServiceLogLevel => self.shell.cycle_service_log_level(),
            TuiSurfaceAction::CycleServiceLogTime => self.shell.cycle_service_log_time(),
        }
        None
    }

    /// Run one TUI-local palette action (the local row the user selected).
    /// Maps back onto the same TUI bindings the keyboard uses, so the palette
    /// never executes anything the direct keys do not.
    pub fn run_palette_local_action(&mut self, action: Option<PaletteLocalAction>) {
        use PaletteLocalAction;
        match action {
            Some(PaletteLocalAction::ToggleHistoryReplay) => self.toggle_history_replay(),
            Some(PaletteLocalAction::RefreshHistoryReplay) => self.refresh_history_replay(),
            Some(PaletteLocalAction::Quit) => {
                self.shell.request_quit(QuitReason::CommandPalette);
            }
            Some(PaletteLocalAction::ToggleHelp) => self.toggle_help(),
            Some(PaletteLocalAction::CycleSortColumn) => match self.page() {
                AppPage::Applications => self.cycle_sort_column_visible(),
                AppPage::Services => {
                    self.cycle_info_sort_column_preserving_anchor(InfoTable::Services)
                }
                AppPage::Startup => {
                    self.cycle_info_sort_column_preserving_anchor(InfoTable::Startup)
                }
                AppPage::Users => self.cycle_info_sort_column_preserving_anchor(InfoTable::Users),
                AppPage::Performance | AppPage::System | AppPage::AppHistory => {}
            },
            Some(PaletteLocalAction::ToggleSortDirection) => match self.page() {
                AppPage::Applications => {
                    self.toggle_sort_direction();
                    self.persist_process_prefs();
                }
                AppPage::Services => {
                    self.toggle_info_sort_direction_preserving_anchor(InfoTable::Services)
                }
                AppPage::Startup => {
                    self.toggle_info_sort_direction_preserving_anchor(InfoTable::Startup)
                }
                AppPage::Users => {
                    self.toggle_info_sort_direction_preserving_anchor(InfoTable::Users)
                }
                AppPage::Performance | AppPage::System | AppPage::AppHistory => {}
            },
            Some(PaletteLocalAction::ToggleSuggestions) => self.shell.toggle_suggestions(),
            Some(PaletteLocalAction::ToggleSettings) => self.toggle_settings(),
            Some(PaletteLocalAction::ToggleAbout) => self.toggle_about(),
            Some(PaletteLocalAction::ToggleHealth) => self.toggle_health(),
            Some(PaletteLocalAction::ToggleContainers) => self.toggle_containers(),
            Some(PaletteLocalAction::ExportSnapshot) => self.export_snapshot(),
            Some(PaletteLocalAction::ExportDiagnosticReport) => {
                self.open_diagnostic_bundle();
            }
            Some(PaletteLocalAction::OpenProcessAffinity)
                if self.page() == AppPage::Applications =>
            {
                let _ = self.open_process_affinity();
            }
            Some(PaletteLocalAction::ToggleColumnMenu) => self.toggle_column_menu(),
            Some(PaletteLocalAction::ToggleProcessMenu) => {
                let _ = self.open_process_menu();
            }
            Some(PaletteLocalAction::ToggleBatchMenu) => {
                let _ = self.open_batch_menu();
            }
            Some(PaletteLocalAction::CopyClipboard) => {
                self.copy_selected_process(&mut std::io::stdout())
            }
            Some(PaletteLocalAction::OpenServiceLog) if self.page() == AppPage::Services => {
                let _ = self.shell.open_service_log();
            }
            Some(PaletteLocalAction::ExportServiceLog)
                if self.page() == AppPage::Services && self.shell.service_log.is_some() =>
            {
                self.export_service_log();
            }
            // Device-scoped actions run only when the palette's active page is
            // the matching Performance device (the same guard the direct keys
            // use), so the palette never executes them against a wrong device.
            Some(PaletteLocalAction::ToggleDirectoryScan)
                if self.page() == AppPage::Performance
                    && self.perf_device == crate::PerfDevice::Disk =>
            {
                let _ = self.toggle_directory_scan();
            }
            Some(PaletteLocalAction::ToggleGpuChartMetric)
                if self.page() == AppPage::Performance
                    && self.perf_device == crate::PerfDevice::Gpu =>
            {
                self.cycle_gpu_chart_metric();
            }
            // The SMART arm additionally refuses a snapshot without a
            // SMART-capable disk — the same readiness the direct scope
            // demands — so the palette can never arm a gate with no target.
            Some(PaletteLocalAction::RequestSmartSelfTest)
                if self.page() == AppPage::Performance
                    && self.perf_device == crate::PerfDevice::Disk =>
            {
                let _ = self.arm_smart_self_test();
            }
            Some(PaletteLocalAction::BrowseServiceDependencies)
                if self.page() == AppPage::Services =>
            {
                let _ = self.open_service_dependencies();
            }
            None
            | Some(PaletteLocalAction::OpenServiceLog)
            | Some(PaletteLocalAction::ExportServiceLog)
            | Some(PaletteLocalAction::ToggleDirectoryScan)
            | Some(PaletteLocalAction::ToggleGpuChartMetric)
            | Some(PaletteLocalAction::RequestSmartSelfTest)
            | Some(PaletteLocalAction::BrowseServiceDependencies)
            | Some(PaletteLocalAction::OpenProcessAffinity) => {}
        }
    }
}
