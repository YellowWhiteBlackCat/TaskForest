//! The ordered keyboard adapter: one press, one pass through the shell
//! routers, in modal-precedence order.
//!
//! [`DispatchFrame`] owns the per-press frame and the arm chain
//! ([`DispatchFrame::dispatch`]); each arm is one small method whose doc
//! comment carries the precedence number from the parity contract. The
//! vocabulary and the resources the arms read live in [`super`]: the surface
//! owner, the action-menu presence snapshot, and the effect / re-render
//! resources the seam publishes.

use bevy::app::AppExit;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::system::{Commands, NonSendMut, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use taskmanager_application::{Modifiers, PlatformEffect};

use taskmanager_shell::{InputDispatch, ShellApp, ShellKeyEvent};

use crate::app::{
    FrontendTrack, Page, Route, action_for_page, modifier_state, request_route, route_key_press,
};
use crate::confirmation::{ConfirmationChanged, PendingConfirmationView};
use crate::drain::FeedbackCache;
use crate::input_contract::shared_key;
use crate::menu_modal::{MenuModalChanged, ModalDriver};
use crate::pages::performance::{PerformanceDeviceFocus, PerformanceDeviceTarget};
use crate::pages::processes::menu::ProcessMenuCtx;
use crate::pages::services::log_panel::{ServiceLogControlAction, ServiceLogExportDir};
use crate::pages::services::menu::ServiceMenuCtx;
use crate::pages::sessions::menu::SessionMenuCtx;
use crate::pages::startup::menu::StartupMenuCtx;

use super::{
    FrontendMenuKind, FrontendMenus, InventoryActionModals, InventorySelections, KeyboardOwner,
    PendingEffects, QuitForwarded, ShellInteractionApplied, TextInputState, commit_query_to_shell,
    keyboard_owner, modifiers_from, text_char,
};

/// One just-pressed key, normalized once for the whole arm chain: the facts
/// every arm reads are captured here so each arm stays a predicate on
/// [`KeyPress`] plus the state it drives.
#[derive(Clone, Copy)]
struct KeyPress {
    /// The raw Bevy key (arms that own physical chords read this).
    key_code: KeyCode,
    /// The layout-correct character, when the press has one.
    character: Option<char>,
    /// The frame modifier snapshot.
    modifiers: Modifiers,
    /// The surface that owns the keyboard for this press.
    context: KeyboardOwner,
    /// This frontend's visible page.
    page: Page,
    /// The frontend-local action menus present when the press landed.
    frontend_menus: FrontendMenus,
}

/// The per-frame dispatch state: the shell, every frontend-local surface an
/// arm may drive, and the accumulated `applied` flag.
///
/// Bundling the state is what keeps the ordered arms small: each arm is a
/// method that reads [`KeyPress`] and mutates exactly the state it owns,
/// while the precedence order stays visible in one place
/// ([`DispatchFrame::dispatch`]). No arm adds semantics: every effect goes
/// through the shell's own routers.
///
/// The lifetime parameters mirror the system parameters they are borrowed
/// from: `'a` is the frame borrow, `'w`/`'s` the [`Commands`] lifetimes,
/// `'m`/`'n` the modal and selection resources, and `'t` the search text
/// state.
struct DispatchFrame<'a, 'w, 's, 'm, 'n, 't> {
    /// Live key state: the route-chord arm captures the modifier snapshot.
    keys: &'a ButtonInput<KeyCode>,
    /// Shell state owner: the single authority for gate, search, and tables.
    shell: &'a mut ShellApp,
    /// This frontend's visible route.
    route: &'a mut Route,
    /// Effects collected for the frame-tail drain.
    pending: &'a mut Vec<PlatformEffect>,
    /// Deferred commands (menu overlay transitions, repaint signals).
    commands: &'a mut Commands<'w, 's>,
    /// The four per-inventory action menus.
    modals: &'a mut InventoryActionModals<'m>,
    /// The inventory table selections.
    selections: &'a InventorySelections<'n>,
    /// Search text-editing state (absent before the page resources mount).
    text_state: &'a mut Option<ResMut<'t, TextInputState>>,
    /// Service-log export directory (absent outside the window shell).
    export_dir: Option<&'a ServiceLogExportDir>,
    /// Performance page device focus (absent outside the window shell).
    perf_device_focus: Option<&'a PerformanceDeviceFocus>,
    /// Whether any press this frame mutated shell state.
    applied: bool,
}

impl DispatchFrame<'_, '_, '_, '_, '_, '_> {
    /// Normalizes one raw Bevy press once for the whole arm chain.
    fn normalize(&self, event: &KeyboardInput, modifiers: Modifiers) -> KeyPress {
        let frontend_menus = self.modals.present();
        KeyPress {
            key_code: event.key_code,
            character: text_char(event, modifiers),
            modifiers,
            context: keyboard_owner(self.shell, frontend_menus, self.route.page),
            page: self.route.page,
            frontend_menus,
        }
    }

    /// Dispatches every just-pressed key through the ordered arms. The frame
    /// tail (re-render, confirmation view, feedback cache, quit) belongs to
    /// [`keyboard_dispatch_system`], which owns the frame-scoped reads.
    fn run(&mut self, events: &[KeyboardInput], modifiers: Modifiers) {
        for event in events {
            let press = self.normalize(event, modifiers);
            self.dispatch(press);
        }
    }

    /// The ordered arm chain for one press: an arm sees the press only when
    /// every earlier arm declined it.
    ///
    /// The two successors of the char router are additive rather than
    /// exclusive — a press the shell char router declined may still be a
    /// table-row arrow or a shared fixed binding — so they run in sequence
    /// once the chain above them falls through.
    fn dispatch(&mut self, press: KeyPress) {
        if self.frontend_menus(press) {
            return;
        }
        if self.service_log_panel(press) {
            return;
        }
        if self.dismiss_feedback(press) {
            return;
        }
        if self.route_chord(press) {
            return;
        }
        if self.confirm_gate(press) {
            return;
        }
        if self.process_action_chord(press) {
            return;
        }
        if self.open_inventory_menu(press) {
            return;
        }
        if self.smart_self_test(press) {
            return;
        }
        if self.search_editing(press) {
            return;
        }
        if self.shell_char(press) {
            return;
        }
        self.table_motion(press);
        self.shared_key(press);
    }

    /// Arm 0a — open action menus (frontend-local modals): an open modal owns the
    /// keyboard ahead of navigation chords and the shell; its overlay
    /// mounts/despawns with the session transition. An open modal swallows
    /// the press whether or not the driven key changed shell state. Presence
    /// comes from the press snapshot, so the drive and the despawn decisions
    /// read the surfaces the press landed on, never a state an earlier arm
    /// produced.
    fn frontend_menus(&mut self, press: KeyPress) -> bool {
        let menus = press.frontend_menus;
        if menus.is_empty() {
            return false;
        }
        if menus.holds(FrontendMenuKind::Service) {
            self.applied |= self
                .modals
                .svc
                .drive(self.shell, press.key_code, self.pending);
        }
        if menus.holds(FrontendMenuKind::Startup) {
            self.applied |= self
                .modals
                .stu
                .drive(self.shell, press.key_code, self.pending);
        }
        if menus.holds(FrontendMenuKind::Session) {
            self.applied |= self
                .modals
                .ses
                .drive(self.shell, press.key_code, self.pending);
        }
        if menus.holds(FrontendMenuKind::Process) {
            self.applied |= self
                .modals
                .proc
                .drive(self.shell, press.key_code, self.pending);
        }
        if menus.holds(FrontendMenuKind::Service) && !self.modals.svc.is_open() {
            self.commands.trigger(MenuModalChanged::<ServiceMenuCtx>(
                false,
                Default::default(),
            ));
        }
        if menus.holds(FrontendMenuKind::Startup) && !self.modals.stu.is_open() {
            self.commands.trigger(MenuModalChanged::<StartupMenuCtx>(
                false,
                Default::default(),
            ));
        }
        if menus.holds(FrontendMenuKind::Session) && !self.modals.ses.is_open() {
            self.commands.trigger(MenuModalChanged::<SessionMenuCtx>(
                false,
                Default::default(),
            ));
        }
        if menus.holds(FrontendMenuKind::Process) && !self.modals.proc.is_open() {
            self.commands.trigger(MenuModalChanged::<ProcessMenuCtx>(
                false,
                Default::default(),
            ));
        }
        if self.applied {
            crate::confirmation::republish(self.shell, self.commands);
            self.commands.trigger(ShellInteractionApplied);
        }
        true
    }

    /// Arm 0b — service log panel (frontend-local surface, TUI panel parity):
    /// F/P/L/T/E/Esc are consumed ahead of navigation chords and the shell
    /// routers while the panel owns the Services page keyboard.
    fn service_log_panel(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::ServiceLogPanel)
            || press.modifiers != Modifiers::NONE
        {
            return false;
        }
        let Some(action) = crate::pages::services::log_panel::log_panel_key(press.key_code) else {
            return false;
        };
        match action {
            ServiceLogControlAction::ToggleFollow => self.shell.toggle_service_log_follow(),
            ServiceLogControlAction::TogglePaused => self.shell.toggle_service_log_paused(),
            ServiceLogControlAction::CycleLevel => self.shell.cycle_service_log_level(),
            ServiceLogControlAction::CycleTime => self.shell.cycle_service_log_time(),
            ServiceLogControlAction::Export => {
                let dir = self.export_dir.and_then(|export| export.0.as_deref());
                crate::pages::services::log_panel::export_service_log(self.shell, dir);
            }
            ServiceLogControlAction::Close => self.shell.close_service_log(),
        }
        self.commands
            .trigger(crate::pages::services::log_panel::LogPanelRepaintRequired);
        self.applied = true;
        true
    }

    /// Arm 0c — active feedback notice dismissal: when no confirmation or modal
    /// is open, bare Escape clears the notice immediately.
    fn dismiss_feedback(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Free)
            || press.key_code != KeyCode::Escape
            || press.modifiers != Modifiers::NONE
            || self.shell.feedback_notice().is_none()
        {
            return false;
        }
        self.shell.clear_feedback_notice();
        self.applied = true;
        true
    }

    /// Arm 1 — frontend navigation: route chords move the Bevy route AND the
    /// shell page so scope derivation follows the visible page.
    fn route_chord(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Free) {
            return false;
        }
        let Some(page) = route_key_press(press.key_code, modifier_state(self.keys)) else {
            return false;
        };
        if let Some(action) = action_for_page(page)
            && let Some(effect) = self.shell.apply_action(action)
        {
            self.pending.push(effect);
        }
        request_route(self.route, page, self.commands);
        self.applied = true;
        true
    }

    /// Arm 2 — dialog-scope Enter: confirm whatever gate is armed.
    fn confirm_gate(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Gate)
            || press.key_code != KeyCode::Enter
            || press.modifiers != Modifiers::NONE
        {
            return false;
        }
        let Some(kind) = self.shell.confirmation_kind() else {
            return false;
        };
        if let Some(effect) = crate::confirmation::confirm_armed(self.shell, kind) {
            self.pending.push(effect);
        }
        self.applied = true;
        true
    }

    /// Arm 2a — Applications action menu: the TUI-local `a` chord (TUI
    /// `OpenProcessMenu` parity) opens the process control menu — end
    /// task/tree, suspend/resume, force kill, and the neutral priority tiers.
    /// Bare Enter stays with the shell (it expands a tree row / jumps to the
    /// next search match there), so this menu does not join the inventory
    /// Enter arm below.
    fn process_action_chord(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Free)
            || press.page != Page::Processes
            || press.key_code != KeyCode::KeyA
            || press.modifiers != Modifiers::NONE
        {
            return false;
        }
        if !crate::pages::processes::menu::open_for_selected(self.modals.proc.as_mut(), self.shell)
        {
            return false;
        }
        self.commands
            .trigger(MenuModalChanged::<ProcessMenuCtx>(true, Default::default()));
        self.applied = true;
        true
    }

    /// Arm 2b — closed-menu Enter / 'a' attempt: bare Enter or 'a' over a selected
    /// row on an inventory page opens that page's action menu (TUI
    /// Enter-actions parity, one open-attempt per inventory).
    fn open_inventory_menu(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Free)
            || press.modifiers != Modifiers::NONE
            || !(press.key_code == KeyCode::Enter
                || (press.key_code == KeyCode::KeyA && press.page != Page::Processes))
        {
            return false;
        }
        let opened = match press.page {
            Page::Services => self.open_services_menu(),
            Page::Startup => self.open_startup_menu(),
            Page::Sessions => self.open_sessions_menu(),
            _ => false,
        };
        if !opened {
            return false;
        }
        match press.page {
            Page::Services => self
                .commands
                .trigger(MenuModalChanged::<ServiceMenuCtx>(true, Default::default())),
            Page::Startup => self
                .commands
                .trigger(MenuModalChanged::<StartupMenuCtx>(true, Default::default())),
            Page::Sessions => self
                .commands
                .trigger(MenuModalChanged::<SessionMenuCtx>(true, Default::default())),
            _ => {}
        }
        self.applied = true;
        true
    }

    /// The Services open-attempt: the table selection, else the first sorted
    /// service. `false` when the page holds neither (no state change).
    fn open_services_menu(&mut self) -> bool {
        let target = self
            .selections
            .svc
            .as_ref()
            .and_then(|state| state.target.clone())
            .or_else(|| {
                self.shell
                    .sorted_services()
                    .first()
                    .map(|service| service.id.clone())
            });
        let Some(target) = target else {
            return false;
        };
        crate::pages::services::menu::open_for(&mut self.modals.svc, self.shell, &target)
    }

    /// The Startup open-attempt: the table selection, else the first sorted
    /// entry. `false` when the page holds neither (no state change).
    fn open_startup_menu(&mut self) -> bool {
        let target = self
            .selections
            .stu
            .as_ref()
            .and_then(|state| state.target.clone())
            .or_else(|| {
                self.shell
                    .sorted_startup_entries()
                    .first()
                    .map(|entry| entry.id.clone())
            });
        let Some(target) = target else {
            return false;
        };
        crate::pages::startup::menu::open_for(&mut self.modals.stu, self.shell, &target)
    }

    /// The Sessions open-attempt: the table selection, else the first sorted
    /// session. `false` when the page holds neither (no state change).
    fn open_sessions_menu(&mut self) -> bool {
        let target = self
            .selections
            .ses
            .as_ref()
            .and_then(|state| state.target.clone())
            .or_else(|| {
                self.shell
                    .sorted_sessions()
                    .first()
                    .map(|session| session.id.clone())
            });
        let Some(target) = target else {
            return false;
        };
        crate::pages::sessions::menu::open_for(&mut self.modals.ses, self.shell, &target)
    }

    /// Arm 2c — Performance page: 't' chord triggers SMART self-test for the
    /// selected disk (focused device, else the first reported disk).
    fn smart_self_test(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Free)
            || press.page != Page::Performance
            || press.key_code != KeyCode::KeyT
            || press.modifiers != Modifiers::NONE
        {
            return false;
        }
        let focused = self.perf_device_focus.and_then(|focus| match &focus.0 {
            PerformanceDeviceTarget::Disk(id) => Some(id.clone()),
            _ => None,
        });
        let disk_id = focused.or_else(|| {
            self.shell
                .projection()
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.disks.first())
                .map(|disk| disk.device_id.clone())
        });
        let Some(disk_id) = disk_id else {
            return false;
        };
        if !crate::pages::performance::request_smart_self_test(
            self.shell,
            &disk_id,
            taskmanager_core::core::smart::SmartSelfTestKind::Short,
        ) {
            return false;
        }
        self.applied = true;
        true
    }

    /// Arm 2d — search input editing: when search owns the keyboard, handle
    /// navigation & editing. A key the line editor does not consume keeps
    /// falling through the chain, so the shell's own search bindings stay
    /// reachable.
    fn search_editing(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Search) {
            return false;
        }
        let mut query = self.shell.query.clone();
        let mut state = self.text_state.as_deref_mut().cloned().unwrap_or_default();
        let consumed = edit_search_line(press, &mut state, &mut query, self.shell);
        if let Some(resource) = self.text_state {
            **resource = state;
        }
        if !consumed {
            return false;
        }
        self.applied = true;
        true
    }

    /// Arm 3 — layout-correct characters through the shell char router.
    fn shell_char(&mut self, press: KeyPress) -> bool {
        let Some(character) = press.character else {
            return false;
        };
        if !dispatch(
            self.shell.handle_local_char(character, press.modifiers),
            self.pending,
        ) {
            return false;
        }
        self.applied = true;
        true
    }

    /// Arm 3a — table row selection motion for non-process inventory tables.
    /// Additive: it runs after the char router declined the press and always
    /// falls through to the fixed-key router below.
    fn table_motion(&mut self, press: KeyPress) {
        if !matches!(press.context, KeyboardOwner::Free) || press.modifiers != Modifiers::NONE {
            return;
        }
        match (press.page, press.key_code) {
            (Page::Startup, KeyCode::ArrowUp) => self
                .commands
                .trigger(crate::pages::startup::StartupSelectionMoved(-1)),
            (Page::Startup, KeyCode::ArrowDown) => self
                .commands
                .trigger(crate::pages::startup::StartupSelectionMoved(1)),
            (Page::Sessions, KeyCode::ArrowUp) => self
                .commands
                .trigger(crate::pages::sessions::SessionSelectionMoved(-1)),
            (Page::Sessions, KeyCode::ArrowDown) => self
                .commands
                .trigger(crate::pages::sessions::SessionSelectionMoved(1)),
            (Page::Services, KeyCode::ArrowUp) => self
                .commands
                .trigger(crate::pages::services::ServiceSelectionMoved(-1)),
            (Page::Services, KeyCode::ArrowDown) => self
                .commands
                .trigger(crate::pages::services::ServiceSelectionMoved(1)),
            _ => return,
        }
        self.applied = true;
    }

    /// Arm 4 — fixed-key router (arrows, Delete, Escape, F5, F9, chorded letters).
    fn shared_key(&mut self, press: KeyPress) {
        let Some(shared) = shared_key(press.key_code) else {
            return;
        };
        if shared == taskmanager_application::KeyCode::F9 && press.modifiers == Modifiers::NONE {
            self.commands
                .trigger(crate::pages::performance::TogglePerformanceSidebar);
            self.applied = true;
            return;
        }
        let outcome = self
            .shell
            .handle_local_key(ShellKeyEvent::new(shared, press.modifiers));
        self.applied |= dispatch(outcome, self.pending);
    }
}

