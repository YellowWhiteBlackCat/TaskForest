//! Ordered TUI keyboard systems.
//!
//! Each system returns the shared explicit [`InputDispatch`] state. The
//! registry is the sole precedence authority: the first consumed route wins,
//! including routes that mutate only local state and emit no platform work.
//!
//! # Chord authority (TUI-003)
//!
//! The TUI-local command chords (`p i h c x`, `Enter`, `1-7`, `C m B y a`,
//! `o`, `e`, `d`, `g`) are NOT hand-matched here: every one resolves through
//! [`crate::command_palette::TUI_LOCAL_COMMANDS`], which declares the chord,
//! its palette executability, and its direct-dispatch arms (scope + typed
//! action). This file owns only the two things a registry cannot: the
//! *precedence order* (which system sees a key first) and the *execution* of
//! one typed action. The shell-owned characters (`q ? s S T`, executed via
//! [`shell_character_system`] and the sort/palette refinements) and the
//! contextual gestures ([`prefix_jump_system`], [`source_retry_system`],
//! [`app_history_window_system`]) are separate layers and must never appear in
//! the TUI registry — one chord, one declaring layer.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_application::{AppPage, Modifiers, PlatformEffect};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, InfoTable, InputDispatch,
};

use crate::command_palette::{TUI_LOCAL_COMMANDS, TuiDirectAction, TuiDirectArm, TuiDirectScope};
use crate::ui::chart_cursor::chart_cursor_system;
use crate::{FocusPanel, PerfDevice, TuiApp, TuiInputScope};

use super::{HELP_PAGE_STEP, inline_network_escalation_ready, key_to_terminal, modals, navigation};
use taskmanager_application::SurfaceKind;
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_shell::ShellApp;

mod direct;
use direct::run_direct_arms;

type KeySystem = fn(&mut TuiApp, &KeyEvent) -> InputDispatch;

/// Input precedence is data, not nesting. New input owners must be inserted in
/// this registry and return an explicit dispatch state.
const KEY_SYSTEMS: [KeySystem; 13] = [
    open_modal_system,
    owned_input_system,
    character_system,
    details_focus_system,
    selection_extension_system,
    detail_scroll_system,
    cpu_detail_scroll_system,
    performance_scroll_system,
    chart_cursor_system,
    table_navigation_system,
    nonflat_navigation_system,
    feedback_notice_dismiss_system,
    content_system,
];

pub(super) fn handle_key(app: &mut TuiApp, key: KeyEvent) -> Option<PlatformEffect> {
    for system in KEY_SYSTEMS {
        let dispatch = system(app, &key);
        if dispatch.is_consumed() {
            return dispatch.into_effect();
        }
    }
    None
}

fn open_modal_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    modals::handle_open_modal(app, *key)
}

/// Route scopes with exclusive keyboard ownership before content systems.
fn owned_input_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    match app.input_scope() {
        TuiInputScope::SharedSurface(kind) => route_shared_surface(app, kind, key),
        TuiInputScope::Help => route_help(app, key),
        TuiInputScope::Suggestions => route_shell_owned_input(app, key),
        TuiInputScope::Search => route_search(app, key),
        TuiInputScope::LocalSurface(_) => InputDispatch::Consumed,
        TuiInputScope::ServiceLog | TuiInputScope::DetailsPanel | TuiInputScope::Content => {
            InputDispatch::Unhandled
        }
    }
}

fn route_shared_surface(app: &mut TuiApp, kind: SurfaceKind, key: &KeyEvent) -> InputDispatch {
    match kind {
        SurfaceKind::Confirmation(_) => match key.code {
            KeyCode::Char(character) => app.handle_local_char(character, modifiers(key)),
            KeyCode::Esc => {
                app.shell.dismiss_overlay();
                InputDispatch::Consumed
            }
            _ => InputDispatch::Consumed,
        },
        SurfaceKind::ProcessProperties => {
            if key.code == KeyCode::Esc {
                app.shell.dismiss_overlay();
            }
            InputDispatch::Consumed
        }
    }
}

fn route_help(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if key.code == KeyCode::F(1) {
        app.toggle_help();
        return InputDispatch::Consumed;
    }
    let offset = match key.code {
        KeyCode::Up => Some(-1),
        KeyCode::Down => Some(1),
        KeyCode::PageUp => Some(-(HELP_PAGE_STEP as isize)),
        KeyCode::PageDown => Some(HELP_PAGE_STEP as isize),
        _ => None,
    };
    if let Some(offset) = offset {
        app.help_scroll_by(offset);
        InputDispatch::Consumed
    } else {
        route_shell_owned_input(app, key)
    }
}

