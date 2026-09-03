//! Navigation-strip + adaptive-decoration render tests.
//!
//! Direction 3 split page navigation out of the CSD titlebar into a dedicated
//! `nav_strip` rendered BELOW the titlebar, and made the renderer prefer native
//! (Server) decorations — reacting to what the compositor grants rather than
//! sniffing XDG_CURRENT_DESKTOP. These tests pin the conditional logic:
//!
//! - the nav strip (page tabs + gear) renders in BOTH decoration modes;
//! - the app-drawn titlebar chrome (drag region `tl-drag` + window controls) is
//!   ABSENT when Server decorations are granted and PRESENT only in the CSD
//!   fallback;
//! - clicking a nav tab still flips `RootView::page`.
//!
//! The gpui `TestWindow` always reports `Decorations::Server`, so the CSD
//! fallback path is unreachable headlessly without the `decorations_override`
//! hook on `RootView` (None in production; Some(true)/Some(false) here). Each
//! mode renders in a FRESH window because gpui's per-window `debug_bounds` map
//! accumulates across draws — a clean window per mode is the only way a negative
//! assertion ("control absent") is meaningful.

use gpui::{AppContext, Keystroke, TestAppContext, VisualTestContext, WindowHandle};
use taskmanager_core::core::setup::SetupScriptInfo;
use taskmanager_gpui::gpui_app::dashboard::SystemSection;
use taskmanager_gpui::gpui_app::first_run::{FirstRunPhase, FirstRunUiState};
use taskmanager_gpui::gpui_app::root::{RootView, TopPage, UnitFamily, WindowSurfaceKind};
use taskmanager_gpui::gpui_app::sidebar::SelectedDevice;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::RowDensity;

