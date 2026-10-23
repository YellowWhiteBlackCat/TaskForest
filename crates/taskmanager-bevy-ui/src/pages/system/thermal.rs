//! Thermal-zone sensor traversal and presentation for the System page.
//!
//! Delivers true parity for `FeatureId::ThermalZoneSensors` (`power.thermal-zones`).
//! Traverses every `SensorQuantity::Temperature` reading in the shared `SensorCenterSnapshot`,
//! naming each reading's source label and formatting its temperature honestly.
//!
//! Two rules shape the presentation:
//!
//! * a reading above the hot threshold is pinned to the top of its group and
//!   washed with the theme's danger ink, so a hot spot is the first thing the
//!   page states — and a snapshot without such a reading pins nothing;
//! * a discrete GPU reports its own die channels (hotspot, VRAM) beside the
//!   host's CPU and board zones, so those readings get their own group card.

use bevy::app::{App, PostUpdate};
use bevy::color::Alpha;
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res};
use bevy::scene::{Scene, bsn};
use bevy::time::Time;
use bevy::ui::prelude::{BackgroundColor, Node, Overflow, percent, px};
use taskmanager_application::i18n::t;
use taskmanager_core::core::sensors::{SensorCenterSnapshot, SensorQuantity};
use taskmanager_shell::presentation::{missing_value, temperature_c_precise};

use super::{SystemFactGroup, SystemFactRow, fact_row_scene, section_card_scene};
use crate::palette::UiPalette;
use crate::window::WindowPalette;

/// The temperature above which an observed reading is a hot spot.
pub(crate) const HOT_SENSOR_THRESHOLD_C: f32 = 85.0;

/// The label tokens that name a GPU die sub-channel.
///
/// A discrete GPU reports its own die temperatures beside the host's CPU and
/// board zones. Those channels are grouped apart from the host thermal
/// zones; a channel the platform names otherwise stays with the host zones,
/// because the grouping must never reattribute a reading.
const GPU_DIE_LABEL_TOKENS: [&str; 4] = ["hotspot", "hot spot", "vram", "gpu memory"];

/// The hot spot's warning wash: the lowest alpha the breath reaches.
const HOT_WASH_MIN: f32 = 0.12;

/// The hot spot's warning wash: the highest alpha the breath reaches.
const HOT_WASH_MAX: f32 = 0.22;

/// The hot spot's breathing rate, in cycles per second.
const HOT_BREATH_HZ: f32 = 0.8;

/// The marker on the pinned hot-spot row.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HotSensorRow;

/// One projected thermal-zone row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThermalZoneRow {
    /// Source sensor label (e.g., "acpitz", "x86_pkg_temp", "coretemp").
    pub(crate) label: String,
    /// Display string: precise temperature (e.g. "54.5 °C") or shared dash.
    pub(crate) value: String,
    /// Whether an observed temperature value was present.
    pub(crate) present: bool,
    /// The observed temperature in °C, when the reading carried one.
    pub(crate) observed_c: Option<f32>,
}

/// One temperature group: the rows in presentation order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TemperatureGroup {
    /// The rows, in presentation order.
    pub(crate) rows: Vec<ThermalZoneRow>,
    /// Whether the first row is the pinned hot spot.
    pub(crate) pinned: bool,
}

/// The sensor-center temperature projection: the host zones and the GPU die
/// channels, each with its own hot spot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SensorTemperatureProjection {
    /// The host thermal zones.
    pub(crate) zones: TemperatureGroup,
    /// The GPU-exclusive die channels.
    pub(crate) gpu: TemperatureGroup,
}

/// Project thermal-zone temperature readings from the shared sensor snapshot.
///
/// Strictly mirrors GPUI (`sensor_rows(..., SensorGroup::Temperature, ...)`)
/// and Iced (`ui::health::projection::thermal_zone_rows`):
/// - Traverses every reading where `quantity() == SensorQuantity::Temperature`.
/// - Non-temperature channels (e.g., fan speed, power) are strictly excluded.
/// - Unreadable / missing temperature readings render the shared dash (`—`),
///   never a fabricated `0.0 °C`.
#[must_use]
pub(crate) fn thermal_zone_rows(sensors: &SensorCenterSnapshot) -> Vec<ThermalZoneRow> {
    sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::Temperature)
        .map(|reading| {
            let current = reading.current_number();
            ThermalZoneRow {
                label: reading.label().to_owned(),
                value: current
                    .map_or_else(missing_value, |value| temperature_c_precise(value as f32)),
                present: current.is_some(),
                observed_c: current.map(|value| value as f32),
            }
        })
        .collect()
}

