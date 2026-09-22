//! Real-input seam: Bevy keyboard events into the shared shell routers.
//!
//! This module is the ONLY place raw Bevy input reaches the shell, and it
//! adds no semantics of its own: every press is forwarded through
//! [`ShellApp::handle_local_char`] / [`ShellApp::handle_local_key`] — the
//! same two entry points the TUI drives — so gate precedence, search
//! ownership, selection movement, overlay dismissal and the shared command
//! table stay single-sourced in the shell (ARCH §8.1 semantic-parity law).
//!
//! The adapter owns exactly five frontend-local facts:
//!
//! 1. **Route authority**: Alt+1..8 / bare `P` switch this frontend's own
//!    route ([`crate::app::Page`]; the shared `AppPage` vocabulary has no
//!    Processes / Settings / Alerts page shape). The same page action is
//!    applied to the shell so `CommandScope` derivation in `dispatch_key`
//!    follows the visible page.
//! 2. **Dialog-scope Enter**: the shared command table binds Enter to
//!    `ConfirmEndTask` under `CommandScope::Dialog`, which the shell's
//!    `dispatch_key` never derives, so an armed gate receives it here.
//! 3. **Action-menu chords**: the per-inventory action menus
//!    ([`crate::menu_modal`]) are frontend-local surfaces, so their open
//!    attempts resolve here ahead of the shell's free bindings — bare Enter
//!    over a selected inventory row, and the TUI's `a` on Applications. Bare
//!    Enter stays out of the Applications arm: the shell owns it there (tree
//!    expansion, next search match).
//! 4. **Effect bridge**: platform effects returned by the shell cross to the
//!    platform client through [`PendingEffects`], drained by the `PreUpdate`
//!    drain system — the one place that holds the client lock (charter
//!    boundary 4).
//! 5. **Re-render signal**: any shell mutation triggers
//!    [`ShellInteractionApplied`] so mounted pages rebuild from the folded
//!    state (never polling).
//!
//! The keyboard adapter itself lives in the [`dispatch`] submodule: it owns
//! the per-press frame and the ordered arm chain, where each arm is one
//! small method whose doc comment carries the precedence number from the
//! parity contract.

use bevy::app::{App, Plugin};
use bevy::ecs::event::Event;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut, SystemParam};
use bevy::input::ButtonInput;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use taskmanager_application::{Modifiers, PlatformEffect};

use taskmanager_shell::ShellApp;

use crate::app::{Page, modifier_state};
use crate::menu_modal::{MenuModal, ModalDriver};
use crate::pages::processes::menu::ProcessMenuCtx;
use crate::pages::services::menu::ServiceMenuCtx;
use crate::pages::sessions::menu::SessionMenuCtx;
use crate::pages::startup::menu::StartupMenuCtx;

#[path = "input/dispatch.rs"]
mod dispatch;
#[path = "input/text_input.rs"]
mod text_input;

pub(crate) use dispatch::keyboard_dispatch_system;
pub(crate) use text_input::TextInputState;

/// Platform effects produced by shell state transitions on the input path.
/// The drain system submits them through the shared `queue_effect` seam.
#[derive(Resource, Default)]
pub(crate) struct PendingEffects(pub(crate) Vec<PlatformEffect>);

/// Triggered once per frame in which a key press mutated the shell state.
/// Page observers rebuild from the (already updated) projection.
#[derive(Event)]
pub(crate) struct ShellInteractionApplied;

/// Forwards the shell's quit decision to the runner exactly once. The TUI
/// polls `quit_reason` in its loop; the Bevy adapter translates the first
/// observation into [`bevy::app::AppExit`].
#[derive(Resource, Default)]
pub(crate) struct QuitForwarded(pub(crate) bool);

pub(crate) fn commit_query_to_shell(shell: &mut ShellApp, text: &str) {
    while !shell.query.is_empty() {
        shell.pop_search_char();
    }
    if !text.is_empty() {
        shell.push_search_text(text);
    }
}

/// The input plugin: resources only. The keyboard adapter system itself is
/// registered once by [`crate::app::AppShellPlugin`], chained before the page
/// mount system — registering it here too would create a second system
/// instance with its own message cursor, dispatching every key twice.
pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingEffects>()
            .init_resource::<QuitForwarded>()
            .init_resource::<crate::drain::FeedbackCache>()
            .init_resource::<crate::pages::services::log_panel::ServiceLogExportDir>()
            .init_resource::<TextInputState>();
    }
}

