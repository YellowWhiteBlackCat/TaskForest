//! The Performance-page Fan-section behavior tests, extracted from
//! [`super`] so the device-test module stays under the source-size budget.
//! Moved verbatim; the assertions are unchanged.

use super::super::{PerfDetail, perf_detail_kind};
use super::*;

#[test]
fn perf_detail_kind_maps_the_fan_selector_to_its_panel() {
    assert_eq!(perf_detail_kind(PerfDevice::Fan(0)), PerfDetail::Fan);
}

#[test]
fn fan_summary_lines_project_typed_values_and_honest_dashes() {
    use crate::ui::fan::{fan_section_state, fan_summary_lines};
    use taskmanager_application::i18n::{Language, set_language};
    set_language(Language::En);
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::identity::DeviceGeneration;
    use taskmanager_core::core::sensors::{
        SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
        SensorReading, SensorScale,
    };

    let reading =
        |id: &str, label: &str, descriptor: SensorDescriptor, magnitude: SensorMagnitude| {
            SensorReading::from_measurement_observation(
                "hwmon:cpu".into(),
                id.into(),
                label.into(),
                SensorMeasurementObservation::available(descriptor, magnitude, 1_000)
                    .expect("valid sensor fixture"),
            )
            .with_device_generation(DeviceGeneration::new(1))
        };

    // No sensor snapshot → Loading; a fanless snapshot → Empty.
    assert_eq!(fan_section_state(None), tables::ListState::Loading);
    let fanless = SensorCenterSnapshot {
        readings: vec![reading(
            "temp1",
            "cpu_temp",
            SensorDescriptor::temperature(SensorScale::IDENTITY),
            SensorMagnitude::Decimal(40.0),
        )],
        ..Default::default()
    };
    assert_eq!(fan_section_state(Some(&fanless)), tables::ListState::Empty);

    // A healthy fan channel projects RPM + PWM + device temperature through
    // the typed accessors; a cold fan channel renders an honest dash.
    let fan = reading(
        "fan1",
        "cpu_fan",
        SensorDescriptor::fan_speed(SensorScale::IDENTITY),
        SensorMagnitude::Unsigned(2_400),
    );
    let pwm = reading(
        "pwm1",
        "fan1_pwm",
        SensorDescriptor::pwm_duty_cycle(),
        SensorMagnitude::DutyCycle {
            value: 60,
            maximum: 255,
        },
    );
    let temperature = reading(
        "temp1",
        "cpu_temp",
        SensorDescriptor::temperature(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(54.5),
    );
    let sensors = SensorCenterSnapshot {
        state: DeviceState::healthy(1_000),
        timestamp_ms: 1_000,
        readings: vec![fan, pwm, temperature],
        ..Default::default()
    };
    assert_eq!(fan_section_state(Some(&sensors)), tables::ListState::Ready);
    let rows = fan_summary_lines(&sensors, &sensors.readings[0]);
    assert_eq!(rows[0].label(), "Speed");
    assert_eq!(rows[0].value(), Some("2400 RPM"));
    assert!(
        rows.iter()
            .any(|row| row.label() == "PWM" && row.value() == Some("24%")),
        "duty cycle must project as percent of maximum: {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.label() == "Temperature cpu_temp" && row.value() == Some("54.5 °C")),
        "device temperature must project: {rows:?}"
    );
}
