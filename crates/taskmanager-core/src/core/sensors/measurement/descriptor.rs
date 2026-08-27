//! Quantity, unit, fixed-point scale, and validated channel descriptors.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::SensorModelError;

/// Physical or logical quantity measured by one sensor channel.
///
/// Unknown wire tokens become `Opaque` and survive reserialization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SensorQuantity {
    Unknown,
    Temperature,
    FanSpeed,
    Power,
    Voltage,
    Current,
    Energy,
    RelativeHumidity,
    PwmDutyCycle,
    Intrusion,
    Opaque(String),
}

impl SensorQuantity {
    #[must_use]
    pub fn as_token(&self) -> &str {
        match self {
            Self::Unknown => "unknown",
            Self::Temperature => "temperature",
            Self::FanSpeed => "fan_speed",
            Self::Power => "power",
            Self::Voltage => "voltage",
            Self::Current => "current",
            Self::Energy => "energy",
            Self::RelativeHumidity => "relative_humidity",
            Self::PwmDutyCycle => "pwm_duty_cycle",
            Self::Intrusion => "intrusion",
            Self::Opaque(token) => token,
        }
    }

    fn from_token(token: String) -> Self {
        match token.as_str() {
            "unknown" => Self::Unknown,
            "temperature" => Self::Temperature,
            "fan_speed" => Self::FanSpeed,
            "power" => Self::Power,
            "voltage" => Self::Voltage,
            "current" => Self::Current,
            "energy" => Self::Energy,
            "relative_humidity" => Self::RelativeHumidity,
            "pwm_duty_cycle" => Self::PwmDutyCycle,
            "intrusion" => Self::Intrusion,
            _ => Self::Opaque(token),
        }
    }
}

impl Serialize for SensorQuantity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_token())
    }
}

impl<'de> Deserialize<'de> for SensorQuantity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_token)
    }
}

/// Unit in which a scaled magnitude is expressed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SensorUnit {
    Unknown,
    Celsius,
    RevolutionsPerMinute,
    Watt,
    Volt,
    Ampere,
    Joule,
    Percent,
    RawPwmDuty,
    Boolean,
    Opaque(String),
}

impl SensorUnit {
    #[must_use]
    pub fn as_token(&self) -> &str {
        match self {
            Self::Unknown => "unknown",
            Self::Celsius => "celsius",
            Self::RevolutionsPerMinute => "revolutions_per_minute",
            Self::Watt => "watt",
            Self::Volt => "volt",
            Self::Ampere => "ampere",
            Self::Joule => "joule",
            Self::Percent => "percent",
            Self::RawPwmDuty => "raw_pwm_duty",
            Self::Boolean => "boolean",
            Self::Opaque(token) => token,
        }
    }

    fn from_token(token: String) -> Self {
        match token.as_str() {
            "unknown" => Self::Unknown,
            "celsius" => Self::Celsius,
            "revolutions_per_minute" => Self::RevolutionsPerMinute,
            "watt" => Self::Watt,
            "volt" => Self::Volt,
            "ampere" => Self::Ampere,
            "joule" => Self::Joule,
            "percent" => Self::Percent,
            "raw_pwm_duty" => Self::RawPwmDuty,
            "boolean" => Self::Boolean,
            _ => Self::Opaque(token),
        }
    }
}

impl Serialize for SensorUnit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_token())
    }
}

impl<'de> Deserialize<'de> for SensorUnit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_token)
    }
}

/// Exact positive ratio applied to a raw numeric magnitude before its unit.
///
/// A missing scale is allowed only for opaque descriptors, where guessing a
/// conversion would be less truthful than retaining the raw unit token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "SensorScaleWire")]
pub struct SensorScale {
    numerator: u64,
    denominator: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct SensorScaleWire {
    numerator: u64,
    denominator: u64,
}

impl TryFrom<SensorScaleWire> for SensorScale {
    type Error = SensorModelError;

    fn try_from(wire: SensorScaleWire) -> Result<Self, Self::Error> {
        Self::ratio(wire.numerator, wire.denominator)
    }
}

impl SensorScale {
    pub const IDENTITY: Self = Self {
        numerator: 1,
        denominator: 1,
    };
    pub const MILLI: Self = Self {
        numerator: 1,
        denominator: 1_000,
    };
    pub const MICRO: Self = Self {
        numerator: 1,
        denominator: 1_000_000,
    };

