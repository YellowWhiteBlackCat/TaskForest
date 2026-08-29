//! GPUI adapter for the toolkit-neutral command router.

use super::{
    ProcessDetailsSection, RefreshRequest, RootView, TopPage, services_view, startup_view,
};
use gpui::{App, Context, KeyDownEvent, ModifiersChangedEvent, Window};
use std::sync::OnceLock;

use taskmanager_application::{
    AppAction, CommandContext, CommandRouter, CommandScope, ConfirmationKind, FocusDirection,
    KeyChord, KeyCode, Modifiers, ProcessTerminationAction, SelectionDirection, SurfaceKind,
    default_router,
};

use taskmanager_ui::focus::restore_modal;

const PROCESS_PAGE_ROWS: usize = 10;

fn command_router() -> Option<&'static CommandRouter> {
    static ROUTER: OnceLock<Option<CommandRouter>> = OnceLock::new();
    ROUTER.get_or_init(|| default_router().ok()).as_ref()
}

fn key_chord(ev: &KeyDownEvent) -> Option<KeyChord> {
    let key = match ev.keystroke.key.as_str() {
        "f" => KeyCode::F,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4,
        "5" => KeyCode::Digit5,
        "6" => KeyCode::Digit6,
        // Alt+7 selects the App-history page (the seventh shared route). The
        // shared router owns the chord; gpui only needs to translate the keysym
        // into `KeyCode::Digit7` so the router can match it.
        "7" => KeyCode::Digit7,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        // gpui spells the arrow keys as `up`/`down` in `keystroke.key`.
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        "tab" => KeyCode::Tab,
        "f5" => KeyCode::F5,
        "f9" => KeyCode::F9,
        "a" => KeyCode::A,
        "delete" => KeyCode::Delete,
        "enter" => KeyCode::Enter,
        "escape" => KeyCode::Escape,
        "space" => KeyCode::Space,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        _ => return None,
    };
    let modifiers = Modifiers::new(
        ev.keystroke.modifiers.control,
        ev.keystroke.modifiers.alt,
        ev.keystroke.modifiers.shift,
        ev.keystroke.modifiers.platform,
    );
    Some(KeyChord::new(key, modifiers))
}

fn command_context(view: &RootView, window: &Window) -> CommandContext {
    let contexts = window.context_stack();
    let text_input_focused = contexts.iter().any(|context| context.contains("Input"));
    let process_list_focused = contexts
        .iter()
        .any(|context| context.contains("ProcessList"));
    // Keep selected-process shortcuts available from the Applications toolbar
    // and headers. Text inputs still take precedence, and the router retains
    // the selection/overlay safety checks for destructive commands.
    let process_shortcuts_active = process_list_focused
        || (view.page == TopPage::Apps
            && view.selected_process_row().is_some()
            && !text_input_focused);
    // A confirmation dialog claims the keyboard scope so Enter can confirm it
    // (Dialog-scoped binding) instead of driving the list beneath it.
    let confirmation_open = view.active_surface_kind().is_some_and(|kind| {
        matches!(
            kind,
            super::GpuiSurfaceKind::Shared(SurfaceKind::Confirmation(_))
        )
    });
    CommandContext {
        scope: if confirmation_open {
            CommandScope::Dialog
        } else if process_shortcuts_active {
            CommandScope::ProcessList
        } else {
            CommandScope::Shell
        },
        text_input_focused,
        overlay_present: view.window_surface_open(),
        process_selected: view.selected_process_row().is_some(),
    }
}

/// Focus this window's own Apps-page search input (per-window state, created
/// on the first Apps render). Returns `false` when the input has not rendered
/// yet, so the caller can enqueue a typed destination in
/// `pending_search_focus`.
fn focus_process_search(view: &RootView, window: &mut Window, cx: &App) -> bool {
    let Some(input) = view.search_input.as_ref() else {
        return false;
    };
    input.read(cx).focus_handle().clone().focus(window);
    true
}

fn focus_page_search(page: TopPage, view: &RootView, window: &mut Window, cx: &mut App) -> bool {
    match page {
        TopPage::Apps => focus_process_search(view, window, cx),
        TopPage::Services => services_view::focus_search(view, window, cx),
        TopPage::Startup => startup_view::focus_search(view, window, cx),
        TopPage::Performance
        | TopPage::System
        | TopPage::Users
        | TopPage::AppHistory
        | TopPage::Containers => false,
    }
}

fn focus_search(view: &mut RootView, window: &mut Window, cx: &mut Context<RootView>) {
    if focus_page_search(view.page, view, window, cx) {
        return;
    }
    if !matches!(
        view.page,
        TopPage::Apps | TopPage::Services | TopPage::Startup
    ) {
        view.page = TopPage::Apps;
    }
    view.pending_search_focus = Some(view.page);
    cx.notify();
}

