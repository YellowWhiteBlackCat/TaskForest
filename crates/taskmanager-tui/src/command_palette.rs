//! Searchable command palette and the TUI-local binding registry (ADR-027
//! frontend-local surface).
//!
//! # The registry layers (who owns which chord)
//!
//! 1. **Shell layer** — `taskmanager_shell::route_key` plus
//!    [`taskmanager_shell::shell_local_bindings`] own the shared commands and
//!    the five shell-local characters (`q` `?` `s` `S` `T`). The TUI refines
//!    their *execution* (identity-preserving sort, palette-instead-of-help)
//!    but never re-declares those chords; the only TUI-side chord→action map
//!    for them is [`terminal_local_action`].
//! 2. **This registry** — [`TUI_LOCAL_COMMANDS`] owns every TUI-local
//!    command chord. It is the single authority for the help rows, the
//!    palette rows, AND the direct keyboard dispatch (the `direct` arms below
//!    are what `runtime::keys` executes). A chord may appear in layer 1 or
//!    layer 2, never both; the binding-matrix test enforces the disjointness.
//! 3. **Surface-modal protocol** — action-semantic character chords consumed
//!    only while a modal surface owns input. [`TUI_SURFACE_PROTOCOL`] is
//!    their single typed declaration source: the settings form's `p i h c`,
//!    the About/Health/Containers overlays' `i h c`, and the open service-log
//!    panel's `f p l t`. They never appear in help/palette — they are input
//!    protocol of the open surface, not commands.
//!    *Hard boundary against layers 1-2:* this layer is consulted at the top
//!    of the modal precedence, above shell and registry dispatch, so a chord
//!    declared both here and in a command layer can never double-route: the
//!    owning surface consumes it first and the command layer never sees it
//!    (the surface-protocol tests lock that masking invariant). The inverse
//!    direction is deliberate partial ownership: full-modal surfaces consume
//!    every key while up, while the service-log panel consumes only its
//!    declared chords and falls the rest through to the command layers.
//!    Structural surface-lifecycle keys (Esc, Enter, Tab/arrow navigation,
//!    the panel's `q` close) stay hand-written at their dispatch sites and
//!    are deliberately not declared here; the same holds for the pure
//!    navigation/text protocols (action menus, palette editing,
//!    Process-Properties). The painted footer hints of these surfaces are the
//!    one presentation lane derived from this layer: [`TUI_SURFACE_HINTS`]
//!    cites each protocol arm it paints (a parity test pins hint ⇄ protocol
//!    coherence), while a structural key folded into a painted token (the
//!    overlays' `/ Esc` glyph, the panel's localized `Esc closes` prefix)
//!    and the unpainted structural `q` close stay outside both tables.
//! 4. **Contextual gestures** — always-conditional moves with no command
//!    identity (name-prefix jump letters, `r`/`R` source retry, the
//!    AppHistory window digits, `F1`/`F9`, `Tab`). Deliberately outside the
//!    registry; each stays in exactly one hand-written dispatch arm in
//!    `runtime::keys`.
//!
//! `?` opens the palette (replacing the static help overlay): typing narrows
//! the keybinding rows, Enter runs the selected row's action, Esc closes. The
//! rows cover the shared router commands (executed through [`AppAction`]) and
//! the TUI-local bindings (executed through [`PaletteLocalAction`]), so the
//! palette is a true command entry point for the whole keyboard surface, not
//! just the shared commands. Extracted from `lib.rs` so no crate-root file
//! exceeds the source line budget; every method stays reachable on `TuiApp`
//! (impl blocks may live in any module of the defining crate), and the types
//! stay reachable at `crate::CommandPalette` / `crate::CommandPaletteRow` /
//! `crate::PaletteLocalAction` via `pub use`.

use taskmanager_application::i18n::t;
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
    ToggleSortDirection,
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
    ExportServiceLog,
    ToggleDirectoryScan,
    /// `g` on the Performance·GPU page: cycles the headline chart metric.
    ToggleGpuChartMetric,
    /// `t` on the Performance·Disk page: arms the shared SMART self-test
    /// confirmation gate (the platform request stays gated behind `y`).
    RequestSmartSelfTest,
}