/// Which surface owns the keyboard this instant. Mirrors the shell's
/// modal precedence (gate > help > suggestions > search > free) plus this
/// frontend's own local modals (the per-inventory action menus), which own
/// the keyboard ahead of the shell's free bindings — a frontend-local modal
/// can never have its keys stolen by navigation chords.
#[derive(Clone, Copy)]
enum KeyboardOwner {
    Gate,
    FrontendMenu,
    ServiceLogPanel,
    SharedSurface,
    Search,
    Free,
}

/// One per-inventory action menu, as a typed surface. A menu is identified by
/// its kind, never by a presence flag, so a captured snapshot can be matched
/// against the resource it belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FrontendMenuKind {
    Service,
    Startup,
    Session,
    Process,
}

/// The frontend-local action menus that owned the keyboard when a press
/// landed, in drive order (Services, Startup, Sessions, Process).
///
/// The four menus are independent resources, so the snapshot keeps one
/// presence slot per resource: `Some(kind)` is the typed surface that was
/// present, `None` an absent one. A press captured before the first arm runs
/// therefore keeps every surface it belonged to, and no later mutation can
/// change the decision.
#[derive(Clone, Copy)]
struct FrontendMenus([Option<FrontendMenuKind>; 4]);

impl FrontendMenus {
    /// Whether no frontend-local surface owned the keyboard.
    fn is_empty(self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    /// Whether `kind`'s surface owned the keyboard.
    fn holds(self, kind: FrontendMenuKind) -> bool {
        self.0.contains(&Some(kind))
    }
}

/// Bundled action-menu modal resources to keep the dispatch system under the argument budget.
#[derive(SystemParam)]
pub(crate) struct InventoryActionModals<'w> {
    pub(crate) svc: ResMut<'w, MenuModal<ServiceMenuCtx>>,
    pub(crate) stu: ResMut<'w, MenuModal<StartupMenuCtx>>,
    pub(crate) ses: ResMut<'w, MenuModal<SessionMenuCtx>>,
    pub(crate) proc: ResMut<'w, MenuModal<ProcessMenuCtx>>,
}

impl InventoryActionModals<'_> {
    /// The frontend-local action menus present this instant, in drive order.
    fn present(&self) -> FrontendMenus {
        FrontendMenus([
            self.svc.is_open().then_some(FrontendMenuKind::Service),
            self.stu.is_open().then_some(FrontendMenuKind::Startup),
            self.ses.is_open().then_some(FrontendMenuKind::Session),
            self.proc.is_open().then_some(FrontendMenuKind::Process),
        ])
    }
}

/// Bundled table selections for the inventory pages.
#[derive(SystemParam)]
pub(crate) struct InventorySelections<'w> {
    pub(crate) svc: Option<Res<'w, crate::pages::services::ServiceSelection>>,
    pub(crate) stu: Option<Res<'w, crate::pages::startup::StartupSelection>>,
    pub(crate) ses: Option<Res<'w, crate::pages::sessions::SessionSelection>>,
}

fn keyboard_owner(shell: &ShellApp, frontend_menus: FrontendMenus, page: Page) -> KeyboardOwner {
    if shell.confirmation_kind().is_some() {
        KeyboardOwner::Gate
    } else if !frontend_menus.is_empty() {
        KeyboardOwner::FrontendMenu
    } else if shell.service_log.is_some() && page == Page::Services {
        KeyboardOwner::ServiceLogPanel
    } else if shell.help_open() || shell.suggestions_open() {
        KeyboardOwner::SharedSurface
    } else if shell.search_active() {
        KeyboardOwner::Search
    } else {
        KeyboardOwner::Free
    }
}

fn modifiers_from(keys: &ButtonInput<KeyCode>) -> Modifiers {
    let state = modifier_state(keys);
    Modifiers::new(state.control, state.alt, state.shift, state.platform)
}

/// The layout-correct character of one press, when the event carries plain
/// text and no chord modifier owns it. Shift is allowed — the produced text
/// already encodes it.
fn text_char(event: &KeyboardInput, modifiers: Modifiers) -> Option<char> {
    if modifiers.control || modifiers.alt || modifiers.platform {
        return None;
    }
    let text = event.text.as_deref()?;
    let mut chars = text.chars();
    let only = chars.next()?;
    (chars.next().is_none() && !only.is_control()).then_some(only)
}

#[cfg(test)]
#[path = "../tests/headless/input.rs"]
mod tests;
