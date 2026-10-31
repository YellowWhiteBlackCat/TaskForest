//! test-intent: behavior
//!
//! The device charts' contract:
//!
//! - a two-series chart draws BOTH directions in ONE coordinate system, so
//!   the read/write pair and the receive/send pair share one scale;
//! - the legend names every series and the readout converts the byte rate
//!   through the shared byte formatter;
//! - the battery's charge curve wears the charging flow while the charge
//!   actually rises — and never while it falls.

use super::*;

use crate::palette::ui_palette;
use crate::widgets::chart::canvas::{CanvasFlow, CanvasSpec, rasterize};
use crate::widgets::chart::geometry::{ChartRun, ChartVertex};
use bevy::color::Color;
use taskforest_core::core::DeviceGeneration;
use taskforest_core::core::metrics::{
    DiskMetrics, DiskScalarObservations, ScalarObservation, SystemSnapshot,
};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::record_demo_history_frame;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

/// One disk frame with the given read and write rates.
fn disk_frame(device_id: &str, read: u64, write: u64, timestamp_ms: u64) -> SystemSnapshot {
    let mut disk = DiskMetrics::new(device_id);
    disk.device_id = device_id.to_owned();
    disk.device_generation = DeviceGeneration::new(1);
    disk.apply_scalar_observations(DiskScalarObservations {
        read_bytes_per_sec: ScalarObservation::available(read, timestamp_ms),
        write_bytes_per_sec: ScalarObservation::available(write, timestamp_ms),
        ..DiskScalarObservations::default()
    });
    SystemSnapshot {
        timestamp_ms,
        disks: vec![disk],
        ..SystemSnapshot::default()
    }
}

/// A shell whose disk history carries the recorded frames.
fn shell_with_disk_history(frames: &[SystemSnapshot]) -> ShellApp {
    let mut shell = ShellApp::new();
    for frame in frames {
        record_demo_history_frame(&mut shell, frame, None, None);
    }
    shell
}

#[test]
fn the_disk_chart_draws_read_and_write_in_one_coordinate_system() {
    // The disk's own window feeds BOTH directions: one chart, one scale, two
    // curves — never two separate charts and never a sibling device's window.
    let frames = vec![
        disk_frame("sda", 1_024, 2_048, 1_000),
        disk_frame("sda", 2_048, 4_096, 2_000),
        disk_frame("sda", 4_096, 8_192, 3_000),
    ];
    let shell = shell_with_disk_history(&frames);
    let curve = DeviceCurve {
        kind: DeviceCurveKind::DiskThroughput,
        id: "sda".to_owned(),
        generation: DeviceGeneration::new(1),
        color: Color::WHITE,
    };
    let series = curve.series(&shell);
    assert_eq!(series.len(), 2, "the chart carries both directions");
    assert_eq!(series[0], vec![1_024.0, 2_048.0, 4_096.0]);
    assert_eq!(series[1], vec![2_048.0, 4_096.0, 8_192.0]);

    // ONE ceiling for both series: they share the coordinate system.
    let ceiling = curve.ceiling(&series, &shell);
    assert!(
        ceiling >= 8_192.0,
        "the shared ceiling covers the measured peak (ceiling {ceiling})"
    );
}

#[test]
fn the_interface_chart_draws_receive_and_send_in_one_coordinate_system() {
    // The network chart is the same shape as the disk one: both directions
    // in one coordinate system.
    let curve = DeviceCurve {
        kind: DeviceCurveKind::NetworkThroughput,
        id: "eth0".to_owned(),
        generation: DeviceGeneration::new(1),
        color: Color::WHITE,
    };
    let plans = curve.kind.plans(
        &ShellApp::new(),
        &ui_palette(&Theme::dark(), UiSize::Standard),
    );
    assert_eq!(plans.len(), 2, "the network chart names both directions");
    assert_eq!(plans[0].label, t("net.receive"));
    assert_eq!(plans[1].label, t("net.send"));
}