/// The typed direct-dispatch lane: what a TUI-local command DOES when its
/// declared chord is pressed on the keyboard. [`TUI_LOCAL_COMMANDS`] is the
/// single authority binding a shortcut to these actions; `runtime::keys`
/// resolves the pressed chord through the registry and executes the first
/// armed arm, so a hand-written `match` on a registry chord there is drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiDirectAction {
    ToggleSettings,
    ToggleAbout,
    ToggleHealth,
    ToggleContainers,
    ExportSnapshot,
    /// Digits `1`-`7` select the Performance resource the digit names.
    SelectPerfResource,
    /// `Enter` opens the selected row's action menu / properties.
    OpenServiceMenu,
    OpenSessionMenu,
    OpenStartupMenu,
    OpenProcessProperties,
    ToggleColumnMenu,
    ToggleMarkedProcess,
    ToggleBatchMenu,
    CopyClipboard,
    OpenProcessMenu,
    OpenServiceLog,
    ExportServiceLog,
    /// `e` on an escalation-ready Applications insight (G-04b).
    RequestNetworkEscalation,
    /// `e` on the Performance·GPU page.
    ToggleGpuEngineRows,
    /// `g` cycles the GPU headline chart metric (ADR-034 stage 2).
    CycleGpuChartMetric,
    /// `d` on the Performance·Disk page.
    ToggleDirectoryScan,
    /// `t` on the Performance·Disk page: arms the shared SMART self-test
    /// confirmation gate (TUI-013). The confirm `y` emits the typed effect.
    RequestSmartSelfTest,
}

/// Where (and under which modifier policy) a direct arm is armed. Declared as
/// data so the binding-matrix test pins each command's scope; the guard
/// implementation lives beside the executor in `runtime::keys`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiDirectScope {
    /// Any page, any modifiers — the shell's own characters already had their
    /// chance in the precedence order.
    Anywhere,
    /// Applications page; Ctrl/Alt refused (chorded variants stay unwired).
    ApplicationsPage,
    /// [`TuiDirectScope::ApplicationsPage`] plus the insights panel reporting
    /// the typed `RequiresEscalation` network facet (G-04b).
    ApplicationsEscalationReady,
    /// The page's selected row offers an action target; Enter opens it.
    /// Modifier-less by contract (the historical behavior ignores chords).
    RowTarget(AppPage),
    /// Performance page, bare digit (Shift passes; Ctrl/Alt/platform refused).
    PerformanceResourceDigit,
    /// Services page with the log panel closed (the panel owns keys while up).
    ServicesPageLogClosed,
    /// Services page with the log panel open.
    ServicesPageLogOpen,
    /// Performance page viewing the GPU device; Ctrl/Alt refused.
    PerformanceGpuPage,
    /// Performance page viewing the Disk device; Ctrl/Alt refused.
    PerformanceDiskPage,
    /// [`TuiDirectScope::PerformanceDiskPage`] plus a snapshot disk whose
    /// provider reports `SmartAvailability::Available` — the disk the gate
    /// freezes as its target (TUI-013).
    PerformanceDiskSmartReady,
}

/// One executable arm of a registry command: scope guard + action. A command
/// with several context-sensitive executions (the page-scoped `Enter`, the
/// dual-personality `e`) declares one arm per context; the first armed arm
/// wins, so arm order inside an entry is precedence.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TuiDirectArm {
    pub(crate) scope: TuiDirectScope,
    pub(crate) action: TuiDirectAction,
}

/// The registry's display token for the row-target (`Enter`) command; the
/// content-level Enter resolver answers exactly this row.
pub(crate) const ROW_TARGET_SHORTCUT: &str = "Enter";
/// The registry's display token for the Performance resource digit range; the
/// digit resolver answers exactly this row.
pub(crate) const RESOURCE_DIGITS_SHORTCUT: &str = "1-7";

/// One TUI-local shortcut together with the two execution lanes the command
/// palette and the direct key router use for it.  Help rows, palette rows and
/// the direct dispatch all derive from this one table; a row that is
/// discoverable but intentionally not palette-executable carries
/// `None` explicitly (for example the context-sensitive `m` and `e`
/// gestures), and every entry wires at least one [`TuiDirectArm`] so an
/// advertised chord always executes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TuiLocalCommand {
    pub(crate) binding: taskmanager_shell::LocalBinding,
    pub(crate) palette_action: Option<PaletteLocalAction>,
    /// Direct-dispatch arms, tried in order (first armed arm wins).
    pub(crate) direct: &'static [TuiDirectArm],
}

