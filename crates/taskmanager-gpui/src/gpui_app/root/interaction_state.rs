//! Root-owned interaction vocabulary and per-window text-input initialization.

use gpui::{AppContext, Context, Entity};
use taskmanager_core::core::process::ProcessSignal;
use taskmanager_ui::inputs::text_input::{InputEvent, TextInputState};

use crate::gpui_app::sidebar::SelectedDevice;
use taskmanager_application::i18n;
use taskmanager_core::core::startup::StartupEntryId;

use super::RootView;

/// Single-slot hover tracker. Only the topmost element under the pointer is hovered at
/// any instant, so one slot covers both static chrome (identified by a unique
/// `&'static str` id) and dynamic list rows (sidebar device / process pid / service name).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Hover {
    Static(&'static str),
    Device(SelectedDevice),
    Proc(u32),
    Service(String),
    Startup(StartupEntryId),
    User(String),
}

/// Typed action carried by each process right-click menu item, replacing the magic
/// `u16` ids (0..=8) that `build_proc_menu` and the dispatcher previously had to keep
/// in sync by hand — the two now agree by type. `Signal` carries the exact
/// `ProcessSignal` to send; `EndTask`/`Kill` create a shared confirmation intent,
/// while `Suspend`/`Resume` submit the neutral `ProcessControlRequest`
/// vocabulary in `apply_proc_action` (the adapters own the stop/continue
/// signal mapping, ARCH §8.1);
/// `Properties` opens the process details dialog; `OpenLocation` / `SearchOnline`
/// are the Win11-TM / MC parity non-signal actions (handled in `dispatch.rs`);
/// `CopyName` / `CopyPid` / `CopyCmdline` back the Win11-TM "Copy" submenu and write
/// the respective process field to the system clipboard via gpui's
/// `App::write_to_clipboard` (`ClipboardItem::new_string`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcMenuAction {
    EndTask,
    EndProcessTree,
    Kill,
    Suspend,
    Resume,
    Signal(ProcessSignal),
    OpenLocation,
    SearchOnline,
    /// Copy the process `name` to the clipboard (Win11-TM "Copy > Copy name").
    CopyName,
    /// Copy the process `pid` (decimal) to the clipboard.
    CopyPid,
    /// Copy the full process `cmdline` to the clipboard.
    CopyCmdline,
    Properties,
}

/// Wire an input change into a debounced RootView write. A monotonic sequence
/// makes the latest keystroke authoritative when older timers complete late.
pub(crate) fn wire_debounced_search(
    entity: &Entity<TextInputState>,
    cx: &mut Context<RootView>,
    write: impl Fn(&mut RootView, String) + 'static,
) {
    let seq = std::rc::Rc::new(std::cell::RefCell::new(0u64));
    let root = cx.entity();
    let write = std::rc::Rc::new(write);
    cx.subscribe(entity, move |_rv, state, ev: &InputEvent, cx| {
        if let InputEvent::Change = ev {
            let value = state.read(cx).value().to_string();
            let my = {
                let mut seq = seq.borrow_mut();
                *seq += 1;
                *seq
            };
            let root = root.clone();
            let seq = seq.clone();
            let write = write.clone();
            cx.spawn(async move |_this, cx| {
                gpui::Timer::after(std::time::Duration::from_millis(100)).await;
                if *seq.borrow() == my {
                    let _ = root.update(cx, |rv, cx| {
                        write(rv, value);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
    })
    .detach();
}

/// Lazily create the per-window Apps search input and bind it to the shell query.
pub(super) fn init_search_entity(cx: &mut Context<RootView>) -> Entity<TextInputState> {
    let entity = cx.new(|cx| {
        let mut state = TextInputState::new(cx);
        state.set_placeholder(i18n::t("search.processes"), cx);
        state
    });
    wire_debounced_search(&entity, cx, |rv, value| rv.set_process_query(&value));
    entity
}

/// Lazily create the per-window Run-dialog command input.
pub(super) fn init_run_entity(cx: &mut Context<RootView>) -> Entity<TextInputState> {
    cx.new(|cx| {
        let mut state = TextInputState::new(cx);
        state.set_placeholder(i18n::t("search.run_command"), cx);
        state
    })
}

impl RootView {
    /// Read the command from its sole input-entity authority.
    #[must_use]
    pub fn run_command_text(&self, cx: &gpui::App) -> String {
        self.run_input
            .as_ref()
            .map(|input| input.read(cx).display_text().to_owned())
            .unwrap_or_default()
    }
}
