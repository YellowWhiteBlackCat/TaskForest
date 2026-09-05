//! TUI-local modal key traps (ADR-027).
//!
//! Every open TUI modal owns ALL keys while it is up: the command palette,
//! the column-visibility menu, the service/process/session/startup action
//! menus, the Process Properties modal, the settings form, the
//! about/health/containers overlays, and the open service-log panel. They are
//! the top of the modal precedence stack, above the shell's end-task/help/
//! suggestions states. `handle_open_modal` returns `None` when no modal is
//! open (the caller falls through to the shared key path) and
//! `Some(effect)` when a modal consumed the key — `effect` is the optional
//! [`PlatformEffect`] the modal produced (e.g. palette Enter runs a shared
//! action). Extracted from `runtime.rs` so no runtime file exceeds the source
//! line budget; behavior unchanged.
//!
//! The action-semantic character chords of the status overlays and the
//! service-log panel are declared in
//! [`crate::command_palette::TUI_SURFACE_PROTOCOL`] and resolve through it;
//! this module owns only their precedence, the structural keys (Esc, the
//! panel's `q` close, the menus' navigation), and the consumption rule
//! (full modals swallow every key; the panel is a partial owner).

use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
use taskmanager_application::AppPage;
use taskmanager_shell::InputDispatch;

use crate::command_palette::{TuiSurfaceScope, surface_protocol_action};
use crate::{TuiApp, TuiSurfaceKind};

use super::handle_settings_key;

