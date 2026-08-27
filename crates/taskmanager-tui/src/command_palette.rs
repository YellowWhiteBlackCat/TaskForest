//! Searchable command palette: the filterable keyboard reference and its
//! executable actions (ADR-027 frontend-local surface).
//!
//! `?` opens the palette (replacing the static help overlay): typing narrows
//! the keybinding rows, Enter runs the selected row's action, Esc closes. The
//! rows cover the shared router commands (executed through [`AppAction`]) and
//! the TUI-local bindings (executed through [`PaletteLocalAction`]), so the
//! palette is a true command entry point for the whole keyboard surface, not
//! just the shared commands. Extracted from `lib.rs` so no crate-root file
//! exceeds the source line budget; behavior unchanged — every method stays
//! reachable on `TuiApp` (impl blocks may live in any module of the defining
//! crate), and the types stay reachable at `crate::CommandPalette` /
//! `crate::CommandPaletteRow` / `crate::PaletteLocalAction` via `pub use`.

use taskmanager_application::{AppAction, AppPage, CommandId, PlatformEffect};
use taskmanager_shell::QuitReason;

use crate::{TuiApp, TuiSurface, TuiSurfaceKind};

/// One filterable row in the command palette: the shortcut + label shown to
/// the user, plus the shared action Enter executes when the row is selected.
/// Local-only rows carry the terminal-only [`PaletteLocalAction`] the TUI
/// can run itself (quit / sort / overlays / batch / clipboard …); `None` rows
/// are discoverable but not executable from the palette.
#[derive(Clone, Copy, Debug)]
pub struct CommandPaletteRow {
    pub shortcut: &'static str,
    pub label: &'static str,
    pub action: Option<AppAction>,
    /// The TUI-local action Enter runs for terminal-only rows. Mirrors the
    /// shared-action lane so the palette can execute the whole keyboard
    /// surface, not just the shared router commands.
    pub local_action: Option<PaletteLocalAction>,
}

/// A TUI-local command-palette action the TUI runs itself (no shared
/// `AppAction` exists for it). Each variant maps to one TUI binding so the
/// palette is a true command entry point for the whole keyboard surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteLocalAction {
    Quit,
    ToggleHelp,
    CycleSortColumn,
    ToggleSuggestions,
    ToggleSettings,
    ToggleAbout,
    ToggleHealth,
    ToggleContainers,
    ExportSnapshot,
    ToggleColumnMenu,
    ToggleProcessMenu,
    ToggleBatchMenu,
    CopyClipboard,
    OpenServiceLog,
    ToggleDirectoryScan,
}

/// The open command palette: the filter text and the cursor over the FILTERED
/// row list.
#[derive(Clone, Debug, Default)]
pub struct CommandPalette {
    pub filter: String,
    pub selection: usize,
}

