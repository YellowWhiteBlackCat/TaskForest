//! Session-kind classification: the terminal family that owns a login session.
//!
//! The provider reports three observable facts per session — the `seat` it
//! belongs to, the `tty` that carries it, and the `remote` flag. The product
//! needs ONE visual family per row so the user can tell a graphical display
//! session from a console terminal and from a session reached over the
//! network. The classifier reads only those observed fields: an unobserved
//! family is `Unknown`, never a guess.
//!
//! Glyph authority: every mark comes from the shared semantic icon registry
//! through the crate's icon layer (`crate::icons`), so a session kind can
//! never smuggle a font codepoint into the row. Every family wears its own
//! dedicated vector glyph: a display-server session the monitor
//! (`IconId::Display`), a console session the terminal (`IconId::Terminal`),
//! a remote session the globe (`IconId::Remote`).

use bevy::ecs::component::Component;
use taskforest_core::core::session::SessionItem;
use taskmanager_ui_contract::IconId;

/// The terminal family a login session belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SessionKind {
    /// A display-server session (a Wayland/X11 compositor owns the seat).
    Graphical,
    /// A console session (a virtual or physical terminal owns it).
    Console,
    /// A session reached from another host.
    Remote,
    /// Nothing observed identifies the family.
    #[default]
    Unknown,
}

/// Marker on the cell that carries one session family's mark. The marker is
/// the row's typed record of WHICH family the mark stands for — the icon
/// plate itself only says "some semantic icon".
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionKindMarker(pub(crate) SessionKind);

impl SessionKind {
    /// The semantic icon plate that marks this family in the session list.
    ///
    /// The mapping stays inside the shared registry, and every family wears
    /// its own vector glyph: a display-server session is a monitor, a console
    /// session is a terminal, a remote session is the globe. An unclassified
    /// session carries the neutral information plate — an unobserved family is
    /// never drawn as a terminal.
    pub(crate) fn icon(self) -> IconId {
        match self {
            SessionKind::Graphical => IconId::Display,
            SessionKind::Console => IconId::Terminal,
            SessionKind::Remote => IconId::Remote,
            SessionKind::Unknown => IconId::System,
        }
    }
}

/// Classify one observed session into its terminal family.
pub(crate) fn classify(session: &SessionItem) -> SessionKind {
    if session.remote {
        // A remote session is reached over the network whatever terminal
        // carries it — the transport is the stronger fact.
        return SessionKind::Remote;
    }
    if let Some(tty) = session.tty.as_deref() {
        let tty = tty.trim();
        if is_display_tty(tty) {
            return SessionKind::Graphical;
        }
        if is_terminal_tty(tty) {
            return SessionKind::Console;
        }
    }
    if session.seat.is_some() {
        // A seat without a terminal belongs to a display session.
        return SessionKind::Graphical;
    }
    SessionKind::Unknown
}

/// A compositor-owned terminal: an X11 display (`:0`) or a Wayland seat
/// (`wayland-0`).
fn is_display_tty(tty: &str) -> bool {
    tty.starts_with(':') || tty.contains("wayland")
}

/// A terminal-owned line: a pseudo terminal (`pts/3`, `/dev/pts/3`) or a
/// physical console (`tty1`, `/dev/tty1`).
fn is_terminal_tty(tty: &str) -> bool {
    tty.starts_with("pts")
        || tty.contains("/dev/pts/")
        || tty.starts_with("tty")
        || tty.contains("/dev/tty")
}
