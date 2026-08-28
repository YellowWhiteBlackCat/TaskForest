//! Session-row ViewModel (ARCH.md §8.1 data layer): folds one
//! [`SessionItem`] into its pre-formatted cell strings exactly once per data
//! swap, so [`super::UsersDelegate::render_td`] only styles and paints. The
//! shared em-dash placeholder
//! ([`crate::gpui_app::formatting::missing_value`]) is folded in here for a
//! missing seat / tty / logon; the remote cell's COLOR stays a render-layer
//! decision keyed on the source row's typed `remote` bool. No theme, gpui, or
//! layout types in this module — pure data, unit-tested without a window.

use crate::gpui_app::formatting::missing_value;
use crate::i18n;
use taskmanager_application::SessionItem;

/// Pre-folded display strings for one sessions-table row (columns: Session /
/// User / Seat / TTY / Remote / Logon).
pub struct UserRowVm {
    /// Session id (column 0).
    pub session: String,
    /// User name (column 1); the render layer applies search highlighting.
    pub user: String,
    /// Seat label, or the shared dash when the platform reported none.
    pub seat: String,
    /// TTY label, or the shared dash when the platform reported none.
    pub tty: String,
    /// Localized yes/no label; the renderer picks the color from the source
    /// row's `remote` bool (color is not data).
    pub remote_label: &'static str,
    /// Logon timestamp, or the shared dash when the platform reported none.
    pub logon: String,
}

/// Fold one session row into its display strings, mirroring the exact
/// formatter/dash conventions the inline render path used (`—` for `None`,
/// localized yes/no for `remote`).
pub fn user_row_vm(row: &SessionItem) -> UserRowVm {
    UserRowVm {
        session: row.id.clone(),
        user: row.user.clone(),
        seat: row.seat.clone().unwrap_or_else(missing_value),
        tty: row.tty.clone().unwrap_or_else(missing_value),
        remote_label: if row.remote {
            i18n::t("common.yes")
        } else {
            i18n::t("common.no")
        },
        logon: row.timestamp.clone().unwrap_or_else(missing_value),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_users_view_row_vm_tests.rs"]
mod tests;
