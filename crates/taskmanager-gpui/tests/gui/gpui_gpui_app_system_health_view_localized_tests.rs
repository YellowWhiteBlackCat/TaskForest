//! Pins the system-health section headings to their shared catalog keys.
//!
//! The thermal-zone group is a system surface, not the device's own
//! temperature channel: heading it with `common.temperature` duplicated the
//! quantity word the same page uses for a device reading (the ambiguity the
//! TUI sibling fixed with `common.thermal_zones`). These tests assert the
//! resolved catalog string in both locales through the production resolver,
//! never the source key literal.

use super::{SensorGroup, SystemHealthText, localized_text};
use taskmanager_application::i18n;

#[test]
fn thermal_zone_heading_resolves_the_shared_zone_key_and_not_the_temperature_quantity() {
    // The shared i18n language is a process-wide global, so pin both locales
    // and restore the prior value (the established GPUI locale-test pattern).
    let prior = i18n::current_language();

    i18n::set_language(i18n::Language::En);
    let en_heading = localized_text(SystemHealthText::SensorGroup(SensorGroup::Temperature));
    let en_zones = i18n::t("common.thermal_zones");
    let en_temperature = i18n::t("common.temperature");

    i18n::set_language(i18n::Language::Zh);
    let zh_heading = localized_text(SystemHealthText::SensorGroup(SensorGroup::Temperature));
    let zh_zones = i18n::t("common.thermal_zones");
    let zh_temperature = i18n::t("common.temperature");

    i18n::set_language(prior);

    assert_eq!(
        en_heading, en_zones,
        "the English zone heading must resolve `common.thermal_zones`"
    );
    assert_eq!(
        zh_heading, zh_zones,
        "the Chinese zone heading must resolve `common.thermal_zones`"
    );
    assert_eq!(en_heading, "Thermal zones");
    assert_eq!(zh_heading, "温度区");
    assert_ne!(
        en_heading, en_temperature,
        "the zone heading must not reuse the device temperature quantity word"
    );
    assert_ne!(
        zh_heading, zh_temperature,
        "the zone heading must not reuse the device temperature quantity word"
    );
}

/// The power sensor group is a system surface, not the device's own power
/// draw: heading it with `common.power` duplicated the quantity word the same
/// page uses for a watt reading (the ambiguity the thermal-zone fix removed).
/// The heading must resolve the shared `common.power_sensors` surface key in
/// both locales and differ from the `common.power` quantity label; the
/// capture-path map carries the same English surface name.
#[test]
fn power_sensor_heading_resolves_the_surface_key_and_not_the_power_quantity() {
    let prior = i18n::current_language();

    i18n::set_language(i18n::Language::En);
    let en_heading = localized_text(SystemHealthText::SensorGroup(SensorGroup::Power));
    let en_surface = i18n::t("common.power_sensors");
    let en_power = i18n::t("common.power");

    i18n::set_language(i18n::Language::Zh);
    let zh_heading = localized_text(SystemHealthText::SensorGroup(SensorGroup::Power));
    let zh_surface = i18n::t("common.power_sensors");
    let zh_power = i18n::t("common.power");

    i18n::set_language(prior);

    assert_eq!(
        en_heading, en_surface,
        "the English power heading must resolve `common.power_sensors`"
    );
    assert_eq!(
        zh_heading, zh_surface,
        "the Chinese power heading must resolve `common.power_sensors`"
    );
    assert_eq!(en_heading, "Power sensors");
    assert_eq!(zh_heading, "功耗传感器");
    assert_ne!(
        en_heading, en_power,
        "the power heading must not reuse the power quantity word"
    );
    assert_ne!(
        zh_heading, zh_power,
        "the power heading must not reuse the power quantity word"
    );
    assert_eq!(
        crate::gpui_app::system_health_view::capture_english_text(SystemHealthText::SensorGroup(
            SensorGroup::Power
        )),
        "Power sensors",
        "the capture-path map must carry the same surface name"
    );
}