/// Applies one press to the search line, committing every query edit to the
/// shell (the single owner of the search text). `true` when the line editor
/// consumed the press; an unconsumed press keeps falling through the chain.
fn edit_search_line(
    press: KeyPress,
    state: &mut TextInputState,
    query: &mut String,
    shell: &mut ShellApp,
) -> bool {
    let is_ctrl = press.modifiers.control;
    match press.key_code {
        KeyCode::ArrowLeft => {
            if is_ctrl {
                state.move_word_left(query);
            } else {
                state.move_left(query);
            }
        }
        KeyCode::ArrowRight => {
            if is_ctrl {
                state.move_word_right(query);
            } else {
                state.move_right(query);
            }
        }
        KeyCode::Home => state.move_home(),
        KeyCode::End => state.move_end(query),
        KeyCode::Backspace => {
            let changed = if is_ctrl {
                state.delete_word_backward(query)
            } else {
                state.delete_backward(query)
            };
            if changed {
                commit_query_to_shell(shell, query);
            }
        }
        KeyCode::Delete => {
            if state.delete_forward(query) {
                commit_query_to_shell(shell, query);
            }
        }
        KeyCode::Escape => {
            if !query.is_empty() {
                state.clear_line(query);
                commit_query_to_shell(shell, query);
            } else {
                shell.close_search();
            }
        }
        KeyCode::KeyU if is_ctrl => {
            state.clear_line(query);
            commit_query_to_shell(shell, query);
        }
        KeyCode::KeyC if is_ctrl => state.copy_to_clipboard(query),
        KeyCode::KeyX if is_ctrl => {
            state.cut_to_clipboard(query);
            commit_query_to_shell(shell, query);
        }
        KeyCode::KeyV if is_ctrl => {
            state.paste_from_clipboard(query);
            commit_query_to_shell(shell, query);
        }
        _ => {
            let Some(character) = press.character else {
                return false;
            };
            if press.modifiers.control || press.modifiers.alt {
                return false;
            }
            state.insert_char(query, character);
            commit_query_to_shell(shell, query);
        }
    }
    true
}

