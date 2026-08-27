//! CPU all-facts composition and short-terminal core viewport behavior.

use taskmanager_application::{
    AppAction, AppPage, CpuTemperatureSource, ScalarObservation, ScalarObservationGroup,
};

use super::frame_text;

fn cpu_app() -> crate::TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.select_perf_device(crate::PerfDevice::Cpu);
    app
}

#[test]
fn cpu_page_shows_every_headline_fact_and_per_core_without_a_selector() {
    let app = cpu_app();
    let text = frame_text(&app, 120, 40);
    for expected in [
        "Utilization 37.4%",
        "Temperature 54°C",
        "Frequency 3284 MHz",
        "Power —",
        "CPU Utilization (%)",
        "C00",
        "C03",
    ] {
        assert!(
            text.contains(expected),
            "CPU all-facts frame lost {expected:?}:\n{text}"
        );
    }

    let compact = frame_text(&app, 54, 16);
    for expected in [
        "Utilization",
        "Temperature",
        "Frequency",
        "Power",
        "CPU Utilization (%)",
    ] {
        assert!(
            compact.contains(expected),
            "short CPU frame must prioritize facts and the main graph; lost {expected:?}:\n{compact}"
        );
    }
    assert!(
        !compact.contains("C00"),
        "short CPU frame must hide optional per-core detail instead of sacrificing the main graph:\n{compact}"
    );
}

#[test]
fn available_power_joins_the_fact_strip_without_displacing_the_main_graph() {
    let mut app = cpu_app();
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.power_w = ScalarObservation::available(12.5, 1_000_000);
        snapshot.cpu.apply_scalar_observations(observations);
    });
    let snapshot = app
        .projection()
        .snapshot
        .clone()
        .expect("edited demo snapshot");
    for _ in 0..3 {
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
    }

    let text = frame_text(&app, 120, 48);
    assert!(text.contains("Power 12.5 W"), "power fact missing:\n{text}");
    assert!(
        text.contains("CPU Utilization (%)"),
        "the utilization history must remain the single dominant graph:\n{text}"
    );
}

#[test]
fn tall_terminal_core_viewport_reaches_the_tail_of_a_dense_topology() {
    let mut app = cpu_app();
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.core_usage_group = ScalarObservationGroup::available(
            (0..60).map(|index| index as f32 * 2.0).collect(),
            1_000_000,
        );
        snapshot.cpu.apply_scalar_observations(observations);
    });
    let snapshot = app
        .projection()
        .snapshot
        .clone()
        .expect("edited demo snapshot");
    taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &snapshot, None, None);

    let first = frame_text(&app, 120, 36);
    assert!(
        first.contains("C00"),
        "viewport must start at the first core"
    );
    assert!(
        !first.contains("C59"),
        "dense topology should require a bounded per-core viewport"
    );

    app.scroll_cpu_cores(99);
    let tail = frame_text(&app, 120, 36);
    assert!(
        tail.contains("C59"),
        "scroll must expose the final core:\n{tail}"
    );
}

/// The Temperature fact carries the typed provenance note: a labeled
/// fallback tier (a CPU-package-labeled channel on another hwmon chip, or
/// an ACPI thermal zone) is qualified so it never masquerades as a
/// dedicated CPU sensor chip, while native chips keep the plain readout —
/// the same fold the iced/gpui CPU readouts apply.
#[test]
fn temperature_fact_annotates_labeled_fallback_sources_like_the_gui_frontends() {
    let mut app = cpu_app();

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.cpu.temperature_source = CpuTemperatureSource::PackageHwmon;
    });
    let fallback = frame_text(&app, 120, 40);
    assert!(
        fallback.contains("Temperature 54°C · hwmon fallback"),
        "a package-labeled hwmon fallback must be qualified:\n{fallback}"
    );

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.cpu.temperature_source = CpuTemperatureSource::ThermalZone;
    });
    let zone = frame_text(&app, 120, 40);
    assert!(
        zone.contains("Temperature 54°C · ACPI thermal zone"),
        "an ACPI thermal zone fallback must be qualified:\n{zone}"
    );

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.cpu.temperature_source = CpuTemperatureSource::Coretemp;
    });
    let native = frame_text(&app, 120, 40);
    assert!(
        native.contains("Temperature 54°C"),
        "a native chip keeps the plain readout:\n{native}"
    );
    assert!(
        !native.contains("hwmon fallback") && !native.contains("ACPI thermal zone"),
        "a native dedicated chip must not carry a fallback qualifier:\n{native}"
    );
}
