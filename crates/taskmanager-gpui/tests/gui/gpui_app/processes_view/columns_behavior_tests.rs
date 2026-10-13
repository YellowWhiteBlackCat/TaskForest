use super::*;
use crate::gpui_app::root::TopPage;
use gpui::{Bounds, Modifiers, TestAppContext, VisualTestContext, point, px};
use taskmanager_theme::Theme;
use taskmanager_ui::overlays::popup::init;

#[gpui::test]
async fn process_column_picker_keeps_checkboxes_open_and_restores_trigger_focus(
    cx: &mut TestAppContext,
) {
    cx.update(init);
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let root = window.entity(cx).expect("root view must exist");
    root.update(cx, |view, cx| {
        view.page = TopPage::Apps;
        view.mark_telemetry_frame_ready();
        cx.notify();
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    draw(&mut vcx);

    let trigger = vcx
        .debug_bounds("columns-trigger")
        .expect("column picker trigger must render");
    vcx.simulate_click(trigger.center(), Modifiers::none());
    draw(&mut vcx);
    let popup = vcx.debug_bounds("tm-popup").expect("column menu must open");
    let toggles: Vec<_> = crate::gpui_app::processes_view::rows::columns()
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(ix, column)| {
            crate::gpui_app::processes_view::rows::is_hideable(column).then_some((ix, column))
        })
        .take(2)
        .collect();
    assert_eq!(toggles.len(), 2, "the picker must expose multiple toggles");
    let first_before = root.read_with(&vcx, |view, _| {
        view.processes_state.hidden_cols.contains(&toggles[0].1)
    });
    let second_before = root.read_with(&vcx, |view, _| {
        view.processes_state.hidden_cols.contains(&toggles[1].1)
    });

    click_column(&mut vcx, popup, toggles[0].0);
    draw(&mut vcx);
    let first_after = root.read_with(&vcx, |view, _| {
        view.processes_state.hidden_cols.contains(&toggles[0].1)
    });
    assert_ne!(
        first_after, first_before,
        "the first checked choice must update the process column projection"
    );

    click_column(&mut vcx, popup, toggles[1].0);
    draw(&mut vcx);
    let second_after = root.read_with(&vcx, |view, _| {
        view.processes_state.hidden_cols.contains(&toggles[1].1)
    });
    assert_ne!(
        second_after, second_before,
        "the second choice must still be actionable in the same popup"
    );

    vcx.simulate_keystrokes("escape");
    draw(&mut vcx);
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("Enter must reopen the menu");
    click_column(&mut vcx, popup, toggles[0].0);
    draw(&mut vcx);
    assert_eq!(
        root.read_with(&vcx, |view, _| view
            .processes_state
            .hidden_cols
            .contains(&toggles[0].1)),
        first_before,
        "Escape must restore trigger focus so Enter can reopen and toggle again"
    );

    vcx.simulate_click(point(px(900.0), px(500.0)), Modifiers::none());
    draw(&mut vcx);
    assert_eq!(
        root.read_with(&vcx, |view, _| view
            .processes_state
            .hidden_cols
            .contains(&toggles[1].1)),
        second_after,
        "outside dismissal must not run a checkbox action"
    );
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("outside dismissal must restore trigger focus");
    click_column(&mut vcx, popup, toggles[1].0);
    draw(&mut vcx);
    assert_eq!(
        root.read_with(&vcx, |view, _| view
            .processes_state
            .hidden_cols
            .contains(&toggles[1].1)),
        second_before,
        "the trigger must remain keyboard-activatable after outside dismissal"
    );
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click_column(vcx: &mut VisualTestContext, popup: Bounds<gpui::Pixels>, row: usize) {
    let y = popup.top() + px(17.0 + 26.0 * row as f32);
    vcx.simulate_click(point(popup.left() + px(40.0), y), Modifiers::none());
}
