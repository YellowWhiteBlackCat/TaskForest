//! test-intent: behavior
//!
//! Headless battery/fan and GPU chart-metric history tests for the Performance
//! page (P0-MC-01 / P0-MC-04). Split from `performance.rs` to respect the
//! per-file source budget; the mount/fold helpers stay in that module.

use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use taskmanager_application::i18n::t;
use taskmanager_core::core::DeviceGeneration;
use taskmanager_core::core::SystemSnapshot;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::sensors::SensorReading;

use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;
use crate::widgets::chart::MAX_CHART_POINTS;

use super::tests::{block_keys, headless_perf_app, route_to_performance};
use super::{
    GpuMetricButton, PerformanceDeviceFocus, PerformanceDeviceTarget, Section, gpu_devices,
};

// ---- battery/fan per-device history (P0-MC-01) -----------------------------

/// Build one fan `SensorReading` whose typed measurement projects to `FanRpm`.
fn fan_sensor_reading(id: &str, label: &str, device_id: &str, rpm: u32) -> SensorReading {
    use taskmanager_core::core::sensors::{
        SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorScale,
    };

    SensorReading::from_measurement_observation(
        device_id.into(),
        id.into(),
        label.into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(u64::from(rpm)),
            1_000,
        )
        .expect("valid fan magnitude"),
    )
    .with_device_generation(DeviceGeneration::new(1))
}

#[test]
fn battery_and_fan_blocks_paint_their_own_history_curve() {
    // A battery and an observed fan each carry their own recorded window; a
    // second fan with no samples keeps the labelled collecting state. The
    // mounted Performance page must bind each block's curve to that device's
    // OWN window — one segment per adjacent finite sample — never a sibling's
    // series or a fabricated flat line.
    use crate::pages::performance::device_curves::{
        DeviceCurve, DeviceCurveKind, DeviceCurveStatus,
    };
    use crate::widgets::chart::line_segments_scaled;
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::power::{
        BatteryInfo, BatteryScalarObservations, PowerSupplySnapshot,
    };
    use taskmanager_core::core::sensors::SensorCenterSnapshot;
    use taskmanager_shell::fixture::{
        ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
    };

    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    let mut battery = BatteryInfo::new("BAT0", DeviceState::healthy(10));
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(72, 10),
        ..Default::default()
    });
    let power = PowerSupplySnapshot {
        state: DeviceState::healthy(10),
        timestamp_ms: 10,
        batteries: vec![battery],
        ..PowerSupplySnapshot::default()
    };
    let fan1 = fan_sensor_reading("fan1", "cpu_fan", "hwmon:cpu", 1_500);
    let fan2 = fan_sensor_reading("fan2", "case_fan", "hwmon:case", 900);
    let recorded = SensorCenterSnapshot {
        state: DeviceState::healthy(1_000),
        timestamp_ms: 1_000,
        readings: vec![fan1.clone()],
        ..SensorCenterSnapshot::default()
    };
    let projected = SensorCenterSnapshot {
        state: DeviceState::healthy(1_000),
        timestamp_ms: 1_000,
        readings: vec![fan1.clone(), fan2.clone()],
        ..SensorCenterSnapshot::default()
    };

    {
        let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
        seed_projection_fact(
            shell,
            ProjectionSeedFact::PowerSupplies(Some(power.clone())),
        );
        let dynamic = SystemSnapshot {
            timestamp_ms: 1_000,
            ..Default::default()
        };
        for _ in 0..3 {
            record_demo_history_frame(shell, &dynamic, Some(&power), None);
        }
        for _ in 0..3 {
            record_demo_history_frame(shell, &dynamic, None, Some(&recorded));
        }
        // Project BOTH fans after fan1's window is warmed, so fan2 mounts but
        // stays unobserved.
        seed_projection_fact(shell, ProjectionSeedFact::Sensors(Some(projected)));
    }
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();

    assert_eq!(
        block_keys(app.world_mut(), Section::Fan),
        vec!["fan1".to_owned(), "fan2".to_owned()],
        "the fan section mounts one block per reported fan channel"
    );
    assert!(
        block_keys(app.world_mut(), Section::Battery).contains(&"BAT0".to_owned()),
        "the battery block mounts alongside the fan blocks"
    );

    let fan_curves: Vec<(Entity, DeviceCurve)> = {
        let world = app.world_mut();
        let mut curves = world.query::<(Entity, &DeviceCurve)>();
        curves
            .iter(world)
            .filter(|(_, curve)| curve.kind == DeviceCurveKind::FanRpm)
            .map(|(entity, curve)| (entity, curve.clone()))
            .collect()
    };
    let fan_curve = |id: &str| {
        fan_curves
            .iter()
            .find(|(_, curve)| curve.id == id)
            .map(|(_, curve)| curve)
            .unwrap_or_else(|| panic!("the {id} fan block mounts its own RPM curve"))
    };

    {
        let world = app.world();
        let shell = &world.non_send::<FrontendTrack>().shell;
        let samples1 = fan_curve("fan1").samples(shell);
        assert_eq!(
            samples1,
            vec![1_500.0, 1_500.0, 1_500.0],
            "the fan block's curve resolves that fan's own window"
        );
        assert_eq!(
            line_segments_scaled(&samples1, 100.0, 34.0, MAX_CHART_POINTS, 1_650.0).len(),
            2,
            "one polyline segment per adjacent finite sample"
        );
        let samples2 = fan_curve("fan2").samples(shell);
        assert!(
            samples2.is_empty(),
            "an unobserved fan channel has no fabricated window"
        );
        assert_eq!(
            line_segments_scaled(&samples2, 100.0, 34.0, MAX_CHART_POINTS, 100.0).len(),
            0,
            "an unobserved channel draws no segment"
        );
    }

    let collecting = t("perf.collecting_samples");
    let status = |world: &mut World, id: &str| -> Option<String> {
        let mut statuses = world.query::<(&DeviceCurveStatus, &Text)>();
        statuses
            .iter(world)
            .find(|(status, _)| status.0.kind == DeviceCurveKind::FanRpm && status.0.id == id)
            .map(|(_, text)| text.0.clone())
    };
    assert_eq!(
        status(app.world_mut(), "fan2").as_deref(),
        Some(collecting),
        "an unobserved channel keeps the labelled collecting state"
    );
    assert_ne!(
        status(app.world_mut(), "fan1").as_deref(),
        Some(collecting),
        "an observed channel leaves the collecting state for its summary"
    );
}

