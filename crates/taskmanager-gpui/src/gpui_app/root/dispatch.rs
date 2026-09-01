//! Context menu builder + initial-state helpers (env-var overrides for the
//! starting tab and selected device).
//!
//! These are pure functions that take all needed state as parameters — no `self`
//! on `RootView`, so they live here instead of in the main `root.rs` impl blocks.

use super::{ProcMenuAction, RootView, TopPage};
use gpui::Entity;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::presentation::search_url_for;
use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem};

use crate::gpui_app::sidebar::SelectedDevice;
#[cfg(target_os = "linux")]
use taskmanager_core::core::process::ProcessSignal;

/// Initial top-level page, overridable via `TM_PAGE` (values: `performance`, `apps`/
/// `processes`, `services`, `system`, `startup`, `users`, `app-history`). Useful for
/// launching to a tab and for screenshotting non-default pages. Defaults to Performance.
pub fn initial_page() -> TopPage {
    match std::env::var("TM_PAGE").ok().as_deref() {
        Some("apps") | Some("processes") => TopPage::Apps,
        Some("services") => TopPage::Services,
        Some("system") => TopPage::System,
        Some("startup") => TopPage::Startup,
        Some("users") => TopPage::Users,
        Some("app-history") => TopPage::AppHistory,
        Some("containers") => TopPage::Containers,
        _ => TopPage::Performance,
    }
}

/// Initial selected device on the Performance page, overridable via `TM_DEVICE`
/// (values: `cpu`, `memory`, `disk`, `nic`/`network`, `gpu`,
/// `battery`, `fan`). Defaults to CPU.
/// Useful for deep-linking to a device detail and for screenshotting it.
pub fn initial_selected() -> SelectedDevice {
    match std::env::var("TM_DEVICE").ok().as_deref() {
        Some("memory") => SelectedDevice::Memory,
        Some("disk") => SelectedDevice::Disk(0),
        Some("nic") | Some("network") => SelectedDevice::Nic(0),
        Some("gpu") => SelectedDevice::Gpu(0),
        Some("battery") | Some("power") => SelectedDevice::Battery(0),
        Some("fan") | Some("fans") => SelectedDevice::Fan(0),
        _ => SelectedDevice::Cpu,
    }
}

/// Build the right-click process menu entries (End task / Kill / Suspend /
/// Resume + 4 signals + Open file location / Search online / Properties + the
/// Win11-TM "Copy" group). Each item carries a typed [`ProcMenuAction`] in its
/// action closure, dispatching through `super::RootView::apply_proc_action` on
/// the given RootView entity. The typed action replaces the old magic `u16` ids,
/// so this builder and `apply_proc_action` agree by type (no `0..=8` arms to
/// keep in sync).
///
/// The `PopupMenuState` is assembled by the caller (the `context_menu` builder
/// in `processes_view::rows::cells.rs`) from these entries; the caller passes
/// its frozen live identity into every action closure and also synchronizes
/// visual selection before the popup opens.
///
/// The gc "Copy" submenu has no submenu variant in the own popup layer
/// (compile-time exclusion in the owned component boundary), so its three
/// items are flattened under a non-interactive label row; the items, i18n
/// labels, and `CopyName`/`CopyPid`/`CopyCmdline` dispatch are unchanged.
pub fn build_proc_menu(entity: Entity<RootView>, identity: ProcessLiveKey) -> Vec<MenuEntry> {
    fn item(
        items: &mut Vec<MenuEntry>,
        entity: &Entity<RootView>,
        identity: ProcessLiveKey,
        label: &'static str,
        action: ProcMenuAction,
    ) {
        let e = entity.clone();
        items.push(MenuEntry::Item(MenuItem::new(label, move |_, cx| {
            e.update(cx, |v, cx| v.apply_proc_action(identity, action, cx));
        })));
    }

    let mut items = Vec::new();
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.end_task"),
        ProcMenuAction::EndTask,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.end_process_tree"),
        ProcMenuAction::EndProcessTree,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.kill"),
        ProcMenuAction::Kill,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.suspend"),
        ProcMenuAction::Suspend,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.resume"),
        ProcMenuAction::Resume,
    );
    #[cfg(target_os = "linux")]
    {
        items.push(MenuEntry::Separator);
        item(
            &mut items,
            &entity,
            identity,
            "Send SIGHUP",
            ProcMenuAction::Signal(ProcessSignal::Hangup),
        );
        item(
            &mut items,
            &entity,
            identity,
            "Send SIGINT",
            ProcMenuAction::Signal(ProcessSignal::Interrupt),
        );
        item(
            &mut items,
            &entity,
            identity,
            "Send SIGUSR1",
            ProcMenuAction::Signal(ProcessSignal::User1),
        );
        item(
            &mut items,
            &entity,
            identity,
            "Send SIGUSR2",
            ProcMenuAction::Signal(ProcessSignal::User2),
        );
    }
    items.push(MenuEntry::Separator);
    // Win11 TM / MC parity: Open file location + Search online, placed adjacent to
    // Properties. Both are non-signal actions dispatched through apply_proc_action →
    // apply_open_location / apply_search_online below.
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.open_location"),
        ProcMenuAction::OpenLocation,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("proc.search_online"),
        ProcMenuAction::SearchOnline,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("dialog.properties"),
        ProcMenuAction::Properties,
    );
    items.push(MenuEntry::Separator);
    // Win11-TM "Copy" group: the own popup layer excludes submenus at compile
    // time for P4, so the submenu is flattened into a label + three items.
    items.push(MenuEntry::Label(
        taskmanager_application::i18n::t("common.copy").into(),
    ));
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("menu.copy_name"),
        ProcMenuAction::CopyName,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("menu.copy_pid"),
        ProcMenuAction::CopyPid,
    );
    item(
        &mut items,
        &entity,
        identity,
        taskmanager_application::i18n::t("menu.copy_command_line"),
        ProcMenuAction::CopyCmdline,
    );
    items
}

/// "Search online" action body: open a Google search for the selected process's name
/// in the default browser via `xdg-open`. The name is percent-encoded per RFC 3986
/// (`taskmanager_shell::presentation::url_encode_query`) so spaces / `/` / `&` /
/// non-ASCII don't corrupt the URL.
/// Sets a transient feedback when there's no name to search or the launch fails.
pub fn apply_search_online(
    v: &mut RootView,
    identity: ProcessLiveKey,
    cx: &mut gpui::Context<RootView>,
) {
    let Some(name) = v
        .processes()
        .iter()
        .find(|process| ProcessLiveKey::from_process(process) == Some(identity))
        .map(|process| process.name.clone())
    else {
        return;
    };
    if name.trim().is_empty() {
        v.show_local_feedback("No process name to search", cx);
        return;
    }
    v.request_open_url(search_url_for(&name), cx);
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_dispatch_tests.rs"]
mod tests;