/// Whether a temperature reading names a GPU-exclusive die channel.
fn is_gpu_die_channel(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    GPU_DIE_LABEL_TOKENS
        .iter()
        .any(|token| label.contains(token))
}

/// Pin the hot spot of a temperature list.
///
/// The hottest observed reading above the hot threshold moves to the front of
/// the list. A list without such a reading keeps its projection order and
/// pins nothing: the rule never invents a hot reading.
fn pin_hot_spot(rows: &mut Vec<ThermalZoneRow>) -> bool {
    let hottest = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.observed_c.map(|value| (index, value)))
        .filter(|(_, value)| *value > HOT_SENSOR_THRESHOLD_C)
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index);
    let Some(index) = hottest else {
        return false;
    };
    let row = rows.remove(index);
    rows.insert(0, row);
    true
}

/// Project the sensor snapshot into the host thermal zones and the GPU die
/// channels, each with its own pinned hot spot.
#[must_use]
pub(crate) fn sensor_temperature_projection(
    sensors: &SensorCenterSnapshot,
) -> SensorTemperatureProjection {
    let mut zones = Vec::new();
    let mut gpu = Vec::new();
    for row in thermal_zone_rows(sensors) {
        if is_gpu_die_channel(&row.label) {
            gpu.push(row);
        } else {
            zones.push(row);
        }
    }
    let zones_pinned = pin_hot_spot(&mut zones);
    let gpu_pinned = pin_hot_spot(&mut gpu);
    SensorTemperatureProjection {
        zones: TemperatureGroup {
            rows: zones,
            pinned: zones_pinned,
        },
        gpu: TemperatureGroup {
            rows: gpu,
            pinned: gpu_pinned,
        },
    }
}

/// The rows of one temperature group; the pinned hot spot wears the wash.
fn group_rows(group: &TemperatureGroup, palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    group
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let fact = SystemFactRow {
                group: SystemFactGroup::Hardware,
                label: row.label.clone(),
                value: row.value.clone(),
            };
            if group.pinned && index == 0 {
                hot_row_scene(&fact, palette)
            } else {
                Box::new(fact_row_scene(&fact, palette)) as Box<dyn Scene>
            }
        })
        .collect()
}

/// The pinned hot spot: the row wears the theme's danger wash.
fn hot_row_scene(fact: &SystemFactRow, palette: &UiPalette) -> Box<dyn Scene> {
    let wash = palette.danger_color.with_alpha(HOT_WASH_MIN);
    Box::new(bsn! {
        Node { width: percent(100), min_width: px(0.0), flex_shrink: 0.0, overflow: Overflow::clip() }
        BackgroundColor({ wash })
        HotSensorRow
        Children [ @{ fact_row_scene(fact, palette) } ]
    })
}

/// Construct a section card scene for thermal zones if any temperature readings are present.
pub(crate) fn thermal_zone_card_scene(
    group: &TemperatureGroup,
    width: Option<f32>,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    if group.rows.is_empty() {
        return None;
    }
    let rows = group_rows(group, palette);
    Some(Box::new(section_card_scene(
        t("common.thermal_zones").to_owned(),
        rows,
        width,
        palette,
    )) as Box<dyn Scene>)
}

/// Construct the GPU-exclusive temperature card.
///
/// The card is absent when the platform reported no GPU die channel: an
/// empty graphics block would read as a graphics board without telemetry.
pub(crate) fn gpu_temperature_card_scene(
    group: &TemperatureGroup,
    width: Option<f32>,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    if group.rows.is_empty() {
        return None;
    }
    let rows = group_rows(group, palette);
    let title = format!("{} · {}", t("common.gpu"), t("common.temperature"));
    Some(Box::new(section_card_scene(title, rows, width, palette)) as Box<dyn Scene>)
}

/// Breathe the hot spot's warning wash.
///
/// The wash rides the theme's danger ink; only its alpha moves, so the row
/// never claims a color the theme does not own.
fn breathe_hot_rows(
    time: Option<Res<Time>>,
    palette: Res<WindowPalette>,
    mut rows: Query<&mut BackgroundColor, With<HotSensorRow>>,
) {
    let phase = time.map_or(0.0, |time| time.elapsed_secs() * HOT_BREATH_HZ);
    let alpha = HOT_WASH_MIN + (HOT_WASH_MAX - HOT_WASH_MIN) * (0.5 + 0.5 * phase.sin());
    for mut fill in &mut rows {
        fill.0 = palette.inner.danger_color.with_alpha(alpha);
    }
}

/// Register the hot-spot presentation.
pub(crate) fn register(app: &mut App) {
    app.add_systems(PostUpdate, breathe_hot_rows);
}
