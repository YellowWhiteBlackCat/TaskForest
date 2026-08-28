//! Shared accessibility behavior exercised through GPUI's real focus dispatch.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, Keystroke,
    Modifiers, MouseButton, ParentElement, Render, TestAppContext, VisualTestContext, Window,
    WindowHandle, div, point, px, size,
};

use taskmanager_gpui::gpui_app::elements;
use taskmanager_gpui::gpui_app::root::{InputModality, RootView, TopPage};
use taskmanager_gpui::gpui_app::theme::Theme;
use taskmanager_ui::inputs::switch::{Switch, SwitchState};

/// The harness window root is our own RootView directly (P4 consumption switch:
/// the gc Root wrapper is gone; the LayerStack overlay host lives inside
/// RootView, so no separate overlay entity is needed here).
fn wrapped_root(cx: &mut TestAppContext) -> (WindowHandle<RootView>, Entity<RootView>) {
    let window = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = window.entity(cx).expect("window root RootView entity");
    (window, view)
}

/// Production `taskmanager_ui` Switch (the app's actual control since the P6
/// consumption sweep) exercised through GPUI's real focus dispatch.
struct SwitchFixture {
    state: Entity<SwitchState>,
    checked: Rc<Cell<bool>>,
    theme: Theme,
}

struct ToolButtonFixture {
    sentinel: FocusHandle,
    theme: Theme,
}

impl Render for ToolButtonFixture {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(elements::tool_btn(
                &self.theme,
                "disabled-action",
                "Disabled",
                false,
                false,
                |_, _| panic!("disabled control must remain inert"),
                |_, _, _| {},
            ))
            .child(elements::tool_btn(
                &self.theme,
                "enabled-action",
                "Enabled",
                true,
                false,
                |_, _| {},
                |_, _, _| {},
            ))
            .child(
                div()
                    .id("focus-order-sentinel")
                    .track_focus(&self.sentinel)
                    .tab_stop(true),
            )
    }
}

impl Render for SwitchFixture {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let checked = self.checked.clone();
        Switch::new(self.state.clone(), self.theme.palette()).on_change(move |on, _, _| {
            checked.set(on);
        })
    }
}

#[gpui::test]
async fn shared_switch_is_tab_reachable_and_uses_unmodified_enter_or_space(
    cx: &mut TestAppContext,
) {
    let checked = Rc::new(Cell::new(false));
    let fixture_checked = checked.clone();
    let window = cx.add_window(move |_window, cx| {
        let state = cx.new(|cx| SwitchState::new(cx));
        SwitchFixture {
            state,
            checked: fixture_checked,
            theme: Theme::dark(),
        }
    });

    for viewport in [size(px(1180.0), px(780.0)), size(px(720.0), px(480.0))] {
        cx.simulate_window_resize(window.into(), viewport);
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }
    window
        .update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();
    assert!(
        window
            .update(cx, |_view, window, cx| window.focused(cx).is_some())
            .unwrap(),
        "the shared switch wrapper must be a real GPUI tab stop"
    );

    cx.dispatch_keystroke(window.into(), Keystroke::parse("enter").unwrap());
    assert!(checked.get(), "Enter must toggle the focused shared switch");

    cx.dispatch_keystroke(window.into(), Keystroke::parse("space").unwrap());
    assert!(
        !checked.get(),
        "Space must toggle the focused shared switch"
    );

    cx.dispatch_keystroke(window.into(), Keystroke::parse("ctrl-space").unwrap());
    assert!(
        !checked.get(),
        "modified Space belongs to the application shortcut layer"
    );
}

#[gpui::test]
async fn shared_action_excludes_disabled_control_from_tab_order(cx: &mut TestAppContext) {
    let window = cx.add_window(move |_window, cx| ToolButtonFixture {
        sentinel: cx.focus_handle().tab_stop(true),
        theme: Theme::dark(),
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    window
        .update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();

    assert!(
        !window
            .update(cx, |view, window, _cx| view.sentinel.is_focused(window))
            .unwrap(),
        "the enabled action must be the first tab stop"
    );
    window
        .update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();
    assert!(
        window
            .update(cx, |view, window, _cx| view.sentinel.is_focused(window))
            .unwrap(),
        "the second Tab must reach the sentinel, proving the disabled action was skipped"
    );
}

/// Root capture must update modality before descendants handle focus, and the
/// state must remain isolated between windows that share one GPUI application.
#[gpui::test]
async fn mc07_focus_modality_case_focus_visible_modality_follows_keyboard_pointer_keyboard_per_window(
    cx: &mut TestAppContext,
) {
    let (window, view) = wrapped_root(cx);
    let (other, other_view) = wrapped_root(cx);

    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        cx.notify();
    });
    for target in [window.into(), other.into()] {
        cx.simulate_window_resize(target, size(px(1180.0), px(780.0)));
        cx.update_window(target, |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }

    assert_eq!(
        view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Programmatic
    );
    window
        .update(cx, |_view, window, _cx| {
            // The window root is RootView itself now (no gc Root wrapper); the
            // two focus_next calls land inside its rendered subtree, where root
            // capture participates.
            window.focus_next();
            window.focus_next();
        })
        .unwrap();
    assert_eq!(
        view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Programmatic,
        "programmatic focus must not opt into focus-visible"
    );
    assert!(
        !view.read_with(cx, |view, _cx| view.input_modality.shows_focus_ring()),
        "programmatic focus must keep the ring suppressed"
    );

    cx.dispatch_keystroke(window.into(), Keystroke::parse("tab").unwrap());
    assert_eq!(
        view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Keyboard
    );
    assert!(view.read_with(cx, |view, _cx| view.input_modality.shows_focus_ring()));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    // P6: the gc-global ring token is gone; ring visibility now derives from
    // the per-frame focus-visible snapshot (`with_focus_visible` →
    // `palette().ring`). Pin the derivation here; the per-window modality
    // assertions below cover the isolation the old token plumbing provided.
    let ring_for = |cx: &TestAppContext, view: &Entity<RootView>, focus_visible: bool| {
        view.read_with(cx, |view, _| {
            view.theme
                .with_focus_visible(focus_visible)
                .palette()
                .ring
                .a
        })
    };
    assert!(
        ring_for(cx, &view, true) > 0.0,
        "keyboard focus must show the ring token"
    );
    assert_eq!(
        ring_for(cx, &view, false),
        0.0,
        "pointer/programmatic focus must keep the ring token transparent"
    );

    // The untouched window keeps its own modality; rendering it must not leak
    // the other window's keyboard state into its snapshot.
    cx.update_window(other.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert_eq!(
        other_view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Programmatic,
        "the untouched window must keep its Programmatic policy"
    );
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert_eq!(
        view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Keyboard
    );

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_mouse_down(
        point(px(400.0), px(300.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    drop(visual);
    assert_eq!(
        view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Pointer
    );
    assert!(
        !view.read_with(cx, |view, _cx| view.input_modality.shows_focus_ring()),
        "pointer focus must suppress the shared ring"
    );
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert_eq!(
        ring_for(cx, &view, false),
        0.0,
        "pointer render must keep the ring suppressed"
    );

    cx.dispatch_keystroke(window.into(), Keystroke::parse("shift-tab").unwrap());
    assert_eq!(
        view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Keyboard
    );
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert!(
        ring_for(cx, &view, true) > 0.0,
        "keyboard navigation after a pointer click must show the ring"
    );
    assert_eq!(
        other_view.read_with(cx, |view, _cx| view.input_modality),
        InputModality::Programmatic,
        "input modality must never leak between windows"
    );
}