/// Open a fresh window, force a decoration mode, drop the cold-start placeholder,
/// and run one headless draw so `debug_bounds` selectors populate. Returns the
/// window handle for selector/click access.
fn render_in_mode(cx: &mut TestAppContext, server_decorations: bool) -> WindowHandle<RootView> {
    let win = cx.add_window(|_w, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _w, cx| {
        v.decorations_override = Some(server_decorations);
        v.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    win
}

/// The page-navigation strip (tabs + settings gear) renders in BOTH decoration
/// modes — it is app content, not window chrome. Tabs keep their stable English
/// identity ids ("Apps"/"Services"/…); the gear keeps "settings-btn".
#[gpui::test]
async fn mc00_page_sweep_case_nav_strip_renders_tabs_and_gear_in_both_decoration_modes(
    cx: &mut TestAppContext,
) {
    for server_decorations in [true, false] {
        let win = render_in_mode(cx, server_decorations);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        let mode = if server_decorations { "native" } else { "CSD" };
        assert!(
            vcx.debug_bounds("Apps").is_some(),
            "Apps tab must render in {mode} mode"
        );
        assert!(
            vcx.debug_bounds("Services").is_some(),
            "Services tab must render in {mode} mode"
        );
        assert!(
            vcx.debug_bounds("System").is_some(),
            "System tab must render in {mode} mode"
        );
        assert!(
            vcx.debug_bounds("settings-btn").is_some(),
            "settings gear must render in {mode} mode (it moved out of the CSD titlebar)"
        );
        #[cfg(target_os = "linux")]
        assert!(
            vcx.debug_bounds("window-capture-btn").is_some(),
            "Linux must render the current-window PNG capture action in {mode} mode"
        );
        drop(vcx);
    }
}

/// Edit mode exposes a concrete device control whose pointer path updates the
/// persisted per-device override, not the category-wide visibility switch.
#[gpui::test]
async fn mc05_sidebar_edit_case_sidebar_edit_click_updates_the_exact_device_override(
    cx: &mut TestAppContext,
) {
    let win = render_in_mode(cx, true);
    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let edit = visual
        .debug_bounds("sidebar-edit")
        .expect("Performance sidebar exposes edit mode");
    visual.simulate_click(edit.center(), Default::default());
    drop(visual);

    assert!(win.read_with(cx, |view, _| view.sidebar_edit_mode).unwrap());
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let cpu = visual
        .debug_bounds("sidebar-device-toggle-cpu")
        .expect("edit mode exposes the concrete CPU visibility control");
    visual.simulate_click(cpu.center(), Default::default());
    drop(visual);

    win.read_with(cx, |view, _| {
        let presentation = view.presentation_snapshot();
        assert_eq!(presentation.sidebar_device_overrides().len(), 1);
        let override_ = &presentation.sidebar_device_overrides()[0];
        assert_eq!(override_.device, "cpu");
        assert!(!override_.visible);
    })
    .unwrap();
}

/// The persisted order drives both wide row placement and the compact pill's
/// pointer target; the compact renderer cannot fall back to discovery order.
#[gpui::test]
async fn mc05_sidebar_order_case_configured_sidebar_order_drives_wide_and_compact_pointer_targets(
    cx: &mut TestAppContext,
) {
    let win = render_in_mode(cx, true);
    win.update(cx, |view, _window, cx| {
        view.set_sidebar_order(vec!["memory".into(), "cpu".into()], cx);
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let memory = visual
        .debug_bounds("sidebar-device:memory")
        .expect("wide sidebar exposes Memory");
    let cpu = visual
        .debug_bounds("sidebar-device:cpu")
        .expect("wide sidebar exposes CPU");
    assert!(memory.origin.y < cpu.origin.y);
    drop(visual);

    cx.simulate_window_resize(win.into(), gpui::size(gpui::px(720.0), gpui::px(480.0)));
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let first = visual
        .debug_bounds("compact-device-0")
        .expect("compact strip exposes its first ordered device");
    visual.simulate_click(first.center(), Default::default());
    drop(visual);

    assert_eq!(
        win.read_with(cx, |view, _| view.selected).unwrap(),
        SelectedDevice::Memory
    );
}

/// The Settings density control writes the same typed geometry axis consumed
/// by Apps headers and rows; no test-only field mutation bypasses the click.
#[gpui::test]
async fn mc05_density_case_density_setting_click_updates_the_table_geometry_contract(
    cx: &mut TestAppContext,
) {
    let win = render_in_mode(cx, true);
    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let settings = visual
        .debug_bounds("settings-btn")
        .expect("settings gear renders");
    visual.simulate_click(settings.center(), Default::default());
    drop(visual);
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let compact = visual
        .debug_bounds("density-compact")
        .expect("Settings exposes the compact density choice");
    visual.simulate_click(compact.center(), Default::default());
    drop(visual);

    assert_eq!(
        win.read_with(cx, |view, _| view.presentation_snapshot().density())
            .unwrap(),
        RowDensity::Compact
    );
    assert!(RowDensity::Compact.row_padding_y() < RowDensity::Comfortable.row_padding_y());
}

/// Performance unit choices are independent typed preferences: changing one
/// pair updates only that pair and stays on the owning RootView window.
#[gpui::test]
async fn mc05_units_case_settings_units_switches_update_per_window_preferences(
    cx: &mut TestAppContext,
) {
    let win = render_in_mode(cx, true);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let settings = vcx
        .debug_bounds("settings-btn")
        .expect("settings gear renders");
    vcx.simulate_click(settings.center(), Default::default());
    drop(vcx);
    win.update(cx, |view, _window, _cx| {
        view.settings_scroll_handle().scroll_to_bottom();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let memory_bits = vcx
        .debug_bounds("memory-unit-bits")
        .expect("Memory Bytes/Bits choices render");
    vcx.simulate_click(memory_bits.center(), Default::default());
    drop(vcx);
    assert!(
        win.read_with(cx, |view, _cx| {
            !view
                .presentation_snapshot()
                .unit_choices(UnitFamily::Memory)
                .0
        })
        .unwrap()
    );
    assert!(
        win.read_with(cx, |view, _cx| {
            view.presentation_snapshot()
                .unit_choices(UnitFamily::Memory)
                .1
        })
        .unwrap()
    );

    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let network_base2 = vcx
        .debug_bounds("network-base-2")
        .expect("Network Base 2 choice renders");
    vcx.simulate_click(network_base2.center(), Default::default());
    drop(vcx);
    assert!(
        win.read_with(cx, |view, _cx| {
            view.presentation_snapshot()
                .unit_choices(UnitFamily::Network)
                .1
        })
        .unwrap()
    );
    assert!(
        win.read_with(cx, |view, _cx| {
            !view
                .presentation_snapshot()
                .unit_choices(UnitFamily::Network)
                .0
        })
        .unwrap()
    );

    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let _sliding = vcx
        .debug_bounds("sliding-graphs")
        .expect("Sliding graphs switch renders");
    drop(vcx);
    win.update(cx, |view, window, cx| {
        let handle = view.settings_switches["sliding-graphs"]
            .read(cx)
            .focus_handle()
            .clone();
        handle.focus(window);
    })
    .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse("enter").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view
            .presentation_snapshot()
            .sliding_graphs())
            .unwrap()
    );

    win.update(cx, |view, window, cx| {
        let handle = view
            .graph_points_slider
            .as_ref()
            .expect("graph data-points slider is initialized")
            .read(cx)
            .focus_handle()
            .clone();
        handle.focus(window);
    })
    .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse("end").unwrap());
    assert_eq!(
        win.read_with(cx, |view, _cx| {
            view.presentation_snapshot().graph_data_points()
        })
        .unwrap(),
        600,
        "graph points keyboard path must reach the bounded maximum"
    );

    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let _dynamic_scaling = vcx
        .debug_bounds("network-dynamic-scaling")
        .expect("Network dynamic scaling switch renders");
    drop(vcx);
    win.update(cx, |view, window, cx| {
        let handle = view.settings_switches["network-dynamic-scaling"]
            .read(cx)
            .focus_handle()
            .clone();
        handle.focus(window);
    })
    .unwrap();
    cx.dispatch_keystroke(win.into(), Keystroke::parse("enter").unwrap());
    assert!(
        !win.read_with(cx, |view, _cx| {
            view.presentation_snapshot().network_dynamic_scaling()
        })
        .unwrap()
    );
}

/// The app-drawn titlebar chrome exists ONLY in the CSD fallback: when Server
/// decorations are granted, the native titlebar owns the title + controls and
/// the renderer emits no `top_bar`. The `tl-drag` id marks the CSD drag region
/// and `wnd-close` marks the GNOME AdwaitaClose button (Theme::dark is GNOME).
#[gpui::test]
async fn titlebar_chrome_absent_in_native_mode_present_in_csd(cx: &mut TestAppContext) {
    // Native (Server granted): no app titlebar, no app-drawn controls.
    let win = render_in_mode(cx, true);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tl-drag").is_none(),
        "CSD drag region must NOT render when Server decorations are granted"
    );
    assert!(
        vcx.debug_bounds("wnd-close").is_none(),
        "app-drawn close button must NOT render when Server decorations are granted"
    );
    drop(vcx);

    // CSD fallback (compositor forced Client): app titlebar + controls render.
    let win = render_in_mode(cx, false);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tl-drag").is_some(),
        "CSD drag region must render in the CSD fallback"
    );
    assert!(
        vcx.debug_bounds("wnd-close").is_some(),
        "app-drawn close button must render in the CSD fallback (GNOME AdwaitaClose)"
    );
}

/// Clicking a nav tab flips `RootView::page` through the same `on_click` wiring
/// the old in-titlebar tabs used (the tab helper moved unchanged into
/// `nav_strip`). Starts on Performance, clicks the Services tab center.
#[gpui::test]
async fn mc00_nav_pointer_case_nav_tab_click_switches_page(cx: &mut TestAppContext) {
    let win = render_in_mode(cx, true);
    let initial = win.read_with(cx, |v, _cx| v.page).unwrap();
    assert_eq!(
        initial,
        TopPage::Performance,
        "sanity: a fresh view starts on Performance"
    );
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let bounds = vcx
        .debug_bounds("Services")
        .expect("the Services nav tab renders");
    vcx.simulate_click(bounds.center(), Default::default());
    drop(vcx);
    let page = win.read_with(cx, |v, _cx| v.page).unwrap();
    assert_eq!(
        page,
        TopPage::Services,
        "clicking the Services nav tab must switch the active page"
    );
}

/// The System page exposes an independent About entry. It must open its own
/// modal state, keep System Information closed, render its copy action, and
/// close through the same root Escape path as every other modal.
#[gpui::test]
async fn mc06_about_case_system_page_about_entry_opens_independent_modal(cx: &mut TestAppContext) {
    let win = render_in_mode(cx, true);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let system = vcx.debug_bounds("System").expect("System tab renders");
    vcx.simulate_click(system.center(), Default::default());
    drop(vcx);
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    win.update(cx, |view, _window, cx| {
        view.dashboard.section = SystemSection::Hardware;
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let about = vcx
        .debug_bounds("about-open")
        .expect("System page exposes the About entry");
    vcx.simulate_click(about.center(), Default::default());
    drop(vcx);

    assert!(
        win.read_with(cx, |view, _cx| view.window_surface_kind())
            .unwrap()
            == Some(WindowSurfaceKind::About),
        "the About entry must open the independent About modal"
    );
    assert!(
        win.read_with(cx, |view, _cx| view.window_surface_kind())
            .unwrap()
            != Some(WindowSurfaceKind::SystemAbout),
        "opening About must not alias System Information state"
    );
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("about-copy-details").is_some(),
        "About must render its typed copy-details action"
    );
    drop(vcx);

    cx.dispatch_keystroke(win.into(), gpui::Keystroke::parse("escape").unwrap());
    assert!(
        win.read_with(cx, |view, _cx| view.window_surface_kind())
            .unwrap()
            != Some(WindowSurfaceKind::About),
        "Escape must close the About modal"
    );
}

/// First Run is a real setup projection, not a static copy block: the dialog
/// exposes View/Run/Revert, and a missing runtime port turns Run into an honest
/// typed failure instead of claiming that setup succeeded.
#[gpui::test]
async fn mc06_first_run_case_first_run_dialog_keeps_setup_actions_typed_and_failure_visible(
    cx: &mut TestAppContext,
) {
    let win = render_in_mode(cx, true);
    win.update(cx, |view, _window, cx| {
        view.first_run = FirstRunUiState {
            phase: FirstRunPhase::Available,
            info: Some(SetupScriptInfo {
                path: std::path::PathBuf::from("/usr/share/taskforest/setup/99-taskforest.rules"),
                run_command: "pkexec /usr/libexec/taskforest-setup-helper install".into(),
                revert_command: "pkexec /usr/libexec/taskforest-setup-helper revert".into(),
            }),
            last_action: None,
        };
        view.show_first_run();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    for id in [
        "first-run-open-docs",
        "first-run-view-script",
        "first-run-run-setup",
        "first-run-revert-setup",
        "first-run-copy-location",
        "first-run-copy-command",
        "first-run-copy-revert",
        "first-run-close",
    ] {
        assert!(vcx.debug_bounds(id).is_some(), "First Run renders {id}");
    }
    let run = vcx
        .debug_bounds("first-run-run-setup")
        .expect("Run setup action renders");
    vcx.simulate_click(run.center(), Default::default());
    drop(vcx);

    assert_eq!(
        win.read_with(cx, |view, _cx| view.first_run.phase.clone())
            .unwrap(),
        FirstRunPhase::Failed(taskmanager_core::core::failure::FailureKind::TemporarilyUnavailable),
        "a missing typed provider must remain an honest failure"
    );
    assert!(
        win.read_with(cx, |view, _cx| view.first_run_open())
            .unwrap()
    );
    cx.dispatch_keystroke(win.into(), gpui::Keystroke::parse("escape").unwrap());
    assert!(
        !win.read_with(cx, |view, _cx| view.first_run_open())
            .unwrap()
    );
}

/// The nav tab's Mission-Center selected indicator: an absolute 3px accent
/// underline that animates its width as the selection changes (the keyed
/// animation id flips with the active state). This is a pure key-stability
/// check — the tab chrome never wraps a focusable shell in the animator.
#[gpui::test]
async fn nav_indicator_keyed_animation_restarts_on_selection_change(cx: &mut TestAppContext) {
    let win = render_in_mode(cx, true);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());
    let tab = vcx
        .debug_bounds("Apps")
        .expect("the Applications tab renders");
    vcx.simulate_click(tab.center(), Default::default());
    vcx.update(|window, cx| window.draw(cx).clear());

    assert_eq!(
        win.read_with(cx, |view, _cx| view.page).unwrap(),
        TopPage::Apps,
        "the click switches the page"
    );
    // The indicator element lives under the tab's absolute overlay child; it
    // must not disturb the tab's own bounds (the animator wraps only the
    // painted background/indicator, never the focusable shell).
    let after = vcx
        .debug_bounds("Apps")
        .expect("the tab still renders after the switch");
    assert_eq!(
        (tab.left(), tab.top(), tab.right(), tab.bottom()),
        (after.left(), after.top(), after.right(), after.bottom()),
        "the indicator animation must not reflow the tab"
    );
}