fn route_shell_owned_input(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    match key.code {
        KeyCode::Char(character) => app.handle_local_char(character, modifiers(key)),
        _ => key_to_terminal(*key)
            .map_or(InputDispatch::Consumed, |event| app.handle_local_key(event)),
    }
}

fn route_search(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    match key.code {
        KeyCode::Char(character) => app.handle_local_char(character, modifiers(key)),
        KeyCode::Esc if !app.query.is_empty() => {
            app.query.clear();
            app.detail_scroll_reset();
            InputDispatch::Consumed
        }
        KeyCode::Backspace => {
            app.pop_search_char();
            InputDispatch::Consumed
        }
        KeyCode::Enter if app.page() == AppPage::Applications => {
            InputDispatch::consumed(app.jump_to_next_search_match())
        }
        _ => route_shell_owned_input(app, key),
    }
}

fn character_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    let KeyCode::Char(character) = key.code else {
        return InputDispatch::Unhandled;
    };
    let modifiers = modifiers(key);
    for system in CHARACTER_SYSTEMS {
        let dispatch = system(app, character, modifiers);
        if dispatch.is_consumed() {
            return dispatch;
        }
    }
    InputDispatch::Unhandled
}

type CharacterSystem = fn(&mut TuiApp, char, Modifiers) -> InputDispatch;

/// Shell-owned characters run first (plus their TUI execution refinements);
/// the TUI-local command registry resolves after them; the contextual prefix
/// jump is the last character consumer. The registry sits here — not earlier —
/// because none of the systems above it consume a registry chord, so the
/// observable precedence of every command is unchanged from the days when
/// each chord had its own hand-written system.
const CHARACTER_SYSTEMS: [CharacterSystem; 7] = [
    app_history_window_system,
    sort_system,
    palette_system,
    source_retry_system,
    shell_character_system,
    tui_local_command_system,
    prefix_jump_system,
];

fn app_history_window_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    if (app.page() != AppPage::AppHistory
        && !(app.page() == AppPage::Performance && app.history_replay_open()))
        || modifiers != Modifiers::NONE
    {
        return InputDispatch::Unhandled;
    }
    let window = match character {
        '1' => HistoryWindow::OneHour,
        '2' => HistoryWindow::TwentyFourHours,
        '3' => HistoryWindow::SevenDays,
        _ => return InputDispatch::Unhandled,
    };
    let _ = app.select_application_history_window(window);
    InputDispatch::Consumed
}

fn sort_system(app: &mut TuiApp, character: char, modifiers: Modifiers) -> InputDispatch {
    if !matches!(character, 's' | 'S') || modifiers.control || modifiers.alt {
        return InputDispatch::Unhandled;
    }
    if let Some(table) = info_table_for_page(app.page()) {
        if character == 's' {
            app.cycle_info_sort_column_preserving_anchor(table);
        } else {
            app.toggle_info_sort_direction_preserving_anchor(table);
        }
        return InputDispatch::Consumed;
    }
    if app.page() != AppPage::Applications {
        return InputDispatch::Unhandled;
    }
    if character == 's' {
        app.cycle_sort_column_visible();
    } else {
        app.toggle_sort_direction();
        app.persist_process_prefs();
    }
    InputDispatch::Consumed
}

fn palette_system(app: &mut TuiApp, character: char, modifiers: Modifiers) -> InputDispatch {
    if character != '?' || modifiers.control || modifiers.alt {
        return InputDispatch::Unhandled;
    }
    app.open_command_palette();
    InputDispatch::Consumed
}

fn source_retry_system(app: &mut TuiApp, character: char, modifiers: Modifiers) -> InputDispatch {
    if !matches!(character, 'r' | 'R') || modifiers != Modifiers::NONE {
        return InputDispatch::Unhandled;
    }
    app.source_retry_request()
        .map_or(InputDispatch::Unhandled, |request| {
            InputDispatch::Effect(Box::new(PlatformEffect::Refresh(request)))
        })
}

fn shell_character_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    app.handle_local_char(character, modifiers)
}

/// Resolve one pressed character through the TUI-local command registry
/// ([`TUI_LOCAL_COMMANDS`]): the entry whose declared shortcut matches the
/// chord runs its first armed direct arm. The resource-digit entry declares a
/// range token rather than one literal character, so bare digits that match no
/// literal row resolve against it. `Unhandled` keeps the shell/contextual
/// layers after this system in play (exactly the fall-through each chord had
/// when it was hand-matched).
fn tui_local_command_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    let mut buffer = [0u8; 4];
    let pressed = character.encode_utf8(&mut buffer);
    let digit = character.is_ascii_digit().then_some(character);
    for command in TUI_LOCAL_COMMANDS {
        if command.binding.shortcut == pressed {
            return run_direct_arms(app, command.direct, digit, modifiers);
        }
    }
    if let Some(digit) = digit {
        let system = run_registry_shortcut(
            app,
            crate::command_palette::SYSTEM_HISTORY_DIGITS_SHORTCUT,
            Some(digit),
            modifiers,
        );
        if system.is_consumed() {
            return system;
        }
        return run_registry_shortcut(
            app,
            crate::command_palette::RESOURCE_DIGITS_SHORTCUT,
            Some(digit),
            modifiers,
        );
    }
    InputDispatch::Unhandled
}

