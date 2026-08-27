//! Performance-battery behavior tests: the Battery section projection
//! (capacity / status / rate / voltage) and the select-Battery selector
//! routing. Extracted from [`super::devices`] so neither sibling file holds
//! the whole device-section suite.

use super::super::perf_devices::battery::{
    battery_section_state, battery_summary_lines, battery_title,
};
use super::super::tables::ListState;
use super::super::*;

fn with_battery_scalars(
    mut battery: taskmanager_application::BatteryInfo,
    observed_at_ms: u64,
    capacity_pct: Option<u8>,
    voltage_uv: Option<u64>,
    power_w: Option<f32>,
    cycle_count: Option<u32>,
) -> taskmanager_application::BatteryInfo {
    use taskmanager_application::{BatteryScalarObservations, ScalarObservation};
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: capacity_pct.map_or_else(ScalarObservation::default, |value| {
            ScalarObservation::available(value, observed_at_ms)
        }),
        voltage_uv: voltage_uv.map_or_else(ScalarObservation::default, |value| {
            ScalarObservation::available(value, observed_at_ms)
        }),
        power_w: power_w.map_or_else(ScalarObservation::default, |value| {
            ScalarObservation::available(value, observed_at_ms)
        }),
        cycle_count: cycle_count.map_or_else(ScalarObservation::default, |value| {
            ScalarObservation::available(value, observed_at_ms)
        }),
        ..Default::default()
    });
    battery
}

#[test]
fn battery_section_state_distinguishes_loading_empty_and_ready() {
    use taskmanager_application::{BatteryInfo, DeviceState, PowerSupplySnapshot};

    // No power-supply snapshot observed yet → Loading (the collecting state).
    assert_eq!(battery_section_state(None), ListState::Loading);

    // A snapshot that reports no battery → Empty (honest "no battery" detected,
    // never a hidden zero).
    let empty = PowerSupplySnapshot {
        state: DeviceState::healthy(10),
        timestamp_ms: 10,
        batteries: Vec::new(),
        ..Default::default()
    };
    assert_eq!(battery_section_state(Some(&empty)), ListState::Empty);

    // A snapshot with one battery → Ready.
    let battery = with_battery_scalars(
        BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(10)),
        10,
        Some(82),
        None,
        None,
        None,
    );
    let populated = PowerSupplySnapshot {
        state: DeviceState::healthy(10),
        timestamp_ms: 10,
        batteries: vec![battery],
        ..Default::default()
    };
    assert_eq!(battery_section_state(Some(&populated)), ListState::Ready);
}

#[test]
fn battery_summary_lines_projects_real_readouts_and_keeps_unknown_capacity_honest() {
    use taskmanager_application::i18n::{Language, set_language};
    use taskmanager_application::{BatteryInfo, DeviceState};
    set_language(Language::En);

    let mut battery = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(100));
    battery.display_name = "BAT0".into();
    battery.model_name = "Li-ion Pack".into();
    battery.status = "Discharging".into();
    battery.technology = "Li-ion".into();
    battery.manufacturer = "TaskForest Cells".into();
    let battery = with_battery_scalars(
        battery,
        100,
        Some(82),
        Some(12_400_000),
        Some(8.4),
        Some(312),
    );
    let rows = battery_summary_lines(&battery);
    // Headline capacity stays a real percentage, not a fabricated zero.
    assert_eq!(rows[0].0, "Charge");
    assert_eq!(rows[0].1, "82%");
    // Status carries the charge/discharge direction.
    assert_eq!(rows[1].0, "Status");
    assert_eq!(rows[1].1, "Discharging");
    // Rate magnitude (watts) and voltage are projected in their readout units.
    assert_eq!(lookup(&rows, "Power"), "8.4 W");
    assert_eq!(lookup(&rows, "Voltage"), "12.40 V");
    assert_eq!(lookup(&rows, "Cycles"), "312");
    assert_eq!(lookup(&rows, "Technology"), "Li-ion");
    assert_eq!(lookup(&rows, "Manufacturer"), "TaskForest Cells");

    // The title prefers the model name over the raw display name.
    assert_eq!(battery_title(&battery, 0), "Battery: Li-ion Pack");

    // Unknown capacity MUST render an honest dash, NEVER 0%.
    let mut unmeasured = BatteryInfo::new("power-supply:BAT1", DeviceState::healthy(100));
    unmeasured.status = "Unknown".into();
    let unmeasured_rows = battery_summary_lines(&unmeasured);
    assert_eq!(unmeasured_rows[0].0, "Charge");
    assert_eq!(
        unmeasured_rows[0].1, "—",
        "unknown capacity is an honest dash"
    );
    assert!(
        !unmeasured_rows
            .iter()
            .any(|(_, value)| value == "0%" || value.contains("0.0 W")),
        "unobserved rate/voltage are omitted, not fabricated as zero"
    );
    // With no model or display name, the title falls back to the per-battery
    // index ("Battery 0") so two anonymous batteries stay distinguishable
    // (mirrors GPUI render_battery).
    let mut unnamed = BatteryInfo::default();
    unnamed.status = "Full".into();
    assert_eq!(battery_title(&unnamed, 0), "Battery 0");
    assert_eq!(battery_title(&unnamed, 2), "Battery 2");

    set_language(Language::En);
}

#[test]
fn battery_panel_renders_honest_states_and_routes_through_the_selector() {
    use taskmanager_application::{BatteryInfo, DeviceState, PowerSupplySnapshot};

    // The demo fixture carries no power-supply snapshot, so the default frontend
    // reaches the Loading state until the first refresh lands; selecting Battery
    // must render that honest state without panicking.
    let mut app = crate::IcedApp::demo();
    assert!(app.shell.projection().power_supplies.is_none());
    assert_eq!(
        battery_section_state(app.shell.projection().power_supplies.as_ref()),
        ListState::Loading
    );
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Battery(0)));
    assert_eq!(app.perf_device(), PerfDevice::Battery(0));
    // Render-and-drop: the Element borrows `app`, so release it before the next
    // mutation (`let _ =` drops the temporary immediately, unlike a named let).
    let _ = view(&app);

    // A snapshot that reported no battery routes to the honest empty line — the
    // canonical "no battery detected" message, not a fabricated panel.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            state: DeviceState::healthy(10),
            timestamp_ms: 10,
            batteries: Vec::new(),
            ..Default::default()
        })),
    );
    assert_eq!(
        battery_section_state(app.shell.projection().power_supplies.as_ref()),
        ListState::Empty
    );
    let _ = view(&app);

    // A populated snapshot renders the per-battery block; the full page composes.
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(PowerSupplySnapshot {
            state: DeviceState::healthy(20),
            timestamp_ms: 20,
            batteries: vec![with_battery_scalars(
                {
                    let mut battery =
                        BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(20));
                    battery.display_name = "BAT0".into();
                    battery.status = "Charging".into();
                    battery
                },
                20,
                Some(64),
                Some(12_600_000),
                Some(12.0),
                None,
            )],
            ..Default::default()
        })),
    );
    assert_eq!(
        battery_section_state(app.shell.projection().power_supplies.as_ref()),
        ListState::Ready
    );
    let _ = view(&app);
}

/// Look up the value projected under one label, failing loudly if the row is
/// absent (mirrors the table-test helper convention used by the disk/gpu tests).
fn lookup<'a>(rows: &'a [(String, String)], label: &str) -> &'a str {
    rows.iter()
        .find(|(key, _)| key == label)
        .map(|(_, value)| value.as_str())
        .expect("projected battery row must be present")
}