pub(super) fn defer_pending_search_focus(
    view: &mut RootView,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    let Some(page) = view.pending_search_focus.take() else {
        return;
    };
    let mark_capture_ready = view.capture_evidence.keyboard_focus_requested();
    let weak = cx.entity().downgrade();
    window.defer(cx, move |window, cx| {
        let focused = weak
            .update(cx, |view, cx| focus_page_search(page, view, window, cx))
            .unwrap_or(false);
        if focused && mark_capture_ready {
            let _ = weak.update(cx, |view, cx| {
                view.capture_evidence.mark_keyboard_focus_ready();
                cx.notify();
            });
        }
        window.refresh();
    });
}

fn move_process_page(view: &mut RootView, direction: SelectionDirection, preserve: bool) {
    // The keyboard paging projection is the SAME cached row model the render
    // path uses, so paging can never diverge from the pixels — and a keypress
    // never rebuilds the 10k-row projection. The sort/filter/query inputs are
    // the shell-owned process viewing state read inside the projection.
    // Navigation walks selectable semantic rows: real processes plus PID-less
    // application aggregates. Structural category headers remain outside the
    // selection domain.
    let (rows, _pids, _) = view.processes_projection();
    let ids: Vec<_> = rows.iter().filter_map(|row| row.selection_key).collect();
    if ids.is_empty() {
        return;
    }
    let current = view
        .selected_process_row()
        .and_then(|active| ids.iter().position(|candidate| *candidate == active));
    let next = match (current, direction) {
        (Some(index), SelectionDirection::PageUp) => index.saturating_sub(PROCESS_PAGE_ROWS),
        (Some(index), SelectionDirection::PageDown) => {
            index.saturating_add(PROCESS_PAGE_ROWS).min(ids.len() - 1)
        }
        // Single-row variants clamp at the same list bounds as the page path,
        // advancing exactly one visible row instead of PROCESS_PAGE_ROWS.
        (Some(index), SelectionDirection::Previous) => index.saturating_sub(1),
        (Some(index), SelectionDirection::Next) => (index + 1).min(ids.len() - 1),
        (Some(_), SelectionDirection::First) | (None, SelectionDirection::First) => 0,
        (Some(_), SelectionDirection::Last) | (None, SelectionDirection::Last) => ids.len() - 1,
        (None, SelectionDirection::PageUp) | (None, SelectionDirection::Previous) => ids.len() - 1,
        (None, SelectionDirection::PageDown) | (None, SelectionDirection::Next) => 0,
    };
    // Bare arrow/PageUp/Down collapses to the focused row; Ctrl/Shift preserves
    // an existing multi-selection (the shell-owned selection owns the rule).
    view.move_process_row_selection(ids.get(next).copied(), preserve);
}

fn dismiss_overlay(view: &mut RootView) {
    view.dismiss_current_surface(super::WindowSurfaceDismissReason::Escape);
    view.hovered = None;
}

/// Frontend-local shortcut-help chord: `F1` (bare) or `?` toggles the help
/// overlay. Neither chord exists in the shared `KeyCode` vocabulary — the
/// shell's `shell_local_bindings` (`crates/taskmanager-shell/src/keys.rs`)
/// treat `?` the same way: a frontend-local binding each frontend implements
/// itself. GPUI's equivalent is `F1` plus `?` (layout-independent: on
/// Linux/Wayland the keysym for Shift+/ arrives as key `"?"` with shift
/// folded away, which is also what `Keystroke::parse("shift-?")` produces in
/// tests).
fn help_toggle_chord(event: &KeyDownEvent) -> bool {
    let key = event.keystroke.key.as_str();
    if key == "?" {
        return true;
    }
    if key != "f1" {
        return false;
    }
    let m = event.keystroke.modifiers;
    !m.control && !m.alt && !m.shift && !m.platform && !m.function
}

fn confirm_active_surface(view: &mut RootView, cx: &mut Context<RootView>) {
    match view.shell.interaction.confirmation_kind() {
        Some(ConfirmationKind::ProcessTermination) => {
            view.confirm_process_termination(cx);
        }
        Some(ConfirmationKind::ServiceControl | ConfirmationKind::StartupControl) => {
            view.confirm_service_control_confirmation(cx);
        }
        Some(ConfirmationKind::ProcessBatch) => {
            view.confirm_process_batch(cx);
        }
        Some(ConfirmationKind::SmartSelfTest) => {
            view.confirm_system_health_self_test(cx);
        }
        Some(ConfirmationKind::EndTask | ConfirmationKind::SessionControl) | None => {}
    }
}

