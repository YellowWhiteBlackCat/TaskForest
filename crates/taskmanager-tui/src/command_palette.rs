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
//!    the Health/Containers overlays' `i h c`, About's `r i c a`, and the open service-log
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
use taskmanager_application::{AppAction, AppPage, CommandId};

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
    /// Browse dependencies for the selected service on the Services page.
    BrowseServiceDependencies,
    ExportDiagnosticReport,
    OpenProcessAffinity,
    ToggleHistoryReplay,
    RefreshHistoryReplay,
    CaptureWindow,
}

/// The typed direct-dispatch lane: what a TUI-local command DOES when its
/// declared chord is pressed on the keyboard. [`TUI_LOCAL_COMMANDS`] is the
/// single authority binding a shortcut to these actions; `runtime::keys`
/// resolves the pressed chord through the registry and executes the first
/// armed arm, so a hand-written `match` on a registry chord there is drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiDirectAction {
    ToggleHistoryReplay,
    RefreshHistoryReplay,
    SystemDashboard,
    SystemHardware,
    SelectSystemHistoryWindow,
    ToggleSettings,
    ToggleAbout,
    ToggleHealth,
    ToggleContainers,
    ExportSnapshot,
    ExportDiagnosticReport,
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
    /// `d` on the Services page: open service dependencies browsing modal.
    BrowseServiceDependencies,
    /// `S`: capture the current window to a PNG through the shared platform
    /// capture backend (the active compositor window).
    CaptureWindow,
}

/// Where (and under which modifier policy) a direct arm is armed. Declared as
/// data so the binding-matrix test pins each command's scope; the guard
/// implementation lives beside the executor in `runtime::keys`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiDirectScope {
    PerformanceHistoryAvailable,
    HistoryReviewAvailable,
    SystemPage,
    SystemDashboard,
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
    /// Services page; Ctrl/Alt refused.
    ServicesPage,
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
pub(crate) const SYSTEM_HISTORY_DIGITS_SHORTCUT: &str = "1-4";

/// One TUI-local shortcut together with the two execution lanes the command
/// palette and the direct key router use for it.  Help rows, palette rows and
/// the direct dispatch all derive from this one table; a row that is
/// discoverable but intentionally not palette-executable carries
/// `None` explicitly (for example the context-sensitive `m` and `e`
/// gestures), and every entry wires at least one [`TuiDirectArm`] so an
/// advertised chord always executes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TuiLocalCommand {
    pub(crate) binding: LocalBinding,
    pub(crate) palette_action: Option<PaletteLocalAction>,
    /// Direct-dispatch arms, tried in order (first armed arm wins).
    pub(crate) direct: &'static [TuiDirectArm],
}

/// The complete TUI-local binding registry.  The direct key router resolves
/// every chord here — declaration and execution are one authority.
pub(crate) const TUI_LOCAL_COMMANDS: [TuiLocalCommand; 24] = [
    TuiLocalCommand {
        binding: LocalBinding {
            shortcut: "w",
            label: "System dashboard",
        },
        palette_action: None,
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::SystemPage,
            action: TuiDirectAction::SystemDashboard,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
            shortcut: SYSTEM_HISTORY_DIGITS_SHORTCUT,
            label: "System history window",
        },
        palette_action: None,
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::SystemDashboard,
            action: TuiDirectAction::SelectSystemHistoryWindow,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
            shortcut: "b",
            label: "System hardware details",
        },
        palette_action: None,
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::SystemPage,
            action: TuiDirectAction::SystemHardware,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
            shortcut: "r",
            label: "History replay",
        },
        palette_action: Some(PaletteLocalAction::ToggleHistoryReplay),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::PerformanceHistoryAvailable,
            action: TuiDirectAction::ToggleHistoryReplay,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
            shortcut: "f",
            label: "Refresh history",
        },
        palette_action: Some(PaletteLocalAction::RefreshHistoryReplay),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::HistoryReviewAvailable,
            action: TuiDirectAction::RefreshHistoryReplay,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
            shortcut: "X",
            label: "Export diagnostic report",
        },
        palette_action: Some(PaletteLocalAction::ExportDiagnosticReport),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::ExportDiagnosticReport,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
            shortcut: "V",
            label: "Capture current window",
        },
        palette_action: Some(PaletteLocalAction::CaptureWindow),
        direct: &[TuiDirectArm {
            scope: TuiDirectScope::Anywhere,
            action: TuiDirectAction::CaptureWindow,
        }],
    },
    TuiLocalCommand {
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
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
        binding: LocalBinding {
            shortcut: "d",
            label: "Directory usage scan (Performance·Disk)",
        },
        palette_action: Some(PaletteLocalAction::ToggleDirectoryScan),
        direct: &[
            TuiDirectArm {
                scope: TuiDirectScope::PerformanceDiskPage,
                action: TuiDirectAction::ToggleDirectoryScan,
            },
            TuiDirectArm {
                scope: TuiDirectScope::ServicesPage,
                action: TuiDirectAction::BrowseServiceDependencies,
            },
        ],
    },
    TuiLocalCommand {
        binding: LocalBinding {
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
        binding: LocalBinding {
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
use taskmanager_shell::LocalBinding;

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

mod controller;

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
