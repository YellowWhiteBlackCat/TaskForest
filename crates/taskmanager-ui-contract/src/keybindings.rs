//! Frontend command-binding coverage contract (the anti-drift matrix).
//!
//! The key chords themselves stay frontend dialect — each shape decides its
//! own display tokens (`"Ctrl+F"`, `"F9"`, `"1-7"`, …) and the contract
//! never parses them. What is shared is the coverage decision: for every
//! command the application knows ([`CommandId::ALL`]), every frontend must
//! explicitly declare the key token it wires or a deliberate
//! [`Binding::Unbound`] — a silent omission is drift, not a choice.
//! [`coverage_report`] folds one declaration into that per-command matrix
//! so each frontend can gate on it.

use taskmanager_application::CommandId;

/// Which product frontend shape a binding declaration describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrontendShape {
    /// The GPUI desktop frontend.
    Gpui,
    /// The Iced desktop frontend.
    Iced,
    /// The Ratatui terminal frontend.
    Tui,
    /// The Bevy desktop frontend.
    Bevy,
}

impl FrontendShape {
    /// Every frontend shape governed by the shared contract.
    pub const ALL: [Self; 4] = [Self::Gpui, Self::Iced, Self::Tui, Self::Bevy];

    /// Stable report-friendly name for gates and diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gpui => "gpui",
            Self::Iced => "iced",
            Self::Tui => "tui",
            Self::Bevy => "bevy",
        }
    }
}

/// One command's binding decision inside a frontend shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// A frontend-dialect key token this shape genuinely wires for the
    /// command.
    Key(&'static str),
    /// Deliberately not offered in this shape (for example the sidebar
    /// toggle in a terminal that has no sidebar surface).
    Unbound,
}

impl Binding {
    /// The key token when bound, `None` when deliberately unbound.
    #[must_use]
    pub const fn key_token(self) -> Option<&'static str> {
        match self {
            Self::Key(token) => Some(token),
            Self::Unbound => None,
        }
    }

    /// Whether this is a wired key binding.
    #[must_use]
    pub const fn is_bound(self) -> bool {
        matches!(self, Self::Key(_))
    }
}

/// One declared command-to-binding pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingEntry {
    pub command: CommandId,
    pub binding: Binding,
}

impl BindingEntry {
    /// A wired entry carrying the frontend's key token.
    #[must_use]
    pub const fn bound(command: CommandId, token: &'static str) -> Self {
        Self {
            command,
            binding: Binding::Key(token),
        }
    }

    /// An explicit not-offered-in-this-shape entry.
    #[must_use]
    pub const fn unbound(command: CommandId) -> Self {
        Self {
            command,
            binding: Binding::Unbound,
        }
    }
}

/// A frontend's explicit declaration of its command binding surface: one
/// entry per contract-known command, either bound or deliberately unbound.
#[derive(Clone, Debug)]
pub struct FrontendBindingDeclaration {
    pub frontend: FrontendShape,
    pub entries: Vec<BindingEntry>,
}

/// The coverage outcome for one command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageStatus {
    /// Wired in this shape with the declared key token.
    Bound(&'static str),
    /// Explicitly declared not offered in this shape.
    DeliberatelyUnbound,
    /// Contract-known but absent from the declaration — a silent omission.
    Missing,
    /// Declared more than once by the same frontend.
    Duplicated,
    /// Declared but outside the known command set.
    Unknown,
}

impl CoverageStatus {
    /// Whether the command carries an explicit decision — the only statuses
    /// a no-drift declaration may show.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Bound(_) | Self::DeliberatelyUnbound)
    }

    /// Whether this status is a drift finding a gate must reject.
    #[must_use]
    pub const fn is_drift(self) -> bool {
        !self.is_explicit()
    }
}

/// The coverage matrix for one declaration against the full contract command
/// set, in canonical [`CommandId::ALL`] order; declared commands outside the
/// known set follow in declaration order as [`CoverageStatus::Unknown`].
#[must_use]
pub fn coverage_report(
    declaration: &FrontendBindingDeclaration,
) -> Vec<(CommandId, CoverageStatus)> {
    coverage_report_over(declaration, &CommandId::ALL)
}

/// The drift findings alone — the one-call gate payload.
#[must_use]
pub fn drift_findings(report: &[(CommandId, CoverageStatus)]) -> Vec<(CommandId, CoverageStatus)> {
    report
        .iter()
        .copied()
        .filter(|(_, status)| status.is_drift())
        .collect()
}

/// The matrix against a restricted known set. The restricted seam is what
/// makes the unknown-command path real: a declared command outside `known`
/// is reported, never silently dropped.
fn coverage_report_over(
    declaration: &FrontendBindingDeclaration,
    known: &[CommandId],
) -> Vec<(CommandId, CoverageStatus)> {
    let mut report: Vec<(CommandId, CoverageStatus)> = known
        .iter()
        .map(|command| (*command, CoverageStatus::Missing))
        .collect();
    for entry in &declaration.entries {
        match report
            .iter_mut()
            .find(|(command, _)| *command == entry.command)
        {
            Some((_, status)) => {
                if status.is_explicit() || matches!(status, CoverageStatus::Duplicated) {
                    *status = CoverageStatus::Duplicated;
                } else {
                    *status = match entry.binding {
                        Binding::Key(token) => CoverageStatus::Bound(token),
                        Binding::Unbound => CoverageStatus::DeliberatelyUnbound,
                    };
                }
            }
            None => report.push((entry.command, CoverageStatus::Unknown)),
        }
    }
    report
}

#[cfg(test)]
#[path = "../tests/headless/ui_keybindings.rs"]
mod tests;
