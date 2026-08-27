//! Per-window isolation of the Settings dialog's interactive state
//! (refresh slider, Startup-page select, text-rendering pills, switches).

use gpui::{Entity, Keystroke, TestAppContext, VisualTestContext, WindowHandle, point, px};
use taskmanager::core::config::{
    COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM,
    STARTUP_PAGE_PERFORMANCE, STARTUP_PAGE_PROCESSES, STARTUP_PAGE_REMEMBER,
    TEXT_RENDERING_PLATFORM_DEFAULT,
};
use taskmanager_gpui::gpui_app::root::{DevicePreference, RootView, TopPage};
use taskmanager_gpui::gpui_app::theme::LightDark;

use super::{draw, wrapped_root};

fn open_settings(cx: &mut TestAppContext, view: &Entity<RootView>) {
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.show_settings();
        cx.notify();
    });
}

/// Click the center of the Settings pill with the given debug id in `win`'s
/// dialog (pills are plain `.id()` elements inside the dialog content).
fn click_settings_pill(cx: &mut TestAppContext, win: WindowHandle<RootView>, id: &'static str) {
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let bounds = vcx
        .debug_bounds(id)
        .expect("the settings dialog renders the requested pill");
    vcx.simulate_click(bounds.center(), Default::default());
}

/// Focus `win`'s own refresh-interval slider (real tab stop) and step it by
/// `keystrokes` through the real key pipeline.
fn step_settings_slider(cx: &mut TestAppContext, win: WindowHandle<RootView>, keystrokes: &str) {
    win.update(cx, |root, window, cx| {
        let handle = root
            .settings_slider
            .as_ref()
            .expect("the window rendered its settings slider")
            .read(cx)
            .focus_handle()
            .clone();
        handle.focus(window);
    })
    .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse(keystrokes).unwrap());
}

/// The Settings dialog's refresh-interval slider must be per-window. With the
/// old shared `thread_local SLIDER`, both windows rendered the SAME
/// SliderState entity, so stepping B's slider moved A's thumb and wrote A's
/// telemetry interval too. Each RootView now owns its own slider entity
/// (`settings_slider`): a keyboard step in B must change only B's interval,
/// and the reverse step must change only A's.
#[gpui::test]
async fn settings_slider_step_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    open_settings(cx, &view_a);
    open_settings(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);
    let interval_ms = |view: &Entity<RootView>, cx: &mut TestAppContext| {
        view.read_with(cx, |v, _cx| {
            v.telemetry_refresh_policy.interval().duration()
        })
        .as_millis() as u64
    };

    // B: focus B's own slider and step right by one 0.1 s keyboard step.
    step_settings_slider(cx, win_b, "right");
    assert_eq!(
        interval_ms(&view_b, cx),
        1100,
        "stepping B's slider must update B's telemetry interval"
    );
    assert_eq!(
        interval_ms(&view_a, cx),
        1000,
        "B's slider step must never change A's telemetry interval"
    );

    // Reverse: A steps its own slider; B keeps its value.
    step_settings_slider(cx, win_a, "right");
    assert_eq!(
        interval_ms(&view_a, cx),
        1100,
        "stepping A's slider must update A's telemetry interval"
    );
    assert_eq!(
        interval_ms(&view_b, cx),
        1100,
        "A's slider step must never disturb B's interval"
    );

    // Step B once more so the two windows hold visibly independent values.
    step_settings_slider(cx, win_b, "right");
    assert_eq!(
        interval_ms(&view_b, cx),
        1200,
        "B's second step must update B's interval"
    );
    assert_eq!(
        interval_ms(&view_a, cx),
        1100,
        "B's second step must never move A's interval"
    );

    // Both windows keep rendering with their independent slider entities.
    draw(cx, win_a);
    draw(cx, win_b);
}

/// The Settings Startup-page select must be per-window. With the old shared
/// `thread_local STARTUP_PAGE`, choosing an option in B rewrote the token
/// window A's dialog rendered. Each RootView now records its own token: B's
/// choice updates B only, and the reverse choice updates A only. Each choice
/// goes through the real select: click the trigger, then click the option
/// row inside the opened popup.
#[gpui::test]
async fn settings_startup_page_choice_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    open_settings(cx, &view_a);
    open_settings(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    // B: choose the "Performance" startup option; only B's token may change.
    select_settings_option(cx, win_b, "startup-page:trigger", 1);
    assert!(
        view_b.read_with(cx, |v, _cx| {
            v.presentation_snapshot().startup_page() == STARTUP_PAGE_PERFORMANCE
        }),
        "choosing Performance in B must record it in B"
    );
    assert!(
        view_a.read_with(cx, |v, _cx| {
            v.presentation_snapshot().startup_page() == STARTUP_PAGE_REMEMBER
        }),
        "B's choice must never record into A"
    );

    // Reverse: A chooses "Processes"; B keeps its own choice.
    select_settings_option(cx, win_a, "startup-page:trigger", 2);
    assert!(
        view_a.read_with(cx, |v, _cx| {
            v.presentation_snapshot().startup_page() == STARTUP_PAGE_PROCESSES
        }),
        "choosing Processes in A must record it in A"
    );
    assert!(
        view_b.read_with(cx, |v, _cx| {
            v.presentation_snapshot().startup_page() == STARTUP_PAGE_PERFORMANCE
        }),
        "A's choice must never disturb B's choice"
    );

    draw(cx, win_a);
    draw(cx, win_b);
}

