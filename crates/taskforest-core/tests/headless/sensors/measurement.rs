use super::*;

#[test]
fn known_descriptor_rejects_contradictory_unit() {
    assert_eq!(
        SensorDescriptor::try_new(
            SensorQuantity::Temperature,
            SensorUnit::Watt,
            Some(SensorScale::MILLI),
        ),
        Err(SensorModelError::InvalidDescriptor)
    );
}

#[test]
fn invalid_wire_combination_is_rejected_during_deserialization() {
    let wire = serde_json::json!({
        "descriptor": {
            "quantity": "intrusion",
            "unit": "boolean",
            "source_scale": { "numerator": 1, "denominator": 1 }
        },
        "value": { "kind": "unsigned", "value": 1 },
        "availability": { "status": "available" },
        "last_success_ms": 42
    });

    assert!(serde_json::from_value::<SensorMeasurementObservation>(wire).is_err());
}

#[test]
fn opaque_quantity_and_unit_tokens_round_trip_without_guessed_scale() {
    let wire = serde_json::json!({
        "descriptor": {
            "quantity": "magnetic_flux_density",
            "unit": "microtesla",
            "source_scale": null
        },
        "value": { "kind": "signed", "value": -12 },
        "availability": { "status": "available" },
        "last_success_ms": 42
    });
    let decoded: SensorMeasurementObservation =
        serde_json::from_value(wire).expect("opaque sensor wire");

    assert_eq!(
        decoded.quantity(),
        &SensorQuantity::Opaque("magnetic_flux_density".into())
    );
    assert_eq!(decoded.unit(), &SensorUnit::Opaque("microtesla".into()));
    assert_eq!(decoded.source_scale(), None);
    assert_eq!(
        serde_json::to_value(decoded)
            .expect("serialize opaque sensor")
            .pointer("/descriptor/quantity")
            .and_then(serde_json::Value::as_str),
        Some("magnetic_flux_density")
    );
}

#[test]
fn fixed_point_scale_retains_exact_raw_and_exposes_normalized_number() {
    let observation = SensorMeasurementObservation::available(
        SensorDescriptor::voltage(SensorScale::MILLI),
        SensorMagnitude::Signed(12_345),
        42,
    )
    .expect("valid voltage");

    assert_eq!(
        observation.current_value(),
        Some(&SensorMagnitude::Signed(12_345))
    );
    assert_eq!(observation.current_number(), Some(12.345));
    assert_eq!(observation.last_success_ms(), Some(42));
}

#[test]
fn pwm_and_intrusion_keep_non_si_value_shapes() {
    let pwm = SensorMeasurementObservation::available(
        SensorDescriptor::pwm_duty_cycle(),
        SensorMagnitude::DutyCycle {
            value: 128,
            maximum: 255,
        },
        1,
    )
    .expect("valid PWM");
    let intrusion = SensorMeasurementObservation::available(
        SensorDescriptor::intrusion(),
        SensorMagnitude::Boolean(true),
        1,
    )
    .expect("valid intrusion");

    assert!(matches!(
        pwm.current_value(),
        Some(SensorMagnitude::DutyCycle {
            value: 128,
            maximum: 255
        })
    ));
    assert_eq!(
        intrusion.current_value(),
        Some(&SensorMagnitude::Boolean(true))
    );
}