/// The complete TUI-local binding registry.  The direct key router resolves
/// every chord here — declaration and execution are one authority.
pub(crate) const TUI_LOCAL_COMMANDS: [TuiLocalCommand; 17] = [
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "p",
            label: "Settings",
        },
        palette_action: Some(PaletteLocalAction::ToggleSettings),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::ToggleSettings,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "i",
            label: "About / system info",
        },
        palette_action: Some(PaletteLocalAction::ToggleAbout),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::ToggleAbout,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "h",
            label: "System health & alerts",
        },
        palette_action: Some(PaletteLocalAction::ToggleHealth),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::ToggleHealth,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "c",
            label: "Containers",
        },
        palette_action: Some(PaletteLocalAction::ToggleContainers),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::ToggleContainers,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "x",
            label: "Export snapshot",
        },
        palette_action: Some(PaletteLocalAction::ExportSnapshot),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::ExportSnapshot,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: ROW_TARGET_SHORTCUT,
            label: "Service actions (Services page)",
        },
        palette_action: None,
        direct: &[
            TuiDirectArm {
                scope: TuiDirectScope::RowTarget(AppPage::Services),
                action: TuiDirectAction::OpenServiceMenu,
            },
            TuiDirectArm {
                scope: TuiDirectScope::RowTarget(AppPage::Users),
                action: TuiDirectAction::OpenSessionMenu,
            },
            TuiDirectArm {
                scope: TuiDirectScope::RowTarget(AppPage::Startup),
                action: TuiDirectAction::OpenStartupMenu,
            },
            TuiDirectArm {
                scope: TuiDirectScope::RowTarget(AppPage::Applications),
                action: TuiDirectAction::OpenProcessProperties,
            },
        ],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: RESOURCE_DIGITS_SHORTCUT,
            label: "Performance resource (Performance page)",
        },
        palette_action: None,
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::PerformanceResourceDigit,
            action: TuiDirectAction::SelectPerfResource,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "C",
            label: "Columns (Applications page)",
        },
        palette_action: Some(PaletteLocalAction::ToggleColumnMenu),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::ApplicationsPage,
            action: TuiDirectAction::ToggleColumnMenu,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "m",
            label: "Mark process for batch control (Applications page)",
        },
        palette_action: None,
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::ApplicationsPage,
            action: TuiDirectAction::ToggleMarkedProcess,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "B",
            label: "Batch actions on marked processes (Applications page)",
        },
        palette_action: Some(PaletteLocalAction::ToggleBatchMenu),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::ApplicationsPage,
            action: TuiDirectAction::ToggleBatchMenu,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "y",
            label: "Copy selected pid+name to clipboard (Applications page)",
        },
        palette_action: Some(PaletteLocalAction::CopyClipboard),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::ApplicationsPage,
            action: TuiDirectAction::CopyClipboard,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "a",
            label: "Process actions · open location / search (Applications page)",
        },
        palette_action: Some(PaletteLocalAction::ToggleProcessMenu),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::ApplicationsPage,
            action: TuiDirectAction::OpenProcessMenu,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "o",
            label: "Service logs (Services)",
        },
        palette_action: Some(PaletteLocalAction::OpenServiceLog),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::ServicesPageLogClosed,
            action: TuiDirectAction::OpenServiceLog,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "e",
            label: "GPU engines (Performance·GPU) · network escalate (process)",
        },
        palette_action: None,
        direct: &[
            TuiDirectArm {
                scope: TuiDirectScope::ApplicationsEscalationReady,
                action: TuiDirectAction::RequestNetworkEscalation,
            },
            TuiDirectArm {
                scope: TuiDirectScope::PerformanceGpuPage,
                action: TuiDirectAction::ToggleGpuEngineRows,
            },
            TuiDirectArm {
                scope: TuiDirectScope::ServicesPageLogOpen,
                action: TuiDirectAction::ExportServiceLog,
            },
        ],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "d",
            label: "Directory usage scan (Performance·Disk)",
        },
        palette_action: Some(PaletteLocalAction::ToggleDirectoryScan),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::PerformanceDiskPage,
            action: TuiDirectAction::ToggleDirectoryScan,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "t",
            label: "SMART self-test (Performance·Disk)",
        },
        palette_action: Some(PaletteLocalAction::RequestSmartSelfTest),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::PerformanceDiskSmartReady,
            action: TuiDirectAction::RequestSmartSelfTest,
        }],
    },
    TuiLocalCommand {
        binding: taskmanager_shell::LocalBinding {
            shortcut: "g",
            label: "GPU chart metric (Performance·GPU)",
        },
        palette_action: Some(PaletteLocalAction::ToggleGpuChartMetric),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::PerformanceGpuPage,
            action: TuiDirectAction::CycleGpuChartMetric,
        }],
    },
];

mod surface_protocol;

