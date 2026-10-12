//! Health-modal input owns review navigation and managed-rule toggles.
use crate::TuiApp;
use crate::command_palette::{TuiSurfaceScope, surface_protocol_action};
use crate::health_review::HealthReviewMode;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
pub(super) fn handle(app: &mut TuiApp, key: KeyEvent) {
    if app.health_review.mode == HealthReviewMode::Events {
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
    if app.health_review.mode != HealthReviewMode::Rules {
        match key.code {
            KeyCode::Esc => app.close_local_overlays(),
            KeyCode::Up => app.health_review_move(-1),
            KeyCode::Down => app.health_review_move(1),
            KeyCode::Home => app.health_review.selected = 0,
            KeyCode::End => app.health_review_move(isize::MAX),
            KeyCode::Char(character) => {
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
        KeyCode::Esc => app.close_local_overlays(),
        KeyCode::Up | KeyCode::Char('k') => {
            app.health_rule_move(-1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.health_rule_move(1);
        }
        KeyCode::Home => {
            app.health_rule_selection = 0;
        }
        KeyCode::End => {
            let count = app.projection().alert_center.managed_rules().len();
            app.health_rule_selection = count.saturating_sub(1);
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            app.toggle_selected_alert_rule();
        }
        KeyCode::Char(character) => {
            if let Some(action) = surface_protocol_action(TuiSurfaceScope::HealthReview, character)
                .or_else(|| surface_protocol_action(TuiSurfaceScope::StatusOverlay, character))
            {
                app.run_surface_protocol_action(action);
            }
        }
        _ => {}
    }
}