/// Click the select trigger with the given debug id in `win`'s dialog, then
/// click the `item_ix`-th option row inside the opened popup. Both clicks
/// pass through the real hit-testing (hover-then-click, like the popup's own
/// tests), and the popup dismisses on the option click.
fn select_settings_option(
    cx: &mut TestAppContext,
    win: WindowHandle<RootView>,
    trigger_id: &'static str,
    item_ix: usize,
) {
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let trigger = vcx
        .debug_bounds(trigger_id)
        .expect("the settings dialog renders the select trigger");
    eprintln!(
        "HELPER: trigger={trigger:?} group-general={:?}",
        vcx.debug_bounds("settings.group_general")
    );
    vcx.simulate_mouse_move(
        trigger.center(),
        None::<gpui::MouseButton>,
        Default::default(),
    );
    vcx.simulate_click(trigger.center(), Default::default());
    drop(vcx);
    draw(cx, win);
    // The opening frame renders the menu at the previous anchor; a second
    // draw re-anchors it below the trigger (away from the trigger hitbox).
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("the trigger click opens the select menu");
    let trigger_now = vcx.debug_bounds(trigger_id);
    eprintln!("HELPER: popup={popup:?} trigger={trigger_now:?}");
    // Rows are 26px tall inside the body's 4px top padding (popup tests).
    let item = point(
        popup.left() + px(40.0),
        popup.top() + px(4.0 + 26.0 * item_ix as f32 + 13.0),
    );
    vcx.simulate_mouse_move(item, None::<gpui::MouseButton>, Default::default());
    vcx.simulate_click(item, Default::default());
    drop(vcx);
    draw(cx, win);
}

/// The Settings text-rendering capability state must be per-window and honest:
/// unsupported choices remain visible for explanation, but clicks in either
/// window must stay at the platform default and never cross window state.
#[gpui::test]
async fn settings_text_rendering_choice_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    open_settings(cx, &view_a);
    open_settings(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);
    // The preview cards add a deliberate visual tier to Appearance, so the
    // lower Fonts group may start below the initial viewport. Scroll both
    // per-window dialogs before probing their text-rendering controls.
    view_a.update(cx, |view, _cx| {
        view.settings_scroll_handle().scroll_to_bottom();
    });
    view_b.update(cx, |view, _cx| {
        view.settings_scroll_handle().scroll_to_bottom();
    });
    draw(cx, win_a);
    draw(cx, win_b);
    // Text rendering sits above the final Units group. Move the just-computed
    // bottom offset back into the viewport by a bounded amount so the target
    // pills are actually hit-testable instead of living at a negative y.
    view_a.update(cx, |view, _cx| {
        let scroll = view.settings_scroll_handle();
        let offset = scroll.offset();
        scroll.set_offset(point(offset.x, offset.y + px(370.0)));
    });
    view_b.update(cx, |view, _cx| {
        let scroll = view.settings_scroll_handle();
        let offset = scroll.offset();
        scroll.set_offset(point(offset.x, offset.y + px(370.0)));
    });
    draw(cx, win_a);
    draw(cx, win_b);

    // B: the visible Subpixel choice is unsupported, so its click is inert.
    click_settings_pill(cx, win_b, "text-rendering-subpixel");
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.presentation_snapshot().text_rendering()),
        TEXT_RENDERING_PLATFORM_DEFAULT,
        "clicking the unsupported Subpixel pill in B must stay at the platform default"
    );
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.presentation_snapshot().text_rendering()),
        TEXT_RENDERING_PLATFORM_DEFAULT,
        "B's pill click must never record into A"
    );

    // Reverse: A clicks the other unsupported choice; both windows remain default.
    click_settings_pill(cx, win_a, "text-rendering-grayscale");
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.presentation_snapshot().text_rendering()),
        TEXT_RENDERING_PLATFORM_DEFAULT,
        "clicking the unsupported Grayscale pill in A must stay at the platform default"
    );
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.presentation_snapshot().text_rendering()),
        TEXT_RENDERING_PLATFORM_DEFAULT,
        "A's unsupported pill click must never disturb B's platform default"
    );

    draw(cx, win_a);
    draw(cx, win_b);
}

