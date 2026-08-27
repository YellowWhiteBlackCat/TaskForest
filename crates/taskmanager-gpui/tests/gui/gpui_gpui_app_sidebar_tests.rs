use crate::core::metrics::{
    GpuMetrics, GpuScalarObservations, NetworkAdapterType, NetworkScalarObservations,
    NetworkWirelessObservations, OptionalObservation, ScalarObservation, SystemSnapshot,
};
use crate::gpui_app::formatting::DisplayUnits;

use super::{
    NetworkVisibility, cpu_caption, gpu_caption_line1, gpu_caption_line2, nic_caption_line2,
};

#[test]
fn network_visibility_keeps_categories_independent_and_maps_loopback_to_other() {
    let visibility = NetworkVisibility {
        all: true,
        wired: false,
        wireless: true,
        vpn: false,
        virtual_devices: true,
        other: false,
    };
    assert!(!visibility.allows(NetworkAdapterType::Ethernet));
    assert!(visibility.allows(NetworkAdapterType::WiFi));
    assert!(!visibility.allows(NetworkAdapterType::Vpn));
    assert!(visibility.allows(NetworkAdapterType::Virtual));
    assert!(!visibility.allows(NetworkAdapterType::Loopback));
    assert!(!visibility.allows(NetworkAdapterType::Other));
    assert!(
        !NetworkVisibility {
            all: false,
            ..visibility
        }
        .allows(NetworkAdapterType::WiFi)
    );
}

#[test]
fn gpu_sidebar_distinguishes_unknown_from_measured_zero() {
    let unknown = GpuMetrics::default();
    assert_eq!(gpu_caption_line2(&unknown), "—");

    let idle = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(0.0, 5),
        ..Default::default()
    });
    assert_eq!(gpu_caption_line2(&idle), "0%");
}

#[test]
fn gpu_caption_uses_only_current_typed_observation_truth() {
    let measured = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(12.0, 5),
        temperature_c: ScalarObservation::available(41.0, 5),
        ..Default::default()
    });
    assert_eq!(gpu_caption_line2(&measured), "12%  ·  41 °C");

    let failed = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::unavailable(crate::core::FailureKind::PermissionDenied),
        ..Default::default()
    });
    assert_eq!(gpu_caption_line2(&failed), "—");
}

#[test]
fn vram_captions_follow_current_dedicated_observations() {
    let wired_zero_but_observed = GpuMetrics::from_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(2 << 30, 5),
        dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 5),
        ..Default::default()
    });
    assert_eq!(
        gpu_caption_line1(&wired_zero_but_observed, DisplayUnits::default()),
        "VRAM 2.00 GiB / 8.00 GiB"
    );
    assert!(gpu_caption_line2(&wired_zero_but_observed).contains("VRAM 25%"));

    let stale_dedicated = GpuMetrics::from_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::unavailable(
            crate::core::FailureKind::PermissionDenied,
        ),
        dedicated_vram_total_bytes: ScalarObservation::unavailable(
            crate::core::FailureKind::PermissionDenied,
        ),
        ..Default::default()
    });
    assert!(gpu_caption_line1(&stale_dedicated, DisplayUnits::default()).is_empty());
    assert!(!gpu_caption_line1(&stale_dedicated, DisplayUnits::default()).contains("VRAM"));
    assert!(!gpu_caption_line2(&stale_dedicated).contains("VRAM"));
}

#[test]
fn cpu_caption_renders_dash_when_typed_usage_is_unavailable() {
    let snapshot = SystemSnapshot {
        cpu: crate::core::metrics::CpuMetrics::from_observations(
            crate::core::metrics::CpuScalarObservations {
                global_usage_pct: ScalarObservation::unavailable(
                    crate::core::FailureKind::PermissionDenied,
                ),
                ..Default::default()
            },
        ),
        ..Default::default()
    };
    assert_eq!(cpu_caption(&snapshot).1, "—");

    let measured = SystemSnapshot {
        cpu: crate::core::metrics::CpuMetrics::from_observations(
            crate::core::metrics::CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(42.0, 9),
                ..Default::default()
            },
        ),
        ..Default::default()
    };
    assert_eq!(cpu_caption(&measured).1, "42%");

    let measured_with_temp = SystemSnapshot {
        cpu: crate::core::metrics::CpuMetrics::from_observations(
            crate::core::metrics::CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(42.0, 9),
                temperature_c: ScalarObservation::available(55.0, 9),
                ..Default::default()
            },
        ),
        ..Default::default()
    };
    assert!(
        cpu_caption(&measured_with_temp)
            .1
            .starts_with("42% (55 °C)")
    );
}