/// Run the first armed arm of the registry entry declared under `shortcut`.
/// Used for the two range/key tokens the character scanner cannot match
/// literally: the Performance resource digits and the row-target `Enter`.
fn run_registry_shortcut(
    app: &mut TuiApp,
    shortcut: &str,
    digit: Option<char>,
    modifiers: Modifiers,
) -> InputDispatch {
    for command in TUI_LOCAL_COMMANDS {
        if command.binding.shortcut == shortcut {
            return run_direct_arms(app, command.direct, digit, modifiers);
        }
    }
    InputDispatch::Unhandled
}

fn toggle_marked_process(app: &mut TuiApp) {
    if let Some(process) = app.selected_detail_process()
        && let Some(identity) = ProcessLiveKey::from_process(&process)
    {
        app.shell.toggle_selected_identity(identity);
    }
    let marked = app.shell.selected_identities().len();
    let text = if marked == 0 {
        "Selection cleared".to_owned()
    } else {
        format!("{marked} processes marked for batch control")
    };
    app.report_notice(
        FeedbackSource::Interaction,
        FeedbackSeverity::Info,
        FeedbackLifecycle::SHORT,
        text,
    );
}

fn performance_scroll_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if key.modifiers.intersects(
        KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::SHIFT,
    ) {
        return InputDispatch::Unhandled;
    }
    let delta = match key.code {
        KeyCode::Up => -1,
        KeyCode::Down => 1,
        KeyCode::PageUp => -(HELP_PAGE_STEP as isize),
        KeyCode::PageDown => HELP_PAGE_STEP as isize,
        _ => return InputDispatch::Unhandled,
    };
    if app.page() == AppPage::Performance && app.history_replay_open() {
        app.scroll_history_replay_rows(delta);
        return InputDispatch::Consumed;
    }
    match (app.page(), app.perf_device) {
        (AppPage::Performance, PerfDevice::Cpu) => app.scroll_cpu_cores(delta),
        (AppPage::Performance, PerfDevice::Gpu) => app.scroll_gpu_engines(delta),
        (AppPage::System, _) => app.scroll_system(delta),
        _ => return InputDispatch::Unhandled,
    }
    InputDispatch::Consumed
}

fn prefix_jump_system(app: &mut TuiApp, character: char, modifiers: Modifiers) -> InputDispatch {
    if app.page() != AppPage::Applications
        || modifiers.control
        || modifiers.alt
        || matches!(character, 'q' | '?' | 's' | 'S' | 'T')
        || !character.is_ascii_alphanumeric()
    {
        return InputDispatch::Unhandled;
    }
    InputDispatch::consumed(app.handle_prefix_jump(character, app.service_log_now_micros))
}

fn details_focus_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.page() != AppPage::Applications || app.focus_panel != FocusPanel::Details {
        return InputDispatch::Unhandled;
    }
    match key.code {
        KeyCode::Up => app.detail_scroll_by(-1),
        KeyCode::Down => app.detail_scroll_by(1),
        KeyCode::Esc => app.focus_panel = FocusPanel::Table,
        _ => return InputDispatch::Unhandled,
    }
    InputDispatch::Consumed
}

fn selection_extension_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.page() != AppPage::Applications || !key.modifiers.contains(KeyModifiers::SHIFT) {
        return InputDispatch::Unhandled;
    }
    let delta = match key.code {
        KeyCode::Down => 1,
        KeyCode::Up => -1,
        _ => return InputDispatch::Unhandled,
    };
    let previously_marked = app.shell.selected_identities().clone();
    let effect = app.move_nonflat_selection_oneshot(delta);
    // The arrow step re-ran the single-select collapse, so the previously
    // marked set is restored member by member before the newly landed row
    // joins it — all through the shell's authorized multi-select writer.
    for identity in previously_marked {
        app.shell.add_selected_identity(identity);
    }
    if let Some(process) = app.selected_detail_process()
        && let Some(identity) = ProcessLiveKey::from_process(&process)
    {
        app.shell.add_selected_identity(identity);
    }
    InputDispatch::consumed(effect)
}

