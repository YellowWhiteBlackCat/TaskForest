//! test-intent: behavior

use super::*;
use gpui::AppContext;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::setup::SetupScriptEvent;
use taskmanager_platform_contract::{CapabilityId, EventSequence};
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_test_support::setup_script::platform;

#[gpui::test]
async fn background_setup_observation_stays_quiet_and_closed_action_does_not_reopen(
    cx: &mut gpui::TestAppContext,
) {
    let (platform, recorder) = platform();
    let root = cx.new(|cx| RootView::new(Theme::dark(), cx));
    root.update(cx, |view, cx| {
        view.platform = Some(platform);
        view.request_first_run_observation(cx);
        let request = recorder
            .submissions()
            .expect("recorded")
            .last()
            .expect("Observe")
            .id;
        let event = |request_id, event| CorrelatedSetupScriptEvent {
            request_id,
            capability: CapabilityId::FIRST_RUN_SETUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            event,
        };
        assert!(view.apply_first_run_event(
            event(
                request,
                SetupScriptEvent::Observed(Some(setup_script_info()))
            ),
            cx
        ));
        assert_eq!(view.first_run.view().phase, FirstRunPhase::Available);
        assert!(view.first_run.view().info.is_some());
        assert!(!view.first_run_open());
        view.show_first_run();
        assert!(view.first_run_open());
        assert!(view.request_first_run_action(SetupScriptAction::Run, cx));
        assert!(!view.request_first_run_action(SetupScriptAction::Revert, cx));
        let requests = recorder.submissions().expect("recorded");
        assert_eq!(requests.len(), 2, "one Observe and one explicit action");
        let request = requests.last().expect("Run").id;
        view.dismiss_window_surface(
            WindowSurfaceKind::FirstRun,
            WindowSurfaceDismissReason::Cancel,
        );
        assert!(view.apply_first_run_event(
            event(
                request,
                SetupScriptEvent::ActionCompleted {
                    action: SetupScriptAction::Run
                }
            ),
            cx
        ));
        assert_eq!(view.first_run.view().phase, FirstRunPhase::RestartRequired);
        assert!(
            !view.first_run_open(),
            "completion preserves facts without commandeering a dismissed surface"
        );
    });
}

#[test]
fn unsupported_without_setup_info_is_rendered_as_failure_not_discovery() {
    assert_eq!(
        empty_state_message_key(&FirstRunPhase::Discovering),
        "first_run.discovering"
    );
    assert_eq!(
        empty_state_message_key(&FirstRunPhase::Failed(FailureKind::Unsupported)),
        "first_run.failure_unsupported"
    );
}

#[gpui::test]
async fn first_run_review_pins_actions_outside_the_real_scroll_owner(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::{ScrollHandle, VisualTestContext, size};
    use taskmanager_application::first_run::FirstRunController;
    for (width, height) in [
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 400.0),
        (720.0, 960.0),
    ] {
        let handle = ScrollHandle::new();
        let win = cx.add_window(|_, cx| RootView::new(Theme::dark(), cx));
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        win.update(cx, |view, _, cx| {
            view.mark_telemetry_frame_ready();
            let mut info = setup_script_info();
            info.run_command = info.run_command.repeat(12);
            info.revert_command = info.revert_command.repeat(12);
            view.first_run = FirstRunController::from_observation(Some(info));
            view.dialog_scroll.first_run = handle.clone();
            view.show_first_run();
            cx.notify();
        })
        .expect("explicit setup");
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
            .expect("render");
        let mut visual = VisualTestContext::from_window(win.into(), cx);
        let viewport = visual
            .debug_bounds("tm-first-run-scroll")
            .expect("bounded metadata");
        let actions = visual
            .debug_bounds("tm-first-run-actions")
            .expect("complete action groups");
        assert!(viewport.size.height > px(0.0) && actions.size.height > px(0.0));
        assert!(viewport.bottom() < actions.top());
        assert!(actions.left() >= px(0.0) && actions.right() < px(width));
        assert!(
            actions.bottom() < px(height),
            "actions must protect the bottom edge at {width}x{height}"
        );
        if height <= 480.0 {
            assert!(
                handle.max_offset().height > px(0.0),
                "the complete descriptor remains reachable through scrolling"
            );
        }
        handle.set_offset(gpui::point(px(0.0), px(-80.0)));
        cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
            .expect("scroll");
        let after = visual
            .debug_bounds("tm-first-run-actions")
            .expect("fixed actions after scrolling");
        let viewport_after = visual
            .debug_bounds("tm-first-run-scroll")
            .expect("body viewport after scrolling");
        assert_eq!(after.size, actions.size);
        assert_eq!(viewport_after.size, viewport.size);
        // Entrance animation translates the whole dialog. Compare the fixed
        // footer to the viewport so only a scroll-coordinate mistake fails.
        assert!(
            (f32::from(after.top() - viewport_after.bottom())
                - f32::from(actions.top() - viewport.bottom()))
            .abs()
                < 0.01,
            "body scrolling cannot move the action row relative to its viewport"
        );
    }
}
