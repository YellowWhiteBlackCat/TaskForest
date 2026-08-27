use super::{
    appear_transition, color_transition, hover_animation, hover_animation_for, hover_bg_transition,
    hover_bg_transition_with_policy, hover_state_key, interpolate_color, interpolate_opacity,
    opacity_transition, safe_opacity,
};
use gpui::{
    AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{DURATION_FAST, DURATION_HOVER, MotionPolicy};

#[test]
fn hover_state_key_maps_state_pairs_bijectively() {
    // The key must be a pure function of the state pair: stable per pair
    // (so the animation persists across frames) and distinct across pairs
    // (so gpui restarts exactly on state changes).
    let keys = [
        hover_state_key(false, false),
        hover_state_key(false, true),
        hover_state_key(true, false),
        hover_state_key(true, true),
    ];
    assert_eq!(keys[0], hover_state_key(false, false));
    assert_eq!(keys[3], hover_state_key(true, true));
    for (i, a) in keys.iter().enumerate() {
        for (j, b) in keys.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "state pairs {i} and {j} must key distinctly");
            }
        }
    }
}

#[test]
fn hover_animation_uses_the_hover_duration_token() {
    assert_eq!(hover_animation().duration, DURATION_HOVER);
    assert!(
        hover_animation().oneshot,
        "hover transitions must be one-shot sweeps (restarted via the keyed id)"
    );
}

#[test]
fn policy_animations_use_theme_tokens_and_skip_no_motion() {
    assert_eq!(
        hover_animation_for(MotionPolicy::Normal).map(|animation| animation.duration),
        Some(DURATION_HOVER)
    );
    assert_eq!(
        hover_animation_for(MotionPolicy::Reduced).map(|animation| animation.duration),
        Some(DURATION_FAST)
    );
    assert!(hover_animation_for(MotionPolicy::NoMotion).is_none());
}

#[test]
fn opacity_and_color_interpolation_clamp_progress_and_endpoints() {
    assert_eq!(safe_opacity(-0.5), 0.0);
    assert_eq!(safe_opacity(1.5), 1.0);
    assert_eq!(safe_opacity(f32::NAN), 0.0);
    assert_eq!(interpolate_opacity(0.0, 1.0, -1.0), 0.0);
    assert_eq!(interpolate_opacity(0.0, 1.0, 2.0), 1.0);

    let theme = Theme::dark();
    let base = theme.sidebar_card_bg;
    let target = theme.hover_bg();
    let at_start = interpolate_color(base, target, -1.0);
    let at_end = interpolate_color(base, target, 2.0);
    let at_nan = interpolate_color(base, target, f32::NAN);
    for (actual, expected) in [(at_start, base), (at_end, target), (at_nan, base)] {
        assert!((actual.r - expected.r).abs() < 0.000_001);
        assert!((actual.g - expected.g).abs() < 0.000_001);
        assert!((actual.b - expected.b).abs() < 0.000_001);
        assert!((actual.a - expected.a).abs() < 0.000_001);
    }
}

/// The transition wrapper is a transparent layout citizen: the animated
/// element keeps its identity and bounds across hovered flips (id keyed
/// restarts must never remount or reflow the element).
#[gpui::test]
async fn hover_bg_transition_keeps_bounds_through_hover_flips(cx: &mut TestAppContext) {
    struct Harness {
        hovered: bool,
    }
    impl Render for Harness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let t = Theme::dark();
            div()
                .id("hover-host")
                .debug_selector(|| "hover-host".to_string())
                .child(hover_bg_transition(
                    div()
                        .id("hover-target")
                        .debug_selector(|| "hover-target".to_string())
                        .w(px(40.0))
                        .h(px(20.0)),
                    ("hover-bg-test", hover_state_key(false, self.hovered)),
                    t.sidebar_card_bg,
                    t.hover_bg(),
                    self.hovered,
                ))
        }
    }
    let window = cx.add_window(|_window, _cx| Harness { hovered: false });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let idle = vcx
        .debug_bounds("hover-target")
        .expect("hover target lays out in the idle state");
    drop(vcx);
    // Flip hovered → the key changes, the animation restarts, the element
    // must keep its identity and bounds.
    window
        .update(cx, |harness, _window, cx| {
            harness.hovered = true;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let hovered = vcx
        .debug_bounds("hover-target")
        .expect("hover target survives the hovered flip");
    assert_eq!(hovered, idle, "the transition must not reflow the element");
    drop(vcx);
}

#[gpui::test]
async fn policy_helpers_keep_layout_and_target_ids_stable(cx: &mut TestAppContext) {
    struct Harness {
        policy: MotionPolicy,
        hovered: bool,
    }

    impl Render for Harness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::dark();
            div()
                .id("motion-policy-host")
                .child(opacity_transition(
                    div()
                        .id("motion-opacity-target")
                        .debug_selector(|| "motion-opacity-target".to_string())
                        .w(px(40.0))
                        .h(px(20.0)),
                    "motion-opacity",
                    0.0,
                    1.0,
                    self.policy,
                ))
                .child(color_transition(
                    div()
                        .id("motion-color-target")
                        .debug_selector(|| "motion-color-target".to_string())
                        .w(px(40.0))
                        .h(px(20.0)),
                    "motion-color",
                    theme.sidebar_card_bg,
                    theme.hover_bg(),
                    self.policy,
                ))
                .child(hover_bg_transition_with_policy(
                    div()
                        .id("motion-hover-target")
                        .debug_selector(|| "motion-hover-target".to_string())
                        .w(px(40.0))
                        .h(px(20.0)),
                    ("motion-hover", hover_state_key(false, self.hovered)),
                    theme.sidebar_card_bg,
                    theme.hover_bg(),
                    self.hovered,
                    self.policy,
                ))
                .child(appear_transition(
                    div()
                        .id("motion-appear-target")
                        .debug_selector(|| "motion-appear-target".to_string())
                        .w(px(40.0))
                        .h(px(20.0)),
                    "motion-appear",
                    self.policy,
                ))
        }
    }

    let window = cx.add_window(|_window, _cx| Harness {
        policy: MotionPolicy::Normal,
        hovered: true,
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let normal_bounds = [
        vcx.debug_bounds("motion-opacity-target"),
        vcx.debug_bounds("motion-color-target"),
        vcx.debug_bounds("motion-hover-target"),
        vcx.debug_bounds("motion-appear-target"),
    ];
    assert!(normal_bounds.iter().all(Option::is_some));
    drop(vcx);

    window
        .update(cx, |harness, _window, cx| {
            harness.policy = MotionPolicy::NoMotion;
            harness.hovered = false;
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let no_motion_bounds = [
        vcx.debug_bounds("motion-opacity-target"),
        vcx.debug_bounds("motion-color-target"),
        vcx.debug_bounds("motion-hover-target"),
        vcx.debug_bounds("motion-appear-target"),
    ];
    assert_eq!(
        no_motion_bounds, normal_bounds,
        "policy changes must not remount or reflow targets"
    );
}
