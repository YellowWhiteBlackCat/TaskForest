//! Ordered TUI keyboard systems.
//!
//! Each system returns the shared explicit [`InputDispatch`] state. The
//! registry is the sole precedence authority: the first consumed route wins,
//! including routes that mutate only local state and emit no platform work.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use taskmanager_application::{AppPage, Modifiers, PlatformEffect};
use taskmanager_shell::{
    FeedbackLifecycle, FeedbackSeverity, FeedbackSource, InfoTable, InputDispatch,
};

use crate::{FocusPanel, PerfDevice, TuiApp, TuiInputScope};

use super::{HELP_PAGE_STEP, inline_network_escalation_ready, key_to_terminal, modals, navigation};

type KeySystem = fn(&mut TuiApp, &KeyEvent) -> InputDispatch;

/// Input precedence is data, not nesting. New input owners must be inserted in
/// this registry and return an explicit dispatch state.
const KEY_SYSTEMS: [KeySystem; 10] = [
    open_modal_system,
    owned_input_system,
    character_system,
    details_focus_system,
    selection_extension_system,
    detail_scroll_system,
    performance_scroll_system,
    table_navigation_system,
    nonflat_navigation_system,
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

fn route_shared_surface(
    app: &mut TuiApp,
    kind: taskmanager_application::SurfaceKind,
    key: &KeyEvent,
) -> InputDispatch {
    match kind {
        taskmanager_application::SurfaceKind::Confirmation(_) => match key.code {
            KeyCode::Char(character) => app.handle_local_char(character, modifiers(key)),
            KeyCode::Esc => {
                app.shell.dismiss_overlay();
                InputDispatch::Consumed
            }
            _ => InputDispatch::Consumed,
        },
        taskmanager_application::SurfaceKind::ProcessProperties => {
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

const CHARACTER_SYSTEMS: [CharacterSystem; 11] = [
    app_history_window_system,
    performance_digit_system,
    sort_system,
    palette_system,
    source_retry_system,
    shell_character_system,
    utility_character_system,
    application_character_system,
    service_character_system,
    performance_character_system,
    prefix_jump_system,
];

fn app_history_window_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    if app.page() != AppPage::AppHistory || modifiers != Modifiers::NONE {
        return InputDispatch::Unhandled;
    }
    let window = match character {
        '1' => taskmanager_application::HistoryWindow::OneHour,
        '2' => taskmanager_application::HistoryWindow::TwentyFourHours,
        '3' => taskmanager_application::HistoryWindow::SevenDays,
        _ => return InputDispatch::Unhandled,
    };
    let _ = app.select_application_history_window(window);
    InputDispatch::Consumed
}

fn performance_digit_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    if app.page() != AppPage::Performance
        || modifiers.control
        || modifiers.alt
        || modifiers.platform
    {
        return InputDispatch::Unhandled;
    }
    let Some(device) = app.select_perf_device_digit(character) else {
        return InputDispatch::Unhandled;
    };
    app.select_perf_device(device);
    InputDispatch::Consumed
}

fn sort_system(app: &mut TuiApp, character: char, modifiers: Modifiers) -> InputDispatch {
    if !matches!(character, 's' | 'S') || modifiers.control || modifiers.alt {
        return InputDispatch::Unhandled;
    }
    if let Some(table) = info_table_for_page(app.page()) {
        if character == 's' {
            app.shell.cycle_info_sort_column(table);
        } else {
            app.shell.toggle_info_sort_direction(table);
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

fn utility_character_system(
    app: &mut TuiApp,
    character: char,
    _modifiers: Modifiers,
) -> InputDispatch {
    match character {
        'p' => app.toggle_settings(),
        'i' => app.toggle_about(),
        'h' => app.toggle_health(),
        'c' => app.toggle_containers(),
        'x' => app.export_snapshot(),
        _ => return InputDispatch::Unhandled,
    }
    InputDispatch::Consumed
}

fn application_character_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    if app.page() != AppPage::Applications || modifiers.control || modifiers.alt {
        return InputDispatch::Unhandled;
    }
    match character {
        'a' => {
            let _ = app.open_process_menu();
        }
        'C' => app.toggle_column_menu(),
        'm' => toggle_marked_process(app),
        'B' => {
            let _ = app.open_batch_menu();
        }
        'y' if !app.search_active() => app.copy_selected_process(&mut std::io::stdout()),
        'e' if inline_network_escalation_ready(app) => {
            return InputDispatch::Effect(Box::new(
                taskmanager_shell::ShellApp::request_process_network_escalation(),
            ));
        }
        _ => return InputDispatch::Unhandled,
    }
    InputDispatch::Consumed
}

fn toggle_marked_process(app: &mut TuiApp) {
    if let Some(process) = app.selected_detail_process() {
        let pid = process.pid;
        app.shell.toggle_selected_pid(pid);
    }
    let marked = app.shell.selected_pids().len();
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

fn service_character_system(
    app: &mut TuiApp,
    character: char,
    _modifiers: Modifiers,
) -> InputDispatch {
    if app.page() != AppPage::Services || character != 'o' || app.shell.service_log.is_some() {
        return InputDispatch::Unhandled;
    }
    InputDispatch::consumed(app.shell.open_service_log())
}

fn performance_character_system(
    app: &mut TuiApp,
    character: char,
    modifiers: Modifiers,
) -> InputDispatch {
    if app.page() != AppPage::Performance || modifiers.control || modifiers.alt {
        return InputDispatch::Unhandled;
    }
    match (character, app.perf_device) {
        ('e', PerfDevice::Gpu) => InputDispatch::consumed(app.toggle_gpu_engine_rows()),
        ('d', PerfDevice::Disk) => InputDispatch::consumed(app.toggle_directory_scan()),
        // GPU headline-chart metric cycle (ADR-034 stage 2): advances the
        // shared shell selection through the fixed vocabulary order, gated
        // by the viewed device's typed availability. `g` is free on the
        // Performance page (the router binds no bare `g` chord) and the
        // earlier scopes above already returned for search/modal typing.
        ('g', PerfDevice::Gpu) => {
            app.cycle_gpu_chart_metric();
            InputDispatch::Consumed
        }
        _ => InputDispatch::Unhandled,
    }
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
    let previously_marked = app.shell.selected_pids().clone();
    let effect = app.move_nonflat_selection_oneshot(delta);
    app.shell.selected_pids.extend(previously_marked);
    if let Some(process) = app.selected_detail_process() {
        app.shell.selected_pids.insert(process.pid);
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
    if let Some(pid) = tree_pid {
        app.expand_tree_pid(pid);
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
    let pid = {
        let rows = app.process_rows_snapshot();
        crate::process_view::category_tree_children_at(&rows, app.selected)
    };
    pid.map_or(InputDispatch::Unhandled, |pid| {
        app.collapse_tree_pid(pid);
        InputDispatch::Consumed
    })
}

fn content_system(app: &mut TuiApp, key: &KeyEvent) -> InputDispatch {
    match key.code {
        KeyCode::Tab if app.page() == AppPage::Applications => {
            app.cycle_focus_panel();
            InputDispatch::Consumed
        }
        KeyCode::Up => move_flat_selection(app, -1),
        KeyCode::Down => move_flat_selection(app, 1),
        KeyCode::Enter => open_page_target(app),
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

fn open_page_target(app: &mut TuiApp) -> InputDispatch {
    match app.page() {
        AppPage::Services => {
            let _ = app.open_service_menu();
        }
        AppPage::Users => {
            let _ = app.open_session_menu();
        }
        AppPage::Startup => {
            let _ = app.open_startup_menu();
        }
        AppPage::Applications => {
            let _ = app.open_process_properties();
        }
        AppPage::Performance | AppPage::System | AppPage::AppHistory => {
            return InputDispatch::Unhandled;
        }
    }
    InputDispatch::Consumed
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
