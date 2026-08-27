//! Localized inventory headings kept separate from the table row/effect code.

use taskmanager_application::i18n::t;

use super::ListState;

pub(crate) fn service_heading(state: ListState, row_count: usize) -> String {
    match state {
        ListState::Loading => format!("{} · {}", t("tab.services"), t("common.waiting_inventory")),
        ListState::Empty => format!("{} · 0 {}", t("tab.services"), t("common.reported")),
        ListState::Ready => format!(
            "{} · {row_count} {}",
            t("tab.services"),
            t("common.reported")
        ),
    }
}