pub(crate) use surface_protocol::{
    TuiSurfaceAction, TuiSurfaceScope, surface_hint_pairs, surface_hint_run,
    surface_protocol_action,
};
// The protocol/hint tables and their row types are consumed by the binding
// and hint-parity matrix tests; lib dispatch resolves through
// `surface_protocol_action` and the painted-footer lane above.
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use surface_protocol::{TUI_SURFACE_HINTS, TUI_SURFACE_PROTOCOL, TuiSurfaceArm};

/// Shared commands safe to invoke from the command palette.  Destructive
/// actions and direction-key commands stay out of the palette because their
/// direct context/confirmation path is the only honest target.
pub(crate) const PALETTE_SHARED_COMMANDS: [CommandId; 14] = [
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
        let mut rows: Vec<CommandPaletteRow> = taskmanager_shell::presentation::command_help()
            .into_iter()
            .filter(|help| crate::command_palette::PALETTE_SHARED_COMMANDS.contains(&help.command))
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
        for binding in taskmanager_shell::shell_local_bindings() {
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
                taskmanager_core::core::text::contains_ascii_ci(row.shortcut, filter)
                    || taskmanager_core::core::text::contains_ascii_ci(row.label, filter)
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
    pub(crate) fn run_surface_protocol_action(&mut self, action: TuiSurfaceAction) {
        match action {
            TuiSurfaceAction::ToggleSettings => self.toggle_settings(),
            TuiSurfaceAction::ToggleAbout => self.toggle_about(),
            TuiSurfaceAction::ToggleHealth => self.toggle_health(),
            TuiSurfaceAction::ToggleContainers => self.toggle_containers(),
            TuiSurfaceAction::ToggleServiceLogFollow => self.shell.toggle_service_log_follow(),
            TuiSurfaceAction::ToggleServiceLogPaused => self.shell.toggle_service_log_paused(),
            TuiSurfaceAction::CycleServiceLogLevel => self.shell.cycle_service_log_level(),
            TuiSurfaceAction::CycleServiceLogTime => self.shell.cycle_service_log_time(),
        }
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
            Some(PaletteLocalAction::CycleSortColumn) => match self.page() {
                AppPage::Applications => self.cycle_sort_column_visible(),
                AppPage::Services => self.cycle_info_sort_column_preserving_anchor(
                    taskmanager_shell::InfoTable::Services,
                ),
                AppPage::Startup => self.cycle_info_sort_column_preserving_anchor(
                    taskmanager_shell::InfoTable::Startup,
                ),
                AppPage::Users => self
                    .cycle_info_sort_column_preserving_anchor(taskmanager_shell::InfoTable::Users),
                AppPage::Performance | AppPage::System | AppPage::AppHistory => {}
            },
            Some(PaletteLocalAction::ToggleSortDirection) => match self.page() {
                AppPage::Applications => {
                    self.toggle_sort_direction();
                    self.persist_process_prefs();
                }
                AppPage::Services => self.toggle_info_sort_direction_preserving_anchor(
                    taskmanager_shell::InfoTable::Services,
                ),
                AppPage::Startup => self.toggle_info_sort_direction_preserving_anchor(
                    taskmanager_shell::InfoTable::Startup,
                ),
                AppPage::Users => self.toggle_info_sort_direction_preserving_anchor(
                    taskmanager_shell::InfoTable::Users,
                ),
                AppPage::Performance | AppPage::System | AppPage::AppHistory => {}
            },
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
            None
            | Some(PaletteLocalAction::OpenServiceLog)
            | Some(PaletteLocalAction::ExportServiceLog)
            | Some(PaletteLocalAction::ToggleDirectoryScan)
            | Some(PaletteLocalAction::ToggleGpuChartMetric)
            | Some(PaletteLocalAction::RequestSmartSelfTest) => {}
        }
    }
}

/// Map a terminal-only shortcut (from [`taskmanager_shell::shell_local_bindings`]) to its
/// executable palette action. This is the ONLY chord→action mapping for the
/// shell-owned characters (registry layer 1): the TUI never re-declares them,
/// it only records how the palette re-runs them locally.
fn terminal_local_action(shortcut: &str) -> Option<PaletteLocalAction> {
    use PaletteLocalAction;
    match shortcut {
        "q" => Some(PaletteLocalAction::Quit),
        "?" => Some(PaletteLocalAction::ToggleHelp),
        "s" => Some(PaletteLocalAction::CycleSortColumn),
        "S" => Some(PaletteLocalAction::ToggleSortDirection),
        "T" => Some(PaletteLocalAction::ToggleSuggestions),
        _ => None,
    }
}