#[test]
fn the_legend_names_every_series_and_the_readout_converts_the_rate() {
    // The disk legend reads "Read" and "Write"; the readout converts the
    // byte rate through the shared byte formatter.
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let plans = DeviceCurveKind::DiskThroughput.plans(&ShellApp::new(), &palette);
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].label, t("disk.read"));
    assert_eq!(plans[1].label, t("disk.write"));
    assert_eq!((plans[0].format)(1_024.0), "1.0 KiB/s");
    assert_eq!((plans[0].format)(2_097_152.0), "2.0 MiB/s");

    // The secondary series wears a tint of the SAME color — never a new
    // product color.
    let primary = plans[0].color.to_srgba();
    let secondary = plans[1].color.to_srgba();
    assert!(
        secondary.red > primary.red
            && secondary.green > primary.green
            && secondary.blue > primary.blue,
        "the secondary ink is lifted toward white"
    );
    assert!(
        secondary.red <= 1.0 && secondary.green <= 1.0 && secondary.blue <= 1.0,
        "the secondary ink stays inside the color range"
    );
}

#[test]
fn the_battery_charge_curve_flows_only_while_the_charge_rises() {
    // A rising charge wears the travelling flow; a falling charge does not.
    let curve = DeviceCurve {
        kind: DeviceCurveKind::BatteryCharge,
        id: "BAT0".to_owned(),
        generation: DeviceGeneration::new(1),
        color: Color::WHITE,
    };
    let rising = vec![vec![40.0_f32, 41.0, 42.0]];
    let falling = vec![vec![42.0_f32, 41.0, 40.0]];
    assert!(curve.charging(&rising), "a rising charge runs the flow");
    assert!(!curve.charging(&falling), "a falling charge does not");
    assert!(
        !curve.charging(&[vec![40.0_f32, 40.0]]),
        "a flat charge does not run the flow"
    );
}

/// The ink of one pixel of a raster, in sRGB bytes.
fn rgb_at(image: &crate::widgets::chart::canvas::CanvasImage, x: u32, y: u32) -> [u8; 3] {
    if x >= image.width || y >= image.height {
        return [0, 0, 0];
    }
    let index = ((y * image.width + x) * 4) as usize;
    [
        image.pixels.get(index).copied().unwrap_or(0),
        image.pixels.get(index + 1).copied().unwrap_or(0),
        image.pixels.get(index + 2).copied().unwrap_or(0),
    ]
}

#[test]
fn the_flow_brightens_the_curve_where_it_runs() {
    // The flow is a travelling highlight ON the curve: the ink under the
    // band is brighter than the same pixels without it.
    let runs = vec![ChartRun {
        vertices: vec![
            ChartVertex { x: 0.0, y: 20.0 },
            ChartVertex { x: 60.0, y: 20.0 },
        ],
    }];
    let base = CanvasSpec {
        width: 60,
        height: 40,
        runs: runs.clone(),
        thickness: 4.0,
        line: Color::srgba(0.2, 0.4, 0.9, 1.0),
        fill: None,
        grid: None,
        crosshair: None,
        flow: None,
    };
    let plain = rasterize(&base);
    let mut flowing = base.clone();
    flowing.flow = Some(CanvasFlow {
        color: Color::srgba(0.75, 0.85, 0.98, 1.0),
        phase: 0.5,
        radius: 8.0,
    });
    let lit = rasterize(&flowing);
    let under_band = rgb_at(&lit, 30, 20);
    let without_band = rgb_at(&plain, 30, 20);
    assert!(
        under_band[0] > without_band[0] && under_band[2] > without_band[2],
        "the flow brightens the curve under the band ({under_band:?} vs {without_band:?})"
    );
    assert_eq!(
        rgb_at(&lit, 2, 20),
        rgb_at(&plain, 2, 20),
        "the flow leaves the far end of the curve alone"
    );
}