fn apply_app_action(
    view: &mut RootView,
    event: &KeyDownEvent,
    action: AppAction,
    window: &mut Window,
    cx: &mut Context<RootView>,
) {
    match action {
        AppAction::FocusSearch => focus_search(view, window, cx),
        AppAction::MoveFocus(FocusDirection::Next) => window.focus_next(),
        AppAction::MoveFocus(FocusDirection::Previous) => window.focus_prev(),
        AppAction::MoveSelection(direction) => {
            let preserve = event.keystroke.modifiers.control || event.keystroke.modifiers.shift;
            move_process_page(view, direction, preserve);
        }
        AppAction::SelectPage(page) => {
            view.select_page(TopPage::from_app_page(page));
        }
        AppAction::Refresh(_) => view.request_refresh(RefreshRequest::Processes),
        AppAction::RequestEndTask => {
            if view.selected_application_root().is_some() {
                view.request_process_batch(
                    taskmanager_core::core::process::ProcessBatchAction::End,
                );
            } else if let Some(pid) = view.selected_pid() {
                view.request_process_termination(ProcessTerminationAction::EndTask, pid);
            }
        }
        AppAction::OpenProperties => {
            if let Some(pid) = view.selected_pid() {
                view.open_process_details(pid, ProcessDetailsSection::Overview);
            }
        }
        AppAction::OpenSystemAbout => view.show_system_about(),
        AppAction::DismissOverlay => {
            restore_modal(window, cx);
            dismiss_overlay(view);
        }
        AppAction::TogglePause => {
            let paused = view.telemetry_refresh_policy.is_manually_paused();
            view.telemetry_refresh_policy
                .apply(taskmanager_application::TelemetryRefreshPolicyChange::SetPaused(!paused));
            super::tray::sync_tray_pause_checkmark(view, !paused);
        }
        AppAction::ToggleSidebar => view.sidebar_visible = !view.sidebar_visible,
        AppAction::ConfirmEndTask => confirm_active_surface(view, cx),
        AppAction::RequestServiceControl
        | AppAction::ConfirmServiceControl
        | AppAction::CopySelectedRow
        | AppAction::OpenAlerts => {}
    }
}

impl RootView {
    /// Apply the real modifier lifecycle to the application-owned refresh
    /// policy. GPUI emits `ModifiersChangedEvent` for Ctrl because Wayland
    /// does not deliver a normal key-up event for modifier keys; this keeps
    /// the hold-Ctrl pause correct across Linux/Wayland and other backends.
    pub(super) fn handle_root_modifiers_changed(
        view: &mut Self,
        event: &ModifiersChangedEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A cold-start mask owns the refresh lifecycle until a complete frame
        // exists. Letting Ctrl pause the scheduler here could freeze the only
        // path that can dismiss the mask, so modifier-driven pause is ignored
        // during warm-up.
        if view.telemetry_frame_state.is_collecting() {
            return;
        }
        let control_held = event.modifiers.control;
        if view.telemetry_refresh_policy.is_control_held() == control_held {
            return;
        }
        view.telemetry_refresh_policy.apply(
            taskmanager_application::TelemetryRefreshPolicyChange::SetControlHeld(control_held),
        );
        cx.notify();
    }

    pub(super) fn handle_root_key_down(
        view: &mut Self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if view.telemetry_frame_state.is_collecting() {
            let key = event.keystroke.key.as_str();
            let retry_surface = view.telemetry_warmup_phase().allows_retry();
            if retry_surface && key == "tab" {
                if let Some(button) = view.telemetry_warmup_retry_button.as_ref() {
                    button.read(cx).focus_handle().focus(window);
                }
                cx.stop_propagation();
            } else if retry_surface && matches!(key, "enter" | "space") {
                // Leave activation keys available to the focused retry button;
                // the button owns the unified pointer/keyboard callback.
            } else {
                // The warm-up surface is a real modal boundary: page changes,
                // refresh shortcuts, pause, help and destructive actions must
                // not reach the hidden page tree.
                cx.stop_propagation();
            }
            return;
        }
        // Frontend-local F1 / `?` help toggle, resolved BEFORE the shared
        // router (neither chord is in the shared KeyCode vocabulary; see
        // `help_toggle_chord`). `?` is suppressed while a text input is
        // focused so the literal character still types there — F1 stays
        // active because no input consumes it.
        let context = command_context(view, window);
        let toggles_help = help_toggle_chord(event)
            && (event.keystroke.key == "f1" || !context.text_input_focused);
        if toggles_help {
            match view.input_scope() {
                super::GpuiInputScope::Content
                | super::GpuiInputScope::Surface(super::GpuiSurfaceKind::Local(
                    super::WindowSurfaceKind::Help,
                )) => {
                    view.toggle_help();
                }
                super::GpuiInputScope::TelemetryWarmup | super::GpuiInputScope::Surface(_) => {}
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let Some(chord) = key_chord(event) else {
            return;
        };
        let Some(router) = command_router() else {
            return;
        };
        let Some(action) = router.route(chord, context) else {
            return;
        };

        apply_app_action(view, event, action, window, cx);
        cx.stop_propagation();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_keyboard_tests.rs"]
mod tests;
