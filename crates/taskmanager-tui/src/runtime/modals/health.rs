//! Health-modal input owns its review, editing and transfer gestures.
use crate::TuiApp;
use crate::command_palette::{TuiSurfaceScope, surface_protocol_action};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
pub(super) fn handle(app: &mut TuiApp, key: KeyEvent) {
    if app.health_review.mode == crate::health_review::HealthReviewMode::Events {
        match key.code {
            KeyCode::F(2) => {
                app.filter_event_history();
                return;
            }
            KeyCode::F(5) => {
                app.export_event_history_to(&mut std::io::stdout());
                return;
            }
            KeyCode::F(8) => {
                app.shell.clear_alert_event_history();
                app.health_review.selected = 0;
                return;
            }
            _ => {}
        }
    }
    if app.health_review.mode != crate::health_review::HealthReviewMode::Rules {
        match key.code {
            ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
            ratatui::crossterm::event::KeyCode::Up => app.health_review_move(-1),
            ratatui::crossterm::event::KeyCode::Down => app.health_review_move(1),
            ratatui::crossterm::event::KeyCode::Home => app.health_review.selected = 0,
            ratatui::crossterm::event::KeyCode::End => app.health_review_move(isize::MAX),
            ratatui::crossterm::event::KeyCode::Char(character) => {
                if let Some(action) =
                    surface_protocol_action(TuiSurfaceScope::HealthReview, character).or_else(
                        || surface_protocol_action(TuiSurfaceScope::StatusOverlay, character),
                    )
                {
                    app.run_surface_protocol_action(action);
                }
            }
            _ => {}
        }
        return;
    }
    // Esc stays structural; the toggle chords resolve through the
    // declared surface protocol. In the health overlay, arrows and
    // Space / Enter navigate and toggle managed alert rules.
    match key.code {
        ratatui::crossterm::event::KeyCode::Esc => app.close_local_overlays(),
        ratatui::crossterm::event::KeyCode::Up | ratatui::crossterm::event::KeyCode::Char('k') => {
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
            if let Some(action) = surface_protocol_action(TuiSurfaceScope::HealthReview, character)
                .or_else(|| surface_protocol_action(TuiSurfaceScope::HealthRule, character))
                .or_else(|| surface_protocol_action(TuiSurfaceScope::StatusOverlay, character))
            {
                app.run_surface_protocol_action(action);
            }
        }
        _ => {}
    }
}
