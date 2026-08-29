//! Dynamic-device render checks kept separate from the static Performance page
//! geometry tests so each test module remains a single semantic family.

use gpui::{TestAppContext, VisualTestContext};

use crate::gpui_app::root::TopPage;
use crate::gpui_app::sidebar::SelectedDevice;
use taskmanager_core::core::{
    BatteryInfo, DeviceGeneration, DeviceId, DeviceState, PowerSupplySnapshot,
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorScale,
};

use super::{draw, sensor_reading, with_battery_scalars, wrapped_root};

#[gpui::test]
async fn mc01_dynamic_readout_case_battery_and_fan_pages_paint_typed_dynamic_device_data(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        let battery = with_battery_scalars(
            {
                let mut battery = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(10));
                battery.display_name = "Internal battery".into();
                battery.device_generation = DeviceGeneration::new(1);
                battery
            },
            10,
            73,
            12.5,
        );
        let battery_snapshot = PowerSupplySnapshot {
            timestamp_ms: 10,
            batteries: vec![battery.clone()],
            ..Default::default()
        };
        v.replace_dynamic_devices_for_test(
            SensorCenterSnapshot::default(),
            battery_snapshot.clone(),
        );
        v.telemetry_ingestor
            .ingest_correlated_power_supplies(
                taskmanager_telemetry_store::CorrelatedTelemetryStamp::from_accepted_event(1, 20)
                    .expect("fixture revision is non-zero"),
                &battery_snapshot,
            )
            .expect("battery fixture enters dynamic history");
        v.selected = SelectedDevice::Battery(0);
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(vcx.debug_bounds("tm-perf-title").is_some());
    assert!(vcx.debug_bounds("tm-perf-stat:0").is_some());

    view.update(cx, |v, cx| {
        let fan = sensor_reading(
            DeviceId::new("hwmon:pwm"),
            "hwmon:pwm:fan1_input",
            "CPU fan",
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(1_380),
            30,
            2,
        );
        let fan_snapshot = SensorCenterSnapshot {
            timestamp_ms: 30,
            readings: vec![fan],
            ..Default::default()
        };
        let power_supplies = v.power_supplies().clone();
        v.replace_dynamic_devices_for_test(fan_snapshot.clone(), power_supplies);
        v.telemetry_ingestor
            .ingest_correlated_sensors(
                taskmanager_telemetry_store::CorrelatedTelemetryStamp::from_accepted_event(1, 40)
                    .expect("fixture revision is non-zero"),
                &fan_snapshot,
            )
            .expect("fan fixture enters dynamic history");
        v.selected = SelectedDevice::Fan(0);
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    assert!(vcx.debug_bounds("tm-perf-title").is_some());
    assert!(vcx.debug_bounds("tm-perf-stat:0").is_some());
}
