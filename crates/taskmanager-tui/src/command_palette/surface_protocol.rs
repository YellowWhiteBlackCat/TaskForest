//! The surface-modal protocol registry (layer 3): action-semantic character
//! chords consumed while a modal surface owns input, plus the painted
//! footer-hint vocabulary derived from it. Split from the command-palette
//! module, whose header owns the masking contract against the command
//! layers; the dispatch itself lives in `runtime::modals` and
//! `runtime::handle_settings_key`.

use super::*;

/// Which owning surface a protocol chord is consumed by. Declared as data so
/// the surface-protocol matrix can pin each scope's exact chord set; the
/// consumption rule (full-modal surfaces swallow every key, the service-log
/// panel only its declared chords) lives beside the dispatch in
/// `runtime::modals` and `runtime::handle_settings_key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiSurfaceScope {
    /// The settings form modal (`runtime::handle_settings_key`).
    Settings,
    /// The three status overlays (About / Health / Containers), which share
    /// one toggle protocol in `runtime::modals`.
    StatusOverlay,
    /// The Services-page service-log panel: a partial owner whose unclaimed
    /// chords fall through to the command layers.
    ServiceLogPanel,
}

/// What a surface-protocol chord DOES while its surface owns input. The
/// overlay toggles run the very methods the global registry's direct arms
/// run, so a chord can never mean one thing as a command and another inside
/// a modal; the service-log transitions stay owned by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiSurfaceAction {
    /// `p` inside the settings form self-closes it (the toggle precedent).
    ToggleSettings,
    ToggleAbout,
    ToggleHealth,
    ToggleContainers,
    /// `f` on the service-log panel: toggle tail follow.
    ToggleServiceLogFollow,
    /// `p` on the service-log panel: toggle feed pause — the panel outranks
    /// the global settings command of the same chord.
    ToggleServiceLogPaused,
    /// `l` on the service-log panel: cycle the level filter.
    CycleServiceLogLevel,
    /// `t` on the service-log panel: cycle the time filter.
    CycleServiceLogTime,
}

/// One declared protocol arm: `chord` runs `action` while `scope` owns
/// input. Modifiers are deliberately not part of the declaration — the open
/// surface claims its chords chorded or bare (the historical behavior this
/// table preserves verbatim).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TuiSurfaceArm {
    pub(crate) scope: TuiSurfaceScope,
    pub(crate) chord: char,
    pub(crate) action: TuiSurfaceAction,
}

/// The typed single authority for every action-semantic surface-protocol
/// chord: `p i h c` inside the settings form, `i h c` inside the
/// About/Health/Containers overlays, `f p l t` on the open service-log
/// panel. A hand-written `match` on one of these chords in
/// `runtime::modals` / `runtime::handle_settings_key` is drift. Structural
/// keys (Esc, Enter, Tab/arrows, the panel's `q` close) stay hand-written at
/// their dispatch sites and must never appear here — the matrix pins the
/// bare-lowercase-letter shape that enforces it. See the command-palette
/// module header for the hard masking contract against the command layers.
pub(crate) const TUI_SURFACE_PROTOCOL: [TuiSurfaceArm; 11] = [
    // Settings form: the overlay toggles keep switching surfaces from inside
    // the modal; `p` self-closes.
    TuiSurfaceArm {
        scope: TuiSurfaceScope::Settings,
        chord: 'p',
        action: TuiSurfaceAction::ToggleSettings,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::Settings,
        chord: 'i',
        action: TuiSurfaceAction::ToggleAbout,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::Settings,
        chord: 'h',
        action: TuiSurfaceAction::ToggleHealth,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::Settings,
        chord: 'c',
        action: TuiSurfaceAction::ToggleContainers,
    },
    // The three status overlays share one toggle protocol.
    TuiSurfaceArm {
        scope: TuiSurfaceScope::StatusOverlay,
        chord: 'i',
        action: TuiSurfaceAction::ToggleAbout,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::StatusOverlay,
        chord: 'h',
        action: TuiSurfaceAction::ToggleHealth,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::StatusOverlay,
        chord: 'c',
        action: TuiSurfaceAction::ToggleContainers,
    },
    // The service-log panel's control chords (its `q`/Esc close stays
    // structural, in `runtime::modals`).
    TuiSurfaceArm {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 'f',
        action: TuiSurfaceAction::ToggleServiceLogFollow,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 'p',
        action: TuiSurfaceAction::ToggleServiceLogPaused,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 'l',
        action: TuiSurfaceAction::CycleServiceLogLevel,
    },
    TuiSurfaceArm {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 't',
        action: TuiSurfaceAction::CycleServiceLogTime,
    },
];

/// Resolve one pressed character against a surface's declared protocol.
/// `None` means the chord is not part of that surface's protocol: a full
/// modal then swallows it as a silent no-op, and the service-log panel lets
/// it fall through to the command layers.
#[must_use]
pub(crate) fn surface_protocol_action(
    scope: TuiSurfaceScope,
    chord: char,
) -> Option<TuiSurfaceAction> {
    TUI_SURFACE_PROTOCOL
        .into_iter()
        .find(|arm| arm.scope == scope && arm.chord == chord)
        .map(|arm| arm.action)
}

// ── Surface-footer hint vocabulary (layer 3 presentation) ─────────────────

