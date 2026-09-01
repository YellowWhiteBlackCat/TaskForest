//! Real-input seam (W4): Bevy keyboard events into the shared shell routers.
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

use bevy::app::{App, AppExit, Plugin};
use bevy::ecs::event::Event;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use taskmanager_application::{CommandContext, CommandScope, Modifiers, PlatformEffect};

use taskmanager_shell::{InputDispatch, ShellApp, ShellKeyEvent};

use crate::app::{
    FrontendTrack, Page, Route, action_for_page, modifier_state, request_route, route_key_press,
};
use crate::confirmation::{ConfirmationChanged, PendingConfirmationView};
use crate::input_contract::{InputModifiers, normalize_key, shared_key};
use crate::menu_modal::{MenuModal, MenuModalChanged, ModalDriver};
use crate::pages::processes::menu::ProcessMenuCtx;
use crate::pages::services::menu::ServiceMenuCtx;
use crate::pages::sessions::menu::SessionMenuCtx;
use crate::pages::startup::menu::StartupMenuCtx;

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
/// observation into [`AppExit`].
#[derive(Resource, Default)]
pub(crate) struct QuitForwarded(pub(crate) bool);

/// The input plugin: resources only. The keyboard adapter system itself is
/// registered once by [`crate::app::AppShellPlugin`], chained before the page
/// mount system — registering it here too would create a second system
/// instance with its own message cursor, dispatching every key twice.
pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingEffects>()
            .init_resource::<QuitForwarded>();
    }
}

/// Which surface owns the keyboard this instant. Mirrors the shell's
/// modal precedence (gate > help > suggestions > search > free) plus this
/// frontend's own local modals (the per-inventory action menus), which own
/// the keyboard ahead of the shell's free bindings — a frontend-local modal
/// can never have its keys stolen by navigation chords.
enum KeyboardOwner {
    Gate,
    FrontendMenu,
    ServiceLogPanel,
    SharedSurface,
    Search,
    Free,
}