/// The `Update` keyboard adapter: normalize every just-pressed Bevy key and
/// forward it through the shell's routers. One pass, in modal-precedence
/// order; effects and re-render signals collect for the frame tail.
#[allow(clippy::too_many_arguments)]
pub(crate) fn keyboard_dispatch_system(
    mut presses: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut track: NonSendMut<FrontendTrack>,
    mut modals: InventoryActionModals,
    selections: InventorySelections,
    mut pending: ResMut<PendingEffects>,
    mut route: ResMut<Route>,
    mut quit: ResMut<QuitForwarded>,
    mut exits: MessageWriter<AppExit>,
    perf_device_focus: Option<Res<PerformanceDeviceFocus>>,
    export_dir: Option<Res<ServiceLogExportDir>>,
    feedback_cache: Option<ResMut<FeedbackCache>>,
    mut text_state: Option<ResMut<TextInputState>>,
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
    let applied = {
        let mut frame = DispatchFrame {
            keys: &keys,
            shell,
            route: &mut route,
            pending: &mut pending.0,
            commands: &mut commands,
            modals: &mut modals,
            selections: &selections,
            text_state: &mut text_state,
            export_dir: export_dir.as_deref(),
            perf_device_focus: perf_device_focus.as_deref(),
            applied: false,
        };
        frame.run(&events, modifiers);
        frame.applied
    };
    if applied {
        commands.trigger(ShellInteractionApplied);
    }
    if armed_before != shell.confirmation_kind() {
        let view = shell
            .pending_confirmation()
            .and_then(PendingConfirmationView::from_pending);
        commands.trigger(ConfirmationChanged(view));
    }
    if let Some(mut cache) = feedback_cache {
        let feedback = shell.feedback_text().to_owned();
        if cache.0.as_deref() != Some(feedback.as_str()) {
            cache.0 = Some(feedback.clone());
            commands.trigger(crate::drain::FeedbackChanged(feedback));
        }
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
