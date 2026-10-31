//! The session-family plate: which mark one login session wears.
//!
//! The classification itself is a contract fact: the provider observes the
//! session's identifying facts and reports a typed
//! [`SessionFamilyFact`](taskforest_core::core::session::SessionFamilyFact)
//! through the one rule core owns. This module only decides which mark a
//! reported family wears, so no frontend re-derives a family from raw fields
//! and none can borrow a neighbouring plate: an unobserved or unavailable
//! family wears the neutral plate, never a real family's glyph.
//!
//! Glyph authority: every mark comes from the shared semantic icon registry
//! through the crate's icon layer (`crate::icons`), so a session kind can
//! never smuggle a font codepoint into the row. Every family wears its own
//! dedicated vector glyph: a display-server session the monitor
//! (`IconId::Display`), a console session the terminal (`IconId::Terminal`),
//! a remote session the globe (`IconId::Remote`).

use bevy::ecs::component::Component;
use taskforest_core::core::session::{SessionFamily, SessionFamilyFact};
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

    /// The mark for one reported session family. The contract decides whether a
    /// family was observed at all; this frontend only picks the glyph.
    pub(crate) fn from_fact(fact: SessionFamilyFact) -> Self {
        match fact.family() {
            Some(SessionFamily::Graphical) => Self::Graphical,
            Some(SessionFamily::Console) => Self::Console,
            Some(SessionFamily::Remote) => Self::Remote,
            Some(SessionFamily::Unknown) | None => Self::Unknown,
        }
    }
}