fn keyboard_owner(shell: &ShellApp, modal_open: bool, page: Page) -> KeyboardOwner {
    if shell.confirmation_kind().is_some() {
        KeyboardOwner::Gate
    } else if modal_open {
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

/// The `Update` keyboard adapter: normalize every just-pressed Bevy key and
/// forward it through the shell's routers. One pass, in modal-precedence
/// order; effects and re-render signals collect for the frame tail.
#[allow(clippy::too_many_arguments)]
pub(crate) fn keyboard_dispatch_system(
    mut presses: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut track: NonSendMut<FrontendTrack>,
    mut svc_modal: ResMut<MenuModal<ServiceMenuCtx>>,
    mut stu_modal: ResMut<MenuModal<StartupMenuCtx>>,
    mut ses_modal: ResMut<MenuModal<SessionMenuCtx>>,
    mut proc_modal: ResMut<MenuModal<ProcessMenuCtx>>,
    svc_selection: Option<Res<crate::pages::services::ServiceSelection>>,
    stu_selection: Option<Res<crate::pages::startup::StartupSelection>>,
    ses_selection: Option<Res<crate::pages::sessions::SessionSelection>>,
    mut pending: ResMut<PendingEffects>,
    mut route: ResMut<Route>,
    mut quit: ResMut<QuitForwarded>,
    mut exits: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let events: Vec<KeyboardInput> = presses
        .read()
        .filter(|event| event.state == ButtonState::Pressed)
        .cloned()
        .collect();
    let modifiers = modifiers_from(&keys);
    let shell = &mut track.shell;
    let armed_before = shell.confirmation_kind();
    let mut applied = false;
    for event in &events {
        let modal_open = svc_modal.is_open()
            || stu_modal.is_open()
            || ses_modal.is_open()
            || proc_modal.is_open();
        let context = keyboard_owner(shell, modal_open, route.page);
        // 0a. Open action menus (frontend-local modals): an open modal owns
        //     the keyboard ahead of navigation chords and the shell; its
        //     overlay mounts/despawns with the session transition.
        if modal_open {
            let svc_before = svc_modal.is_open();
            let stu_before = stu_modal.is_open();
            let ses_before = ses_modal.is_open();
            let proc_before = proc_modal.is_open();
            if svc_before {
                applied |= svc_modal.drive(shell, event.key_code, &mut pending.0);
            }
            if stu_before {
                applied |= stu_modal.drive(shell, event.key_code, &mut pending.0);
            }
            if ses_before {
                applied |= ses_modal.drive(shell, event.key_code, &mut pending.0);
            }
            if proc_before {
                applied |= proc_modal.drive(shell, event.key_code, &mut pending.0);
            }
            if svc_before && !svc_modal.is_open() {
                commands.trigger(MenuModalChanged::<ServiceMenuCtx>(
                    false,
                    Default::default(),
                ));
            }
            if stu_before && !stu_modal.is_open() {
                commands.trigger(MenuModalChanged::<StartupMenuCtx>(
                    false,
                    Default::default(),
                ));
            }
            if ses_before && !ses_modal.is_open() {
                commands.trigger(MenuModalChanged::<SessionMenuCtx>(
                    false,
                    Default::default(),
                ));
            }
            if proc_before && !proc_modal.is_open() {
                commands.trigger(MenuModalChanged::<ProcessMenuCtx>(
                    false,
                    Default::default(),
                ));
            }
            continue;
        }
        // 0b. Service log panel (frontend-local surface, TUI panel parity):
        //     F/P/L/T/Esc are consumed ahead of navigation chords and the
        //     shell routers while the panel owns the Services page keyboard.
        if matches!(context, KeyboardOwner::ServiceLogPanel)
            && modifiers == Modifiers::NONE
            && let Some(action) = crate::pages::services::log_panel::log_panel_key(event.key_code)
        {
            use crate::pages::services::log_panel::ServiceLogControlAction;
            match action {
                ServiceLogControlAction::ToggleFollow => shell.toggle_service_log_follow(),
                ServiceLogControlAction::TogglePaused => shell.toggle_service_log_paused(),
                ServiceLogControlAction::CycleLevel => shell.cycle_service_log_level(),
                ServiceLogControlAction::CycleTime => shell.cycle_service_log_time(),
                ServiceLogControlAction::Close => shell.close_service_log(),
            }
            applied = true;
            commands.trigger(crate::pages::services::log_panel::LogPanelRepaintRequired);
            continue;
        }
        // 1. Frontend navigation: route chords move the Bevy route AND the
        //    shell page so scope derivation follows the visible page.
        if matches!(context, KeyboardOwner::Free)
            && let Some(page) = route_key_press(event.key_code, modifier_state(&keys))
        {
            if let Some(action) = action_for_page(page)
                && let Some(effect) = shell.apply_action(action)
            {
                pending.0.push(effect);
            }
            request_route(&mut route, page, &mut commands);
            applied = true;
            continue;
        }
        // 2. Dialog-scope Enter: the shared table's confirmation binding.
        if matches!(context, KeyboardOwner::Gate)
            && event.key_code == KeyCode::Enter
            && modifiers == Modifiers::NONE
            && let Some(action) = normalize_key(
                KeyCode::Enter,
                InputModifiers::default(),
                CommandContext {
                    scope: CommandScope::Dialog,
                    overlay_present: true,
                    ..CommandContext::default()
                },
            )
            && let Some(effect) = shell.apply_action(action)
        {
            pending.0.push(effect);
        }
        // 2a. Applications action menu: the TUI-local `a` chord (TUI
        //     `OpenProcessMenu` parity) opens the process control menu —
        //     end task/tree, suspend/resume, force kill, and the neutral
        //     priority tiers. Bare Enter stays with the shell (it expands a
        //     tree row / jumps to the next search match there), so this menu
        //     does not join the inventory Enter arm below.
        if matches!(context, KeyboardOwner::Free)
            && route.page == Page::Processes
            && event.key_code == KeyCode::KeyA
            && modifiers == Modifiers::NONE
        {
            let opened =
                crate::pages::processes::menu::open_for_selected(proc_modal.as_mut(), shell);
            if opened {
                commands.trigger(MenuModalChanged::<ProcessMenuCtx>(true, Default::default()));
                applied = true;
                continue;
            }
        }
        // 2b. Closed-menu Enter attempt: bare Enter over a selected row on
        //     an inventory page opens that page's action menu (TUI
        //     Enter-actions parity, one arm per inventory).
        if matches!(context, KeyboardOwner::Free)
            && event.key_code == KeyCode::Enter
            && modifiers == Modifiers::NONE
        {
            let opened = match route.page {
                Page::Services => {
                    match svc_selection
                        .as_ref()
                        .and_then(|state| state.target.as_ref())
                    {
                        Some(target) => {
                            crate::pages::services::menu::open_for(&mut svc_modal, shell, target)
                        }
                        None => false,
                    }
                }
                Page::Startup => {
                    match stu_selection
                        .as_ref()
                        .and_then(|state| state.target.clone())
                    {
                        Some(target) => {
                            crate::pages::startup::menu::open_for(&mut stu_modal, shell, &target)
                        }
                        None => false,
                    }
                }
                Page::Sessions => {
                    match ses_selection
                        .as_ref()
                        .and_then(|state| state.target.clone())
                    {
                        Some(target) => {
                            crate::pages::sessions::menu::open_for(&mut ses_modal, shell, &target)
                        }
                        None => false,
                    }
                }
                _ => false,
            };
            if opened {
                match route.page {
                    Page::Services => commands
                        .trigger(MenuModalChanged::<ServiceMenuCtx>(true, Default::default())),
                    Page::Startup => commands
                        .trigger(MenuModalChanged::<StartupMenuCtx>(true, Default::default())),
                    Page::Sessions => commands
                        .trigger(MenuModalChanged::<SessionMenuCtx>(true, Default::default())),
                    _ => {}
                }
                applied = true;
                continue;
            }
        }
        // 3. Layout-correct characters through the shell char router.
        if text_char(event, modifiers).is_some_and(|character| {
            dispatch(
                shell.handle_local_char(character, modifiers),
                &mut pending.0,
            )
        }) {
            applied = true;
            continue;
        }
        // 4. Fixed-key router (arrows, Delete, Escape, F5, chorded letters).
        if let Some(shared) = shared_key(event.key_code) {
            let outcome = shell.handle_local_key(ShellKeyEvent::new(shared, modifiers));
            applied |= dispatch(outcome, &mut pending.0);
        }
    }
    if applied {
        commands.trigger(ShellInteractionApplied);
    }
    if armed_before != shell.confirmation_kind() {
        let view = shell
            .pending_confirmation()
            .and_then(PendingConfirmationView::from_pending);
        commands.trigger(ConfirmationChanged(view));
    }
    // Quit forwarding is frame-level, not key-level: a quit requested
    // outside the keyboard (tray, platform lifecycle) still exits exactly
    // once. The TUI checks the same state every loop iteration.
    if !quit.0 && shell.quit_reason().is_some() {
        exits.write(AppExit::Success);
        quit.0 = true;
    }
}

/// Record one dispatch outcome: `true` when the shell consumed the input.
fn dispatch(outcome: InputDispatch, pending: &mut Vec<PlatformEffect>) -> bool {
    match outcome {
        InputDispatch::Unhandled => false,
        InputDispatch::Consumed => true,
        InputDispatch::Effect(effect) => {
            pending.push(*effect);
            true
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/input.rs"]
mod tests;
