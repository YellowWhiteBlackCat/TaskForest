use super::{
    MODAL_CONTEXT, ModalEscTarget, begin_modal, ensure_support, esc_chain_target, restore_modal,
    trap_modal,
};
use gpui::{
    AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, Keystroke,
    ParentElement, Render, TestAppContext, Window, div,
};
#[test]
fn esc_chain_prioritizes_top_layer_kind() {
    assert_eq!(esc_chain_target(Some(false)), ModalEscTarget::Popup);
    assert_eq!(esc_chain_target(Some(true)), ModalEscTarget::Dialog);
    assert_eq!(esc_chain_target(None), ModalEscTarget::Window);
}

struct TestRoot {
    trigger_focus: FocusHandle,
    modal_scope: Option<FocusHandle>,
    modal_open: bool,
}

impl TestRoot {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            trigger_focus: cx.focus_handle(),
            modal_scope: None,
            modal_open: false,
        }
    }
}

impl Render for TestRoot {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let trigger = self.trigger_focus.clone().tab_stop(true);
        let mut root = div()
            .id("test-root")
            .child(div().id("trigger").track_focus(&trigger));
        if self.modal_open
            && let Some(scope) = &self.modal_scope
        {
            let panel = div()
                .id("modal-panel")
                .child(div().id("modal-focusable").tab_stop(true));
            root = root.child(trap_modal(panel, scope));
        }
        root
    }
}

/// End-to-end trap/restore with a real window: opening a modal scope
/// moves focus inside, Tab stays inside the modal context, and restore
/// puts focus back on the recorded trigger.
#[gpui::test]
async fn modal_trap_keeps_tab_inside_and_restores_trigger(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| TestRoot::new(cx));
    cx.update(ensure_support);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Focus the trigger via its handle (deterministic), then open a
    // modal scope: initial focus moves to the scope.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let trigger = window
        .read_with(&vcx, |root, _| root.trigger_focus.clone())
        .unwrap();
    vcx.update(|window, _| {
        window.activate_window();
        trigger.focus(window);
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    let trigger_was_focused = window
        .update(cx, |_, window, _| trigger.is_focused(window))
        .unwrap();
    assert!(
        trigger_was_focused,
        "trigger must be focused before modal opens"
    );
    let scope = window
        .update(cx, |_, window, cx| begin_modal(window, cx))
        .unwrap();
    let _ = window.update(cx, |root, _window, cx| {
        root.modal_scope = Some(scope);
        root.modal_open = true;
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let in_modal = window
        .update(cx, |_, window, _| {
            window
                .context_stack()
                .iter()
                .any(|c| c.contains(MODAL_CONTEXT))
        })
        .unwrap();
    assert!(in_modal, "opening a modal must enter the modal key context");

    // Tab / Shift-Tab must wrap inside the modal scope, never the inert page.
    for keystroke in ["tab", "shift-tab"] {
        for _ in 0..64 {
            let _ = window.update(cx, |_, window, cx| {
                window.dispatch_keystroke(Keystroke::parse(keystroke).unwrap(), cx);
            });
            let in_modal = window
                .update(cx, |_, window, _| {
                    window
                        .context_stack()
                        .iter()
                        .any(|c| c.contains(MODAL_CONTEXT))
                })
                .unwrap();
            assert!(in_modal, "{keystroke} must wrap inside the modal scope");
        }
    }

    // Closing restores the exact trigger.
    let _ = window.update(cx, |_, window, cx| restore_modal(window, cx));
    let _ = window.update(cx, |root, _window, cx| {
        root.modal_open = false;
        cx.notify();
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let trigger_focused = window
        .update(cx, |_, window, _| trigger.is_focused(window))
        .unwrap();
    assert!(
        trigger_focused,
        "restore must put focus back on the recorded trigger handle"
    );
}

/// A trigger view that can be destroyed (dropped) while a modal is open.
struct TriggerView {
    focus: FocusHandle,
}

impl TriggerView {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
}

impl Render for TriggerView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("trigger")
            .track_focus(&self.focus.clone().tab_stop(true))
    }
}

struct DestroyHarness {
    trigger: Option<Entity<TriggerView>>,
    modal_scope: Option<FocusHandle>,
    modal_open: bool,
}

impl Render for DestroyHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().id("test-root");
        if let Some(trigger) = &self.trigger {
            root = root.child(trigger.clone());
        }
        if self.modal_open
            && let Some(scope) = &self.modal_scope
        {
            let panel = div()
                .id("modal-panel")
                .child(div().id("modal-focusable").tab_stop(true));
            root = root.child(trap_modal(panel, scope));
        }
        root
    }
}

/// 附录 A-21 lock: destroying the trigger element while a modal is open
/// must not make `restore_modal` panic or restore focus onto the dead
/// handle — the restore-token/`blur` fallback leaves the window with no
/// focused element. (gc kept an unvalidated `previous_focus_handle`.)
#[gpui::test]
async fn restore_after_trigger_destroyed_blurs_instead_of_panicking(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| DestroyHarness {
        trigger: None,
        modal_scope: None,
        modal_open: false,
    });
    cx.update(ensure_support);

    // Install and focus the trigger, then open a modal scope: the scope
    // records the trigger handle as the restore target.
    let trigger = cx.new(TriggerView::new);
    window
        .update(cx, |harness, _window, cx| {
            harness.trigger = Some(trigger.clone());
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let handle = trigger.read_with(cx, |view, _| view.focus.clone());
    window
        .update(cx, |_, window, _| {
            window.activate_window();
            handle.focus(window);
        })
        .unwrap();
    drop(handle);
    let scope = window
        .update(cx, |_, window, cx| begin_modal(window, cx))
        .unwrap();
    window
        .update(cx, |harness, _window, cx| {
            harness.modal_scope = Some(scope);
            harness.modal_open = true;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Destroy the trigger: drop the entity and our clone; the frame
    // releases its focus ref on the next draw, leaving only the recorded
    // restore handle alive.
    drop(trigger);
    window
        .update(cx, |harness, _window, cx| {
            harness.trigger = None;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Closing the modal must not panic; the restore target is gone, so
    // the focus must end up blurred (never on the dead handle).
    window
        .update(cx, |_, window, cx| restore_modal(window, cx))
        .unwrap();
    window
        .update(cx, |harness, _window, cx| {
            harness.modal_open = false;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let focused = window
        .update(cx, |_, window, cx| window.focused(cx))
        .unwrap();
    assert!(
        focused.is_none(),
        "restoring onto a destroyed trigger must blur (A-21 fallback)"
    );
}