/// The footer hint of one declared protocol arm: the painted chord token plus
/// the i18n catalog key and exact spacing of its label. Every entry cites the
/// protocol `(scope, chord, action)` triple it presents, so a painted hint can
/// never name a chord the protocol does not declare (the parity test pins the
/// citation). The vocabulary feeds the shared `KeyHint` component — the same
/// shape [`crate::bindings::MenuHint`] gives the action menus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TuiSurfaceHint {
    pub(crate) scope: TuiSurfaceScope,
    /// The declared protocol chord this hint paints. A structural close key
    /// folded into a token (the overlays' `/ Esc` glyph) stays out of
    /// [`TUI_SURFACE_PROTOCOL`]; only the painted glyph carries it. Read by
    /// the coherence test to re-resolve the cited arm against the protocol
    /// table; the painted footer goes through `token`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) chord: char,
    pub(crate) action: TuiSurfaceAction,
    /// The chord token exactly as painted (padding is part of the glyph).
    pub(crate) token: &'static str,
    /// The shared i18n catalog key of the label text.
    pub(crate) label_key: &'static str,
    /// Spacing painted before / after the resolved label — part of the pinned
    /// footer bytes (the health footer's tail separates the protocol hint from
    /// the shell-layer `T` hint span that follows it).
    pub(crate) label_prefix: &'static str,
    pub(crate) label_suffix: &'static str,
}

impl TuiSurfaceHint {
    /// The rendered `(token, label)` pair shaped for the `KeyHint` component.
    fn pair(self) -> (&'static str, String) {
        (
            self.token,
            format!(
                "{}{}{}",
                self.label_prefix,
                t(self.label_key),
                self.label_suffix
            ),
        )
    }
}

/// The footer-hint vocabulary of the surface-protocol scopes: the About /
/// Health / Containers overlays' close footers and the service-log panel's
/// title control run derive their chord/label pairs from this one table,
/// cross-pinned to [`TUI_SURFACE_PROTOCOL`]. The settings scope deliberately
/// has no entries — the settings form owns its own footer copy, and its
/// `p i h c` protocol arms are not advertised in a footer.
pub(crate) const TUI_SURFACE_HINTS: [TuiSurfaceHint; 7] = [
    // The three status overlays: each paints its own toggle chord, with the
    // structural Esc close folded into the painted token (never declared in
    // the protocol table above).
    TuiSurfaceHint {
        scope: TuiSurfaceScope::StatusOverlay,
        chord: 'i',
        action: TuiSurfaceAction::ToggleAbout,
        token: " i / Esc ",
        label_key: "chrome.close",
        label_prefix: "  ",
        label_suffix: "",
    },
    TuiSurfaceHint {
        scope: TuiSurfaceScope::StatusOverlay,
        chord: 'h',
        action: TuiSurfaceAction::ToggleHealth,
        token: " h / Esc ",
        label_key: "chrome.close",
        label_prefix: "  ",
        // The health footer appends the shell-layer `T` hint span after the
        // label; this tail is the pinned gap between the two.
        label_suffix: "   ",
    },
    TuiSurfaceHint {
        scope: TuiSurfaceScope::StatusOverlay,
        chord: 'c',
        action: TuiSurfaceAction::ToggleContainers,
        token: " c / Esc ",
        label_key: "chrome.close",
        label_prefix: "  ",
        label_suffix: "",
    },
    // The service-log panel's title control run: `chord label · …` over its
    // four declared action chords. The structural "Esc closes" prefix stays
    // with the panel renderer (its own catalog key), and the panel's `q`
    // close stays unpainted, exactly as dispatched.
    TuiSurfaceHint {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 'f',
        action: TuiSurfaceAction::ToggleServiceLogFollow,
        token: "f",
        label_key: "tui.surface.hint_follow",
        label_prefix: " ",
        label_suffix: " · ",
    },
    TuiSurfaceHint {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 'p',
        action: TuiSurfaceAction::ToggleServiceLogPaused,
        token: "p",
        label_key: "tui.surface.hint_pause",
        label_prefix: " ",
        label_suffix: " · ",
    },
    TuiSurfaceHint {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 'l',
        action: TuiSurfaceAction::CycleServiceLogLevel,
        token: "l",
        label_key: "tui.surface.hint_level",
        label_prefix: " ",
        label_suffix: " · ",
    },
    TuiSurfaceHint {
        scope: TuiSurfaceScope::ServiceLogPanel,
        chord: 't',
        action: TuiSurfaceAction::CycleServiceLogTime,
        token: "t",
        label_key: "tui.surface.hint_time",
        label_prefix: " ",
        label_suffix: "",
    },
];

/// The footer hint pairs `scope` paints for `action`'s declared protocol arm,
/// in table order (exactly one pair for every declared entry). An undeclared
/// action yields an empty run — honest absence, unreachable for a live
/// overlay because the parity test pins every painted scope's arms.
#[must_use]
pub(crate) fn surface_hint_pairs(
    scope: TuiSurfaceScope,
    action: TuiSurfaceAction,
) -> Vec<(&'static str, String)> {
    TUI_SURFACE_HINTS
        .into_iter()
        .filter(|hint| hint.scope == scope && hint.action == action)
        .map(TuiSurfaceHint::pair)
        .collect()
}

/// The concatenated `chord label · …` control-hint run of a partial owner's
/// declared protocol arms (the service-log panel's title suffix). The
/// structural close copy stays with the calling surface.
#[must_use]
pub(crate) fn surface_hint_run(scope: TuiSurfaceScope) -> String {
    TUI_SURFACE_HINTS
        .into_iter()
        .filter(|hint| hint.scope == scope)
        .map(|hint| {
            let (token, label) = hint.pair();
            format!("{token}{label}")
        })
        .collect()
}