#[test]
fn gpu_curve_metric_selection_drives_the_shared_graph() {
    // Activating the GPU curve card's metric selector drives the shared shell
    // selection, the card's graph resolves the selected family's own window,
    // and a device-generation advance resets the selection to the default.
    use crate::pages::performance::device_curves::{DeviceCurve, DeviceCurveKind};
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_core::core::metrics::{
        GpuEngine, GpuEngineKind, GpuMetrics, GpuScalarObservations,
    };
    use taskmanager_shell::fixture::{
        ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
    };
    use taskmanager_shell::gpu_chart_metric_gate;
    use taskmanager_shell::presentation::gpu_chart_metric::{
        GpuChartMetric, gpu_chart_metric_history,
    };

    let device_id = "gpu:pci:0000:01:00.0";
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);

    // Seed one adapter with utilization + temperature + frequency samples so
    // more than the default family is available.
    let mut gpu = GpuMetrics::new(device_id, "Test GPU");
    // `record_demo_history_frame` scopes ingested GPU history to generation 1,
    // so the projected device must carry the same generation for the metric
    // window to resolve.
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.engines = vec![GpuEngine {
        name: "Render/3D".into(),
        kind: GpuEngineKind::Render,
        usage_pct: 40.0,
    }];
    for (index, (utilization, temperature, frequency)) in [
        (20.0_f32, 40.0_f32, 700_u64),
        (35.0, 45.0, 900),
        (50.0, 50.0, 1_100),
    ]
    .into_iter()
    .enumerate()
    {
        let timestamp_ms = 1_000 + index as u64 * 1_000;
        let mut frame = gpu.clone();
        frame.apply_scalar_observations(GpuScalarObservations {
            utilization_pct: ScalarObservation::available(utilization, timestamp_ms),
            temperature_c: ScalarObservation::available(temperature, timestamp_ms),
            frequency_mhz: ScalarObservation::available(frequency, timestamp_ms),
            ..Default::default()
        });
        let snapshot = SystemSnapshot {
            timestamp_ms,
            gpu: vec![frame],
            ..Default::default()
        };
        let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
        seed_projection_fact(
            shell,
            ProjectionSeedFact::Snapshot(Box::new(Some(snapshot.clone()))),
        );
        record_demo_history_frame(shell, &snapshot, None, None);
    }
    app.world_mut().resource_mut::<PerformanceDeviceFocus>().0 =
        PerformanceDeviceTarget::Gpu(device_id.to_owned());
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();

    let generation = {
        let world = app.world();
        let shell = &world.non_send::<FrontendTrack>().shell;
        let gpu = gpu_devices(shell)
            .and_then(|devices| devices.iter().find(|gpu| gpu.device_id == device_id))
            .expect("the seeded adapter is projected");
        let gate = gpu_chart_metric_gate(Some(gpu));
        assert!(
            shell.gpu_chart_metric_projection(&gate).selected == GpuChartMetric::Utilization,
            "the selection starts at the default family"
        );
        gpu.device_generation
    };

    // Activate the temperature selector through the real button path.
    let temperature_button = {
        let world = app.world_mut();
        let mut buttons = world.query::<(Entity, &GpuMetricButton)>();
        buttons
            .iter(world)
            .find(|(_, button)| button.0 == GpuChartMetric::Temperature)
            .map(|(entity, _)| entity)
            .expect("the temperature selector is mounted")
    };
    app.world_mut().commands().trigger(Activate {
        entity: temperature_button,
    });
    app.update();
    app.update();
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .gpu_chart_metric_selected(),
        GpuChartMetric::Temperature,
        "activating a selector drives the shared shell selection"
    );

    let gpu_curve = {
        let world = app.world_mut();
        let mut curves = world.query::<&DeviceCurve>();
        curves
            .iter(world)
            .find(|curve| curve.kind == DeviceCurveKind::GpuMetric && curve.id == device_id)
            .cloned()
            .expect("the GPU card mounts its selectable metric curve")
    };
    {
        let world = app.world();
        let shell = &world.non_send::<FrontendTrack>().shell;
        let selected = gpu_curve.samples(shell);
        assert!(
            selected.iter().any(|value| value.is_finite()),
            "the selected temperature family carries its real samples"
        );
        let power = gpu_chart_metric_history(
            &shell.history,
            device_id,
            generation.get(),
            GpuChartMetric::Power,
        );
        assert!(
            power.iter().all(|value| !value.is_finite()),
            "an unobserved family stays explicit gaps, never a fabricated line"
        );
    }

    // A device-generation advance resets the selection to the default.
    let mut bumped = gpu.clone();
    bumped.device_generation = DeviceGeneration::new(generation.get() + 1);
    let snapshot = SystemSnapshot {
        timestamp_ms: 9_000,
        gpu: vec![bumped],
        ..Default::default()
    };
    let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
    seed_projection_fact(
        shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();
    assert_eq!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "a device-generation advance resets the selection to the default"
    );
}
