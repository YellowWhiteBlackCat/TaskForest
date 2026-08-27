use super::*;
#[test]
fn classifies_all_standard_channel_families_without_vendor_tables() {
    for (name, quantity, unit, scale) in [
        (
            "temp1_input",
            SensorQuantity::Temperature,
            SensorUnit::Celsius,
            SensorScale::MILLI,
        ),
        (
            "fan1_input",
            SensorQuantity::FanSpeed,
            SensorUnit::RevolutionsPerMinute,
            SensorScale::IDENTITY,
        ),
        (
            "power1_input",
            SensorQuantity::Power,
            SensorUnit::Watt,
            SensorScale::MICRO,
        ),
        (
            "in0_input",
            SensorQuantity::Voltage,
            SensorUnit::Volt,
            SensorScale::MILLI,
        ),
        (
            "curr1_input",
            SensorQuantity::Current,
            SensorUnit::Ampere,
            SensorScale::MILLI,
        ),
        (
            "energy1_input",
            SensorQuantity::Energy,
            SensorUnit::Joule,
            SensorScale::MICRO,
        ),
        (
            "humidity1_input",
            SensorQuantity::RelativeHumidity,
            SensorUnit::Percent,
            SensorScale::MILLI,
        ),
        (
            "pwm1",
            SensorQuantity::PwmDutyCycle,
            SensorUnit::RawPwmDuty,
            SensorScale::IDENTITY,
        ),
        (
            "intrusion0_alarm",
            SensorQuantity::Intrusion,
            SensorUnit::Boolean,
            SensorScale::IDENTITY,
        ),
    ] {
        let channel = parse_channel(name).expect("standard hwmon channel");
        assert_eq!(channel.descriptor.quantity(), &quantity);
        assert_eq!(channel.descriptor.unit(), &unit);
        assert_eq!(channel.descriptor.source_scale(), Some(scale));
    }
}

#[test]
fn similar_control_and_threshold_files_are_not_misclassified() {
    for name in [
        "pwm1_enable",
        "pwm1_mode",
        "temp1_max",
        "power1_average",
        "intrusion0_beep",
        "fan0_input",
        "curr0_input",
    ] {
        assert_eq!(parse_channel(name), None, "{name}");
    }
}

#[test]
fn unfamiliar_input_family_is_retained_as_opaque_raw_integer() {
    let channel = parse_channel("flux_density17_input").expect("opaque input channel");

    assert_eq!(
        channel.descriptor.quantity(),
        &SensorQuantity::Opaque("flux_density".into())
    );
    assert_eq!(
        channel.descriptor.unit(),
        &SensorUnit::Opaque("raw_hwmon_flux_density".into())
    );
    assert_eq!(channel.descriptor.source_scale(), None);
    assert_eq!(
        parse_magnitude(&channel.descriptor, "-42"),
        Some(SensorMagnitude::Signed(-42))
    );
}

#[test]
fn parses_integer_boolean_and_raw_duty_shapes_without_float_guessing() {
    let voltage = parse_channel("in0_input").expect("voltage");
    let pwm = parse_channel("pwm1").expect("PWM");
    let intrusion = parse_channel("intrusion0_alarm").expect("intrusion");

    assert_eq!(
        parse_magnitude(&voltage.descriptor, "-12000"),
        Some(SensorMagnitude::Signed(-12_000))
    );
    assert_eq!(
        parse_magnitude(&pwm.descriptor, "128"),
        Some(SensorMagnitude::DutyCycle {
            value: 128,
            maximum: 255,
        })
    );
    assert_eq!(
        parse_magnitude(&intrusion.descriptor, "1"),
        Some(SensorMagnitude::Boolean(true))
    );
    assert_eq!(parse_magnitude(&pwm.descriptor, "256"), None);
    assert_eq!(parse_magnitude(&intrusion.descriptor, "2"), None);
}