    pub fn ratio(numerator: u64, denominator: u64) -> Result<Self, SensorModelError> {
        if numerator == 0 || denominator == 0 {
            return Err(SensorModelError::InvalidScale);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    #[must_use]
    pub const fn numerator(self) -> u64 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    pub(super) fn apply(self, raw: f64) -> Option<f64> {
        let numerator = self.numerator.to_string().parse::<f64>().ok()?;
        let denominator = self.denominator.to_string().parse::<f64>().ok()?;
        let product = raw * numerator;
        if !product.is_finite() || denominator == 0.0 {
            return None;
        }
        let scaled = product / denominator;
        scaled.is_finite().then_some(scaled)
    }
}

/// Validated channel metadata that remains present even when a read fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SensorDescriptorWire")]
pub struct SensorDescriptor {
    quantity: SensorQuantity,
    unit: SensorUnit,
    source_scale: Option<SensorScale>,
}

#[derive(Debug, Clone, Deserialize)]
struct SensorDescriptorWire {
    quantity: SensorQuantity,
    unit: SensorUnit,
    source_scale: Option<SensorScale>,
}

impl TryFrom<SensorDescriptorWire> for SensorDescriptor {
    type Error = SensorModelError;

    fn try_from(wire: SensorDescriptorWire) -> Result<Self, Self::Error> {
        Self::try_new(wire.quantity, wire.unit, wire.source_scale)
    }
}

impl Default for SensorDescriptor {
    fn default() -> Self {
        Self {
            quantity: SensorQuantity::Unknown,
            unit: SensorUnit::Unknown,
            source_scale: None,
        }
    }
}

impl SensorDescriptor {
    pub fn try_new(
        quantity: SensorQuantity,
        unit: SensorUnit,
        source_scale: Option<SensorScale>,
    ) -> Result<Self, SensorModelError> {
        validate_descriptor(&quantity, &unit, source_scale)?;
        Ok(Self {
            quantity,
            unit,
            source_scale,
        })
    }

    #[must_use]
    pub fn temperature(source_scale: SensorScale) -> Self {
        Self::known(
            SensorQuantity::Temperature,
            SensorUnit::Celsius,
            source_scale,
        )
    }

    #[must_use]
    pub fn fan_speed(source_scale: SensorScale) -> Self {
        Self::known(
            SensorQuantity::FanSpeed,
            SensorUnit::RevolutionsPerMinute,
            source_scale,
        )
    }

    #[must_use]
    pub fn power(source_scale: SensorScale) -> Self {
        Self::known(SensorQuantity::Power, SensorUnit::Watt, source_scale)
    }

    #[must_use]
    pub fn voltage(source_scale: SensorScale) -> Self {
        Self::known(SensorQuantity::Voltage, SensorUnit::Volt, source_scale)
    }

    #[must_use]
    pub fn current(source_scale: SensorScale) -> Self {
        Self::known(SensorQuantity::Current, SensorUnit::Ampere, source_scale)
    }

    #[must_use]
    pub fn energy(source_scale: SensorScale) -> Self {
        Self::known(SensorQuantity::Energy, SensorUnit::Joule, source_scale)
    }

    #[must_use]
    pub fn relative_humidity(source_scale: SensorScale) -> Self {
        Self::known(
            SensorQuantity::RelativeHumidity,
            SensorUnit::Percent,
            source_scale,
        )
    }

    #[must_use]
    pub fn pwm_duty_cycle() -> Self {
        Self::known(
            SensorQuantity::PwmDutyCycle,
            SensorUnit::RawPwmDuty,
            SensorScale::IDENTITY,
        )
    }

    #[must_use]
    pub fn intrusion() -> Self {
        Self::known(
            SensorQuantity::Intrusion,
            SensorUnit::Boolean,
            SensorScale::IDENTITY,
        )
    }

    pub fn opaque(
        quantity_token: String,
        unit: SensorUnit,
        source_scale: Option<SensorScale>,
    ) -> Result<Self, SensorModelError> {
        Self::try_new(SensorQuantity::Opaque(quantity_token), unit, source_scale)
    }

    fn known(quantity: SensorQuantity, unit: SensorUnit, source_scale: SensorScale) -> Self {
        Self {
            quantity,
            unit,
            source_scale: Some(source_scale),
        }
    }

    #[must_use]
    pub const fn quantity(&self) -> &SensorQuantity {
        &self.quantity
    }

    #[must_use]
    pub const fn unit(&self) -> &SensorUnit {
        &self.unit
    }

    #[must_use]
    pub const fn source_scale(&self) -> Option<SensorScale> {
        self.source_scale
    }
}

fn validate_descriptor(
    quantity: &SensorQuantity,
    unit: &SensorUnit,
    source_scale: Option<SensorScale>,
) -> Result<(), SensorModelError> {
    if let SensorQuantity::Opaque(token) = quantity
        && token.trim().is_empty()
    {
        return Err(SensorModelError::EmptyOpaqueToken);
    }
    if let SensorUnit::Opaque(token) = unit
        && token.trim().is_empty()
    {
        return Err(SensorModelError::EmptyOpaqueToken);
    }
    let expected_unit = match quantity {
        SensorQuantity::Unknown => {
            return (unit == &SensorUnit::Unknown && source_scale.is_none())
                .then_some(())
                .ok_or(SensorModelError::InvalidDescriptor);
        }
        SensorQuantity::Temperature => SensorUnit::Celsius,
        SensorQuantity::FanSpeed => SensorUnit::RevolutionsPerMinute,
        SensorQuantity::Power => SensorUnit::Watt,
        SensorQuantity::Voltage => SensorUnit::Volt,
        SensorQuantity::Current => SensorUnit::Ampere,
        SensorQuantity::Energy => SensorUnit::Joule,
        SensorQuantity::RelativeHumidity => SensorUnit::Percent,
        SensorQuantity::PwmDutyCycle => SensorUnit::RawPwmDuty,
        SensorQuantity::Intrusion => SensorUnit::Boolean,
        SensorQuantity::Opaque(_) => return Ok(()),
    };
    if unit != &expected_unit || source_scale.is_none() {
        return Err(SensorModelError::InvalidDescriptor);
    }
    if matches!(
        quantity,
        SensorQuantity::PwmDutyCycle | SensorQuantity::Intrusion
    ) && source_scale != Some(SensorScale::IDENTITY)
    {
        return Err(SensorModelError::InvalidDescriptor);
    }
    Ok(())
}