#[test]
fn nic_caption_uses_typed_wireless_and_link_truth() {
    let wifi = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .interface_name("wlan0".into())
        .adapter_type(NetworkAdapterType::WiFi)
        .ssid_observation(match Some("office".into()) {
            Some(value) => taskmanager_core::OptionalObservation::present(value, 1),
            None => taskmanager_core::OptionalObservation::default(),
        })
        .signal_observation(match Some(-50) {
            Some(value) => taskmanager_core::OptionalObservation::present(value, 1),
            None => taskmanager_core::OptionalObservation::default(),
        })
        .link_speed_observation(match Some(866) {
            Some(value) => taskmanager_core::ScalarObservation::available(value, 1),
            None => taskmanager_core::ScalarObservation::default(),
        })
        .wireless_observations(NetworkWirelessObservations {
            ssid: OptionalObservation::present("office".into(), 7),
            signal_dbm: OptionalObservation::present(-50, 7),
            ..Default::default()
        })
        .scalar_observations(NetworkScalarObservations {
            link_speed_mbps: ScalarObservation::available(866, 7),
            ..Default::default()
        })
        .build();
    assert_eq!(nic_caption_line2(&wifi), "office  ·  67%  ·  866 Mbps");

    let wired = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .interface_name("enp3s0".into())
        .link_speed_observation(match Some(1000) {
            Some(value) => taskmanager_core::ScalarObservation::available(value, 1),
            None => taskmanager_core::ScalarObservation::default(),
        })
        .scalar_observations(NetworkScalarObservations {
            link_speed_mbps: ScalarObservation::available(1000, 7),
            ..Default::default()
        })
        .build();
    assert_eq!(nic_caption_line2(&wired), "enp3s0  ·  1000 Mbps");
}

#[gpui::test]
async fn long_device_identity_cannot_expand_the_configured_sidebar_width(
    cx: &mut gpui::TestAppContext,
) {
    use crate::gpui_app::root::{RootView, TopPage};
    use crate::gpui_app::theme::Theme;
    use gpui::{AppContext, Modifiers, MouseButton, VisualTestContext, point, px, size};

    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    cx.simulate_window_resize(*win, size(px(1280.0), px(720.0)));
    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.resize_sidebar(px(276.0), cx);
        let mut cpu = crate::core::metrics::CpuMetrics::default();
        cpu.brand = Some("A deliberately overlong provider CPU identity that must truncate".into());
        view.replace_system_snapshot_for_test(SystemSnapshot {
            cpu,
            disks: vec![
                taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                    .name("/dev/nvme0n1".into())
                    .model(
                        "A deliberately overlong storage model that must never resize chrome"
                            .into(),
                    )
                    .build(),
            ],
            ..Default::default()
        });
        cx.notify();
    })
    .unwrap();
    cx.update_window(*win, |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(*win, cx);
    let frame = visual
        .debug_bounds("tm-sidebar-fixed-width-frame")
        .expect("the sidebar width authority must remain addressable");
    let viewport = visual
        .debug_bounds("tm-sidebar-scroll")
        .expect("the sidebar viewport must remain mounted");
    let rail = visual
        .debug_bounds("tm-sidebar-scrollbar")
        .expect("the sidebar rail must remain mounted");
    let resize = visual
        .debug_bounds("tm-sidebar-resize-handle")
        .expect("the resize gutter must remain mounted");
    assert_eq!(frame.size.width, px(276.0));
    assert_eq!(viewport.size.width, px(268.0));
    assert!(rail.right() <= resize.left());
    assert_eq!(resize.right(), frame.right());

    let start = resize.center();
    visual.simulate_mouse_move(start, None, Modifiers::none());
    visual.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(
        point(start.x + px(2.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_move(
        point(start.x + px(42.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    // Crossing GPUI's drag threshold creates the typed drag payload. The next
    // move is the first `DragMoveEvent` and captures the production anchor;
    // only subsequent movement is a resize delta.
    visual.simulate_mouse_move(
        point(start.x + px(43.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_move(
        point(start.x + px(83.0), start.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    visual.simulate_mouse_up(
        point(start.x + px(83.0), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(
        win.read_with(cx, |view, _cx| {
            view.presentation_snapshot().sidebar_width()
        })
        .expect("sidebar root must remain alive after drag"),
        px(316.0),
        "a real pointer drag in the dedicated gutter must resize the sidebar"
    );
}