fn detail_scroll_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.page() != AppPage::Applications || !key.modifiers.contains(KeyModifiers::CONTROL) {
        return InputDispatch::Unhandled;
    }
    match key.code {
        KeyCode::Up => app.detail_scroll_by(-1),
        KeyCode::Down => app.detail_scroll_by(1),
        _ => return InputDispatch::Unhandled,
    }
    InputDispatch::Consumed
}

fn cpu_detail_scroll_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.page() != AppPage::Performance
        || app.perf_device != PerfDevice::Cpu
        || !key.modifiers.contains(KeyModifiers::CONTROL)
        || key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return InputDispatch::Unhandled;
    }
    let delta = match key.code {
        KeyCode::Up => -1,
        KeyCode::Down => 1,
        KeyCode::PageUp => -(HELP_PAGE_STEP as isize),
        KeyCode::PageDown => HELP_PAGE_STEP as isize,
        _ => return InputDispatch::Unhandled,
    };
    app.scroll_cpu_details(delta);
    InputDispatch::Consumed
}

fn table_navigation_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    navigation::handle_table_navigation(app, key)
}

fn nonflat_navigation_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if app.page() != AppPage::Applications {
        return InputDispatch::Unhandled;
    }
    match key.code {
        KeyCode::Up => InputDispatch::consumed(app.move_nonflat_selection_oneshot(-1)),
        KeyCode::Down => InputDispatch::consumed(app.move_nonflat_selection_oneshot(1)),
        KeyCode::Enter | KeyCode::Right => expand_nonflat_row(app),
        KeyCode::Left => collapse_nonflat_row(app),
        _ => InputDispatch::Unhandled,
    }
}

fn expand_nonflat_row(app: &mut TuiApp) -> InputDispatch {
    let tree_pid = {
        let rows = app.process_rows_snapshot();
        crate::process_view::category_tree_children_at(&rows, app.selected)
    };
    if let Some(identity) = tree_pid {
        app.expand_tree_identity(identity);
        return InputDispatch::Consumed;
    }
    let name = {
        let rows = app.process_rows_snapshot();
        crate::process_view::group_name_at(&rows, app.selected).map(str::to_owned)
    };
    name.map_or(InputDispatch::Unhandled, |name| {
        app.toggle_group_named(name);
        InputDispatch::Consumed
    })
}

fn collapse_nonflat_row(app: &mut TuiApp) -> InputDispatch {
    let identity = {
        let rows = app.process_rows_snapshot();
        crate::process_view::category_tree_children_at(&rows, app.selected)
    };
    identity.map_or(InputDispatch::Unhandled, |identity| {
        app.collapse_tree_identity(identity);
        InputDispatch::Consumed
    })
}

fn feedback_notice_dismiss_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    if key.code == KeyCode::Esc
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        && app.shell.feedback_notice().is_some()
    {
        app.shell.clear_feedback_notice();
        return InputDispatch::Consumed;
    }
    InputDispatch::Unhandled
}

fn content_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    match key.code {
        KeyCode::Tab if app.page() == AppPage::Applications => {
            app.cycle_focus_panel();
            InputDispatch::Consumed
        }
        KeyCode::Up => move_flat_selection(app, -1),
        KeyCode::Down => move_flat_selection(app, 1),
        KeyCode::Enter => run_registry_shortcut(
            app,
            crate::command_palette::ROW_TARGET_SHORTCUT,
            None,
            Modifiers::NONE,
        ),
        KeyCode::F(1)
            if matches!(
                app.input_scope(),
                TuiInputScope::Content | TuiInputScope::Help
            ) =>
        {
            app.toggle_help();
            InputDispatch::Consumed
        }
        KeyCode::F(9) => InputDispatch::Consumed,
        _ => key_to_terminal(*key).map_or(InputDispatch::Unhandled, |event| {
            app.handle_local_key(event)
        }),
    }
}

fn move_flat_selection(app: &mut TuiApp, delta: isize) -> InputDispatch {
    app.detail_scroll_reset();
    app.move_selection(delta);
    InputDispatch::consumed(app.refresh_selected_process_insights())
}

fn modifiers(key: &KeyEvent) -> Modifiers {
    Modifiers::new(
        key.modifiers.contains(KeyModifiers::CONTROL),
        key.modifiers.contains(KeyModifiers::ALT),
        key.modifiers.contains(KeyModifiers::SHIFT),
        key.modifiers.contains(KeyModifiers::SUPER),
    )
}

fn info_table_for_page(page: AppPage) -> Option<InfoTable> {
    match page {
        AppPage::Services => Some(InfoTable::Services),
        AppPage::Startup => Some(InfoTable::Startup),
        AppPage::Users => Some(InfoTable::Users),
        AppPage::Applications | AppPage::Performance | AppPage::System | AppPage::AppHistory => {
            None
        }
    }
}
