//! test-intent: behavior
//! The actual root plan reserves a nonoverlapping navigation slot and protected page edges.
use super::*;
use crate::ui::TuiFramePlan;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use taskmanager_application::AppPage;
#[test]
fn native_vertical_navigation_budget_keeps_page_and_rail_inside_every_terminal_shape() {
    let mut app = crate::demo_app();
    crate::runtime::handle_key(&mut app, KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE));
    for (width, height) in [
        (54, 16),
        (80, 24),
        (120, 36),
        (180, 20),
        (54, 50),
        (200, 60),
    ] {
        let area = Rect::new(0, 0, width, height);
        let plan = TuiFramePlan::build(&app, area);
        let rail = plan.navigation.expect("rail");
        assert!(rail.right() <= plan.chrome.body.x);
        assert!(
            plan.chrome.body.right() <= area.right()
                && plan.chrome.body.bottom() <= plan.chrome.footer.y
        );
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| {
                crate::ui::render_with_plan(frame, &app, crate::TuiTheme::default(), &plan)
            })
            .expect("paint");
        for index in 0..AppPage::ALL.len() {
            let painted: Vec<_> = (rail.x..rail.right())
                .map(|x| terminal.backend().buffer()[(x, rail.y + index as u16)].symbol())
                .collect();
            assert!(
                painted.iter().any(|text| !text.trim().is_empty()),
                "whole visible native page row"
            );
        }
    }
}