impl TuiApp {
    /// The command-palette rows: the executable shared commands (page
    /// switches, refresh, properties, pause, system about, search focus, and
    /// the Home/End jumps) followed by the discoverable local bindings (quit /
    /// help / sort / overlays). Executable rows carry their
    /// [`AppAction`]; local rows carry `None` and are shown for discovery only.
    #[must_use]
    pub fn palette_rows() -> Vec<CommandPaletteRow> {
        // Shared router commands with an action that is safe to run from the
        // palette (no direction keys — the keyboard moves the cursor directly —
        // no dialog Confirm, no destructive End-task).
        let executable: &[CommandId] = &[
            CommandId::ShowPerformance,
            CommandId::ShowApplications,
            CommandId::ShowServices,
            CommandId::ShowSystem,
            CommandId::ShowStartup,
            CommandId::ShowUsers,
            CommandId::ShowAppHistory,
            CommandId::Refresh,
            CommandId::OpenProperties,
            CommandId::ShowSystemAbout,
            CommandId::TogglePause,
            CommandId::FocusSearch,
            CommandId::MoveToFirst,
            CommandId::MoveToLast,
        ];
        let mut rows: Vec<CommandPaletteRow> = crate::command_help()
            .into_iter()
            .filter(|help| executable.contains(&help.command))
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
        for binding in crate::shell_local_bindings() {
            rows.push(CommandPaletteRow {
                shortcut: binding.shortcut,
                label: binding.label,
                action: None,
                local_action: terminal_local_action(binding.shortcut),
            });
        }
        for binding in crate::ui::help::TUI_LOCAL_BINDINGS {
            rows.push(CommandPaletteRow {
                shortcut: binding.shortcut,
                label: binding.label,
                action: None,
                local_action: tui_local_action(binding.shortcut),
            });
        }
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
                taskmanager_application::text::contains_ascii_ci(row.shortcut, filter)
                    || taskmanager_application::text::contains_ascii_ci(row.label, filter)
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

    /// Run one TUI-local palette action (the local row the user selected).
    /// Maps back onto the same TUI bindings the keyboard uses, so the palette
    /// never executes anything the direct keys do not.
    pub fn run_palette_local_action(&mut self, action: Option<PaletteLocalAction>) {
        use PaletteLocalAction;
        match action {
            Some(PaletteLocalAction::Quit) => {
                self.shell.request_quit(QuitReason::CommandPalette);
            }
            Some(PaletteLocalAction::ToggleHelp) => self.toggle_help(),
            Some(PaletteLocalAction::CycleSortColumn) => {
                if self.page() == AppPage::Applications {
                    self.cycle_sort_column_visible();
                }
            }
            Some(PaletteLocalAction::ToggleSuggestions) => self.shell.toggle_suggestions(),
            Some(PaletteLocalAction::ToggleSettings) => self.toggle_settings(),
            Some(PaletteLocalAction::ToggleAbout) => self.toggle_about(),
            Some(PaletteLocalAction::ToggleHealth) => self.toggle_health(),
            Some(PaletteLocalAction::ToggleContainers) => self.toggle_containers(),
            Some(PaletteLocalAction::ExportSnapshot) => self.export_snapshot(),
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
            // Device-scoped actions run only when the palette's active page is
            // the matching Performance device (the same guard the direct keys
            // use), so the palette never executes them against a wrong device.
            Some(PaletteLocalAction::ToggleDirectoryScan)
                if self.page() == AppPage::Performance
                    && self.perf_device == crate::PerfDevice::Disk =>
            {
                let _ = self.toggle_directory_scan();
            }
            None
            | Some(PaletteLocalAction::OpenServiceLog)
            | Some(PaletteLocalAction::ToggleDirectoryScan) => {}
        }
    }
}

/// Map a terminal-only shortcut (from
/// [`crate::shell_local_bindings`]) to its executable palette action.
fn terminal_local_action(shortcut: &str) -> Option<PaletteLocalAction> {
    use PaletteLocalAction;
    match shortcut {
        "q" => Some(PaletteLocalAction::Quit),
        "?" => Some(PaletteLocalAction::ToggleHelp),
        "s" | "S" => Some(PaletteLocalAction::CycleSortColumn),
        "T" => Some(PaletteLocalAction::ToggleSuggestions),
        _ => None,
    }
}

/// Map a TUI-local shortcut (from [`crate::ui::help::TUI_LOCAL_BINDINGS`]) to
/// its executable palette action. `m` (mark the current row) and `e`
/// (context-sensitive escalation) are deliberately not executable from the
/// palette: they are table/device-scoped gestures over the current cursor
/// or projection, which a modal palette has no meaningful target for.
fn tui_local_action(shortcut: &str) -> Option<PaletteLocalAction> {
    use PaletteLocalAction;
    match shortcut {
        "p" => Some(PaletteLocalAction::ToggleSettings),
        "i" => Some(PaletteLocalAction::ToggleAbout),
        "h" => Some(PaletteLocalAction::ToggleHealth),
        "c" => Some(PaletteLocalAction::ToggleContainers),
        "x" => Some(PaletteLocalAction::ExportSnapshot),
        "C" => Some(PaletteLocalAction::ToggleColumnMenu),
        "B" => Some(PaletteLocalAction::ToggleBatchMenu),
        "y" => Some(PaletteLocalAction::CopyClipboard),
        "a" => Some(PaletteLocalAction::ToggleProcessMenu),
        "o" => Some(PaletteLocalAction::OpenServiceLog),
        "d" => Some(PaletteLocalAction::ToggleDirectoryScan),
        _ => None,
    }
}