/// Route one key through the open TUI-local modals, highest-precedence first.
/// `Unhandled` means no modal was open. Every full modal consumes every key;
/// the service-log panel consumes only its documented control chords.
#[must_use]
pub(super) fn handle_open_modal(app: &mut TuiApp, key: KeyEvent) -> InputDispatch {
    if app.process_properties().is_some() {
        // The Process Properties modal traps navigation while open: Tab and
        // Left/Right cycle the four sections (Overview / Performance / Command /
        // Insights), Esc closes, and Up / Down / Ctrl+Up / Ctrl+Down scroll the
        // tab body so a short terminal can still reach every row. Arrows never
        // reach the table cursor, so the modal is the sole owner of those keys
        // (mirrors the menu trap above). The renderer clamps the offset, so
        // scroll-by only stores the user's intent.
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let effect = match key.code {
            ratatui::crossterm::event::KeyCode::Tab | ratatui::crossterm::event::KeyCode::Right => {
                app.process_properties_next_tab();
                None
            }
            ratatui::crossterm::event::KeyCode::BackTab
            | ratatui::crossterm::event::KeyCode::Left => {
                app.process_properties_prev_tab();
                None
            }
            ratatui::crossterm::event::KeyCode::Up if ctrl => {
                app.process_properties_scroll_by(-1);
                None
            }
            ratatui::crossterm::event::KeyCode::Down if ctrl => {
                app.process_properties_scroll_by(1);
                None
            }
            ratatui::crossterm::event::KeyCode::Up => {
                app.process_properties_scroll_by(-1);
                None
            }
            ratatui::crossterm::event::KeyCode::Down => {
                app.process_properties_scroll_by(1);
                None
            }
            // Per-process network escalation trigger (G-04b): `e` on the
            // Insights tab fires the shared one-shot escalation request when
            // — and only when — the projected network facet reports the typed
            // `RequiresEscalation` state (the same gate as the rendered
            // hint), mirroring GPUI's "Enable per-process network" pill. The
            // effect returns to the runtime loop for `queue_effect` routing.
            ratatui::crossterm::event::KeyCode::Char('e')
                if !ctrl
                    && app.process_properties().is_some_and(|target| {
                        target.section == crate::ProcessDetailsSection::Insights
                            && crate::ui::process_details::network_requires_escalation(
                                app,
                                target.item.pid,
                            )
                    }) =>
            {
                return InputDispatch::Effect(Box::new(
                    taskmanager_shell::ShellApp::request_process_network_escalation(),
                ));
            }
            ratatui::crossterm::event::KeyCode::Esc => {
                app.close_local_overlays();
                None
            }
            _ => None,
        };
        return InputDispatch::consumed(effect);
    }

    if let Some(surface) = app.local_surface_kind() {
        let effect = match surface {
            TuiSurfaceKind::CommandPalette => match key.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    app.close_command_palette();
                    None
                }
                ratatui::crossterm::event::KeyCode::Backspace => {
                    app.palette_backspace();
                    None
                }
                ratatui::crossterm::event::KeyCode::Up => {
                    app.palette_move(-1);
                    None
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    app.palette_move(1);
                    None
                }
                ratatui::crossterm::event::KeyCode::Enter => app.palette_select(),
                ratatui::crossterm::event::KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    app.palette_push_char(character);
                    None
                }
                _ => None,
            },
            TuiSurfaceKind::ColumnMenu => {
                match key.code {
                    ratatui::crossterm::event::KeyCode::Up => app.column_menu_move(-1),
                    ratatui::crossterm::event::KeyCode::Down => app.column_menu_move(1),
                    ratatui::crossterm::event::KeyCode::Enter
                    | ratatui::crossterm::event::KeyCode::Char(' ') => app.column_menu_toggle(),
                    ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
                    _ => {}
                }
                None
            }
            TuiSurfaceKind::ServiceMenu => {
                match key.code {
                    ratatui::crossterm::event::KeyCode::Up => app.service_menu_move(-1),
                    ratatui::crossterm::event::KeyCode::Down => app.service_menu_move(1),
                    ratatui::crossterm::event::KeyCode::Enter => app.service_menu_select(),
                    ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
                    _ => {}
                }
                None
            }
            TuiSurfaceKind::ProcessMenu => match key.code {
                ratatui::crossterm::event::KeyCode::Up => {
                    app.process_menu_move(-1);
                    None
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    app.process_menu_move(1);
                    None
                }
                ratatui::crossterm::event::KeyCode::Enter => app.process_menu_select(),
                ratatui::crossterm::event::KeyCode::Char('a' | 'A') => {
                    let target = app.process_menu_mut().and_then(|m| {
                        taskmanager_core::core::process::FrozenProcessIdentity::from_process(
                            &m.item,
                        )
                    });
                    match target {
                        Some(target) => app.open_process_affinity_for(target),
                        None => app.open_process_affinity(),
                    }
                }
                ratatui::crossterm::event::KeyCode::Esc => {
                    app.close_local_overlays();
                    None
                }
                _ => None,
            },
            TuiSurfaceKind::BatchMenu => match key.code {
                ratatui::crossterm::event::KeyCode::Up => {
                    app.batch_menu_move(-1);
                    None
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    app.batch_menu_move(1);
                    None
                }
                ratatui::crossterm::event::KeyCode::Enter => app.batch_menu_select(),
                ratatui::crossterm::event::KeyCode::Esc => {
                    app.close_local_overlays();
                    None
                }
                _ => None,
            },
            TuiSurfaceKind::SessionMenu => {
                match key.code {
                    ratatui::crossterm::event::KeyCode::Up => app.session_menu_move(-1),
                    ratatui::crossterm::event::KeyCode::Down => app.session_menu_move(1),
                    ratatui::crossterm::event::KeyCode::Enter => app.session_menu_select(),
                    ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
                    _ => {}
                }
                None
            }
            TuiSurfaceKind::StartupMenu => {
                match key.code {
                    ratatui::crossterm::event::KeyCode::Up => app.startup_menu_move(-1),
                    ratatui::crossterm::event::KeyCode::Down => app.startup_menu_move(1),
                    ratatui::crossterm::event::KeyCode::Enter => app.startup_menu_select(),
                    ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
                    _ => {}
                }
                None
            }
            TuiSurfaceKind::ServiceDependencies => {
                match key.code {
                    ratatui::crossterm::event::KeyCode::Up
                    | ratatui::crossterm::event::KeyCode::Char('k') => {
                        app.service_dependencies_scroll(-1);
                    }
                    ratatui::crossterm::event::KeyCode::Down
                    | ratatui::crossterm::event::KeyCode::Char('j') => {
                        app.service_dependencies_scroll(1);
                    }
                    ratatui::crossterm::event::KeyCode::Esc
                    | ratatui::crossterm::event::KeyCode::Char('q')
                    | ratatui::crossterm::event::KeyCode::Char('d') => {
                        app.close_local_overlays();
                    }
                    _ => {}
                }
                None
            }
            TuiSurfaceKind::ProcessAffinity => match key.code {
                ratatui::crossterm::event::KeyCode::Up => {
                    if let Some(state) = app.process_affinity_mut() {
                        state.move_up();
                    }
                    None
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    if let Some(state) = app.process_affinity_mut() {
                        state.move_down();
                    }
                    None
                }
                ratatui::crossterm::event::KeyCode::Left => {
                    if let Some(state) = app.process_affinity_mut() {
                        state.move_left();
                    }
                    None
                }
                ratatui::crossterm::event::KeyCode::Right => {
                    if let Some(state) = app.process_affinity_mut() {
                        state.move_right();
                    }
                    None
                }
                ratatui::crossterm::event::KeyCode::Char(' ') => {
                    if let Some(state) = app.process_affinity_mut() {
                        state.toggle_selected();
                    }
                    None
                }
                ratatui::crossterm::event::KeyCode::Char('a' | 'A') => {
                    if let Some(state) = app.process_affinity_mut() {
                        state.toggle_all();
                    }
                    None
                }
                ratatui::crossterm::event::KeyCode::Enter => app.apply_process_affinity(),
                ratatui::crossterm::event::KeyCode::Esc => {
                    app.close_local_overlays();
                    None
                }
                _ => None,
            },
            TuiSurfaceKind::Settings => {
                handle_settings_key(app, key);
                None
            }
            TuiSurfaceKind::About | TuiSurfaceKind::Containers => {
                // Esc stays structural; the toggle chords resolve through the
                // declared surface protocol. The full modal consumes every
                // key, so an unmatched character is a silent no-op and can
                // never double-route a command chord.
                match key.code {
                    ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
                    ratatui::crossterm::event::KeyCode::Char(character) => {
                        if let Some(action) =
                            surface_protocol_action(TuiSurfaceScope::StatusOverlay, character)
                        {
                            app.run_surface_protocol_action(action);
                        }
                    }
                    _ => {}
                }
                None
            }
            TuiSurfaceKind::Health => {
                // Esc stays structural; the toggle chords resolve through the
                // declared surface protocol. In the health overlay, arrows and
                // Space / Enter navigate and toggle managed alert rules.
                match key.code {
                    ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
                    ratatui::crossterm::event::KeyCode::Up
                    | ratatui::crossterm::event::KeyCode::Char('k') => {
                        app.health_rule_move(-1);
                    }
                    ratatui::crossterm::event::KeyCode::Down
                    | ratatui::crossterm::event::KeyCode::Char('j') => {
                        app.health_rule_move(1);
                    }
                    ratatui::crossterm::event::KeyCode::Home => {
                        app.health_rule_selection = 0;
                    }
                    ratatui::crossterm::event::KeyCode::End => {
                        let count = app.projection().alert_center.managed_rules().len();
                        app.health_rule_selection = count.saturating_sub(1);
                    }
                    ratatui::crossterm::event::KeyCode::Enter
                    | ratatui::crossterm::event::KeyCode::Char(' ') => {
                        app.toggle_selected_alert_rule();
                    }
                    ratatui::crossterm::event::KeyCode::Char(character) => {
                        if let Some(action) =
                            surface_protocol_action(TuiSurfaceScope::StatusOverlay, character)
                        {
                            app.run_surface_protocol_action(action);
                        }
                    }
                    _ => {}
                }
                None
            }
        };
        return InputDispatch::consumed(effect);
    }

    // The open service-log panel owns its control chords while the Services
    // page shows it: f follow, p pause, l level cycle, t time cycle, q/Esc
    // close. The `f p l t` action chords are declared in
    // [`crate::command_palette::TUI_SURFACE_PROTOCOL`]; `q`/Esc stay the
    // handwritten structural close. The shell owns the actual state
    // transitions; the protocol executor only calls them. Unclaimed keys
    // fall THROUGH to the shared key path (the panel is a partial owner,
    // unlike the full modals above).
    if app.page() == AppPage::Services && app.shell.service_log.is_some() {
        match key.code {
            ratatui::crossterm::event::KeyCode::Esc
            | ratatui::crossterm::event::KeyCode::Char('q') => {
                app.shell.close_service_log();
                return InputDispatch::Consumed;
            }
            ratatui::crossterm::event::KeyCode::Char(character) => {
                return match surface_protocol_action(TuiSurfaceScope::ServiceLogPanel, character) {
                    Some(action) => {
                        app.run_surface_protocol_action(action);
                        InputDispatch::Consumed
                    }
                    None => InputDispatch::Unhandled,
                };
            }
            _ => return InputDispatch::Unhandled,
        }
    }

    InputDispatch::Unhandled
}

/// Whether ANY modal, overlay, confirmation gate, search field, or details
/// panel currently owns the interactive surface. While this is true a
/// pointer click must be a no-op: the click would address the table under a
/// surface that owns the keyboard, creating a second selection semantics.
/// Mirrors the precedence list `handle_open_modal` + the shell's own modal
/// states; kept beside it so the two lists are reviewed together.
pub(super) fn any_pointer_surface_open(app: &TuiApp) -> bool {
    app.input_scope().blocks_pointer()
}