/// The Mission Center-compatible System/Light/Dark preference must be owned by
/// each window. System resolves from the typed appearance already on RootView;
/// this test uses the unknown-appearance fallback to prove the selection is
/// not a UI-only label and that B cannot rewrite A.
#[gpui::test]
async fn settings_color_scheme_choice_is_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    open_settings(cx, &view_a);
    open_settings(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    click_settings_pill(cx, win_b, "mode-dark");
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_DARK,
        "B's dark choice must persist as B's preference"
    );
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.theme.mode),
        LightDark::Dark,
        "B's dark choice must resolve the dark palette"
    );
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_SYSTEM,
        "B's choice must never rewrite A's System preference"
    );

    click_settings_pill(cx, win_a, "mode-light");
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_LIGHT,
        "A's light choice must persist as A's preference"
    );
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_DARK,
        "A's choice must never disturb B"
    );

    click_settings_pill(cx, win_a, "mode-eyeforest");
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_EYEFOREST,
        "A's EyeForest choice must persist as A's preference"
    );
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.theme.mode),
        LightDark::EyeForest,
        "EyeForest must resolve the product-owned forest palette"
    );
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_DARK,
        "A's EyeForest choice must never disturb B"
    );

    click_settings_pill(cx, win_b, "mode-system");
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_SYSTEM,
        "B can return to the System preference"
    );
    assert_eq!(
        view_b.read_with(cx, |v, _cx| v.theme.mode),
        LightDark::Light,
        "unknown native appearance must resolve System to the documented light fallback"
    );
    assert_eq!(
        view_a.read_with(cx, |v, _cx| v.presentation_snapshot().color_scheme()),
        COLOR_SCHEME_EYEFOREST,
        "returning B to System must not rewrite A"
    );

    draw(cx, win_a);
    draw(cx, win_b);
}

/// The Settings toggle switches must be per-window. With the old shared
/// thread_local switch state (pre-per-window sweep), toggling B's switch
/// would have flipped A's entity too; now each RootView owns its own
/// `settings_switches` entity map, so a keyboard toggle in B changes only
/// B's flags. Drives the real ui `Switch` through focus + Enter.
#[gpui::test]
async fn settings_switches_are_isolated_per_window(cx: &mut TestAppContext) {
    let (win_a, view_a) = wrapped_root(cx);
    let (win_b, view_b) = wrapped_root(cx);
    open_settings(cx, &view_a);
    open_settings(cx, &view_b);
    draw(cx, win_a);
    draw(cx, win_b);

    fn toggle_focused(cx: &mut TestAppContext, win: WindowHandle<RootView>, id: &'static str) {
        win.update(cx, |root, window, cx| {
            let handle = root.settings_switches[id].read(cx).focus_handle().clone();
            handle.focus(window);
        })
        .unwrap();
        cx.dispatch_keystroke(win.into(), Keystroke::parse("enter").unwrap());
        draw(cx, win);
    }

    // B: toggle the High-contrast switch; only B's theme may change.
    toggle_focused(cx, win_b, "hc-switch");
    assert!(
        view_b.read_with(cx, |v, _| v.theme.hc),
        "Enter on B's focused hc switch must enable B's high contrast"
    );
    assert!(
        !view_a.read_with(cx, |v, _| v.theme.hc),
        "B's hc toggle must never change A's theme"
    );

    // B: toggle the CPU-visibility switch; only B's flag may change.
    toggle_focused(cx, win_b, "device-cpu");
    assert!(
        !view_b.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::Cpu)
        }),
        "Enter on B's focused device-cpu switch must hide B's CPU row"
    );
    assert!(
        view_a.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::Cpu)
        }),
        "B's cpu toggle must never change A's visibility flags"
    );

    // Network visibility is a master toggle plus independent typed category
    // switches. A VPN choice in B must not alter A or the other categories in
    // B; this exercises the same real Switch + Enter path as the flat device
    // controls above.
    toggle_focused(cx, win_b, "network-vpn");
    assert!(
        !view_b.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::NetworkVpn)
        }),
        "B's VPN category switch must hide only B's VPN adapters"
    );
    assert!(
        view_b.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::NetworkWireless)
        }),
        "B's VPN category switch must not change B's wireless category"
    );
    assert!(
        view_a.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::NetworkVpn)
        }),
        "B's VPN category switch must never change A's visibility policy"
    );

    toggle_focused(cx, win_b, "device-network");
    assert!(
        !view_b.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::Network)
        }),
        "B's master network switch must hide all network categories in B"
    );
    assert!(
        view_b.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::NetworkWireless)
        }),
        "the master switch must preserve child category preferences"
    );
    assert!(
        view_a.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::Network)
        }),
        "B's master network switch must never change A"
    );

    // B: the Apps-page zero-value presentation preference is also owned by the
    // window. Its switch changes only B's rendering policy.
    toggle_focused(cx, win_b, "gray-zero-values");
    assert!(
        view_b.read_with(cx, |v, _| v.presentation_snapshot().gray_zero_values()),
        "Enter on B's zero-value switch must enable B's Apps preference"
    );
    assert!(
        !view_a.read_with(cx, |v, _| v.presentation_snapshot().gray_zero_values()),
        "B's zero-value preference must never change A's Apps rendering"
    );

    // A's own switches still work after B's toggles (entities never crossed).
    toggle_focused(cx, win_a, "device-cpu");
    assert!(
        !view_a.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::Cpu)
        }),
        "A's own device-cpu toggle must flip A's flag"
    );
    assert!(
        !view_b.read_with(cx, |v, _| {
            v.presentation_snapshot()
                .device_visible(DevicePreference::Cpu)
        }),
        "A's toggle must never resurrect B's cpu row"
    );
}
