//! Session-row ViewModel (ARCH.md §8.1 data layer): folds one
//! [`SessionItem`] into its pre-formatted cell strings exactly once per data
//! swap, so [`super::UsersDelegate::render_td`] only styles and paints. The
//! shared em-dash placeholder
//! ([`crate::gpui_app::formatting::missing_value`]) is folded in here for a
//! missing seat / tty / logon; the remote-family accent COLOR stays a
//! render-layer decision keyed on the canonical family fact. No theme, gpui, or
//! layout types in this module — pure data, unit-tested without a window.

use crate::gpui_app::formatting::missing_value;
use taskforest_core::core::session::{SessionFamily, SessionItem};
use taskmanager_application::i18n;

/// Pre-folded display strings for one sessions-table row (columns: Session /
/// User / Seat / TTY / Family / Logon).
pub struct UserRowVm {
    /// Session id (column 0).
    pub session: String,
    /// User name (column 1); the render layer applies search highlighting.
    pub user: String,
    /// Seat label, or the shared dash when the platform reported none.
    pub seat: String,
    /// TTY label, or the shared dash when the platform reported none.
    pub tty: String,
    /// Localized family and availability label from the canonical session fact.
    pub family_label: &'static str,
    /// A remote family gets the existing network-accent text treatment.
    pub family_is_remote: bool,
    /// Logon timestamp, or the shared dash when the platform reported none.
    pub logon: String,
}

/// Fold one session row into its display strings. Missing seat/TTY/logon use
/// the shared dash; family labels preserve the contract's observed,
/// unobserved, and unavailable states.
pub fn user_row_vm(row: &SessionItem) -> UserRowVm {
    UserRowVm {
        session: row.id.to_string(),
        user: row.user.clone(),
        seat: row.seat.clone().unwrap_or_else(missing_value),
        tty: row.tty.clone().unwrap_or_else(missing_value),
        family_label: i18n::session_family_label(row.family),
        family_is_remote: row.family.family() == Some(SessionFamily::Remote),
        logon: row.timestamp.clone().unwrap_or_else(missing_value),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_users_view_row_vm_tests.rs"]
mod tests;
