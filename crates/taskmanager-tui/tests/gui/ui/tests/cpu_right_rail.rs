//! CPU Performance right-rail parity (gap inventory §2.1 #1, #3-21): the
//! page-title block, the pinned details column (live counters + the full
//! spec sheet), the BogoMIPS frequency qualifier and the per-core
//! temperature footnote — plus honest whole-column degradation on narrow
//! frames. Value expectations are the fixture projections the folds are
//! fed below, asserted row-by-row like a reader would scan the column.

use taskmanager_application::i18n::Language;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::hardware::CoreBreakdown;
use taskmanager_core::core::metrics::{
    CpuFrequencySource, ScalarObservation, ScalarObservationGroup,
};

use super::acceptance_support::frame_in_language;
use super::frame_text;

fn cpu_app() -> crate::TuiApp {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.select_perf_device(crate::PerfDevice::Cpu);
    app
}

/// True when one painted line carries both the label and the value — a
/// same-line pairing, so a value that drifted to another widget cannot
/// satisfy a spec-row assertion.
fn row_paints(text: &str, label: &str, value: &str) -> bool {
    text.lines()
        .any(|line| line.contains(label) && line.contains(value))
}

/// Feed the rail's full spec sheet through the fixture boundary: every
/// asserted projection is pinned here, so the assertions below hold against
/// the values the folds are actually fed (never fabricated by the renderer
/// — only painted because the projection reports it), independent of any
/// concurrent fixture enrichment.
fn with_full_spec(app: &mut crate::TuiApp) {
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.processes = 347;
        snapshot.threads = Some(2_816);
        snapshot.uptime_secs = 6 * 3600 + 42 * 60;
        snapshot.cpu.brand = Some("Intel(R) Core(TM) Ultra 7 358H".into());
        snapshot.cpu.physical_cores = Some(16);
        snapshot.cpu.logical_cores = Some(22);
        snapshot.cpu.l1_cache_kb = Some(1280);
        snapshot.cpu.l2_cache_kb = Some(20_480);
        snapshot.cpu.l3_cache_kb = Some(24_576);
        snapshot.cpu.performance_policy.frequency_implementation = Some("intel_pstate".into());
        snapshot.cpu.performance_policy.active_policy = Some("powersave".into());
        snapshot.cpu.performance_policy.energy_preference = Some("balance_performance".into());
    });
    taskmanager_shell::fixture::edit_hardware(&mut app.shell, |hardware| {
        let hardware = hardware.as_mut().expect("demo hardware");
        hardware.base_freq_mhz = Some(3200);
        hardware.sockets = Some(2);
        hardware.virt = Some("KVM".into());
        hardware.core_breakdown = CoreBreakdown {
            p_cores: 4,
            e_cores: 8,
            lp_cores: 2,
        };
    });
}

#[test]
fn right_rail_paints_the_full_live_and_spec_row_set_at_reference_size() {
    let mut app = cpu_app();
    with_full_spec(&mut app);
    let text = frame_text(&app, 120, 48);

    // Title block: the "CPU" heading plus the fixture brand subtitle. The
    // TestBackend serializes every row wrapped in literal quotes, so the
    // standalone-title check strips the quoting before comparing.
    assert!(
        text.lines()
            .any(|line| line.trim_matches(|c: char| c == '"' || c == ' ') == "CPU"),
        "the CPU page title row is missing:\n{text}"
    );
    assert!(
        text.contains("Intel(R) Core(TM) Ultra 7 358H"),
        "the brand subtitle is missing from the title block:\n{text}"
    );

    // Live rows (fixture projection: 347 processes, 2816 threads, 6h42m).
    for (label, value) in [
        ("Processes", "347"),
        ("Threads", "2816"),
        ("Uptime", "06h 42m"),
    ] {
        assert!(
            row_paints(&text, label, value),
            "live rail row {label:?} lost its value {value:?}:\n{text}"
        );
    }

    // Spec sheet, gpui cpu_spec_rows order and value semantics: base clock
    // in GHz, honest counts, hypervisor label, caches as MiB.
    for (label, value) in [
        ("Base speed", "3.20 GHz"),
        ("Sockets", "2"),
        ("Cores", "16"),
        ("Performance cores", "4"),
        ("Efficiency cores", "8"),
        ("Low-power E-cores", "2"),
        ("Logical processors", "22"),
        ("Virtualization", "KVM"),
        ("L1 cache", "1.25 MiB"),
        ("L2 cache", "20.00 MiB"),
        ("L3 cache", "24.00 MiB"),
        ("Cpufreq driver", "intel_pstate"),
        ("Cpufreq governor", "powersave"),
    ] {
        assert!(
            row_paints(&text, label, value),
            "spec rail row {label:?} lost its value {value:?}:\n{text}"
        );
    }
    // The one value longer than the rail's 28-cell inner width wraps to the
    // next line (the System page's Wrap behavior) instead of being clipped:
    // label and value must both survive, adjacency is width-dependent.
    assert!(
        text.contains("Power preference") && text.contains("balance_performance"),
        "power-preference row lost its label or value:\n{text}"
    );

    // The rail is an addition: the dominant utilization history stays.
    assert!(
        text.contains("CPU Utilization (%)"),
        "the rail must not displace the main graph:\n{text}"
    );
}

#[test]
fn right_rail_degrades_by_omitting_the_whole_column_on_narrow_frames() {
    let mut app = cpu_app();
    with_full_spec(&mut app);

    // The 54x16 minimum frame keeps its historical contract: facts + main
    // graph, no rail column, no panic.
    let compact = frame_text(&app, 54, 16);
    for fact in ["Utilization", "Temperature", "Frequency", "Power"] {
        assert!(
            compact.contains(fact),
            "the minimum frame must keep the headline facts; lost {fact:?}:\n{compact}"
        );
    }
    assert!(
        compact.contains("CPU Utilization (%)"),
        "the minimum frame must keep the main graph:\n{compact}"
    );
    for rail_text in ["Base speed", "Logical processors", "Sockets", "06h 42m"] {
        assert!(
            !compact.contains(rail_text),
            "the rail column must be omitted entirely below the width threshold; \
             found {rail_text:?}:\n{compact}"
        );
    }

    // A narrow-but-taller frame stays below the rail's width threshold and
    // hides the whole column too; a wide frame shows it again.
    let narrow = frame_text(&app, 60, 30);
    assert!(
        !narrow.contains("Base speed"),
        "60 columns cannot afford rail + chart; the column must be omitted:\n{narrow}"
    );
    let wide = frame_text(&app, 100, 40);
    assert!(
        row_paints(&wide, "Base speed", "3.20 GHz"),
        "a wide frame must show the spec rail:\n{wide}"
    );
}

#[test]
fn right_rail_rows_translate_across_locales() {
    let mut app = cpu_app();
    with_full_spec(&mut app);

    let en = frame_in_language(&app, 120, 48, Language::En);
    assert!(
        row_paints(&en, "Base speed", "3.20 GHz")
            && row_paints(&en, "Logical processors", "22")
            && row_paints(&en, "Processes", "347")
            && row_paints(&en, "Uptime", "06h 42m")
            && en.contains("Details"),
        "English rail rows drifted:\n{en}"
    );

    let zh = frame_in_language(&app, 120, 48, Language::Zh);
    assert!(
        row_paints(&zh, "基础速度", "3.20 GHz")
            && row_paints(&zh, "插槽", "2")
            && row_paints(&zh, "核心", "16")
            && row_paints(&zh, "逻辑处理器", "22")
            && row_paints(&zh, "进程", "347")
            && row_paints(&zh, "运行时间", "06h 42m")
            && row_paints(&zh, "性能核（P 核）", "4")
            && zh.contains("详情"),
        "Chinese rail rows drifted:\n{zh}"
    );
    assert!(
        zh.lines()
            .any(|line| line.trim_matches(|c: char| c == '"' || c == ' ') == "CPU"),
        "the CPU page title must stay painted under Zh:\n{zh}"
    );
}

/// Speed-row parity (#3): a BogoMIPS fallback frequency carries the typed
/// source qualifier so a boot-calibration value never masquerades as a
/// native clock measurement; a native readout stays unqualified.
#[test]
fn bogomips_fallback_qualifies_the_frequency_fact() {
    let mut app = cpu_app();
    set_frequency(&mut app, 3284);
    let native = frame_text(&app, 120, 40);
    assert!(
        native.contains("Frequency 3284 MHz"),
        "the native frequency readout is missing:\n{native}"
    );
    assert!(
        !native.contains("BogoMIPS"),
        "a native frequency must not carry the BogoMIPS qualifier:\n{native}"
    );

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        snapshot.cpu.frequency_source = CpuFrequencySource::BogoMips;
    });
    let fallback = frame_text(&app, 120, 40);
    assert!(
        fallback.contains("Frequency 3284 MHz · BogoMIPS fallback"),
        "a BogoMIPS fallback must be qualified:\n{fallback}"
    );
}

/// Temperature-row parity (#4): with two or more real per-core channels the
/// package reading gains the average/maximum footnote (collapsing to a
/// single "Cores" readout when every core sits within one degree); a
/// single-channel observation keeps the package line alone.
#[test]
fn per_core_temperature_footnote_reports_average_and_peak() {
    let mut app = cpu_app();
    set_per_core_temperatures(&mut app, vec![44.0, 48.0, 46.0, 45.0]);
    let spread = frame_text(&app, 120, 40);
    assert!(
        spread.contains("Avg 46 · Max 48 °C"),
        "four reporting cores must surface the avg/max footnote:\n{spread}"
    );

    set_per_core_temperatures(&mut app, vec![45.0, 45.4, 45.8, 45.2]);
    let tight = frame_text(&app, 120, 40);
    assert!(
        tight.contains("Cores 46 °C"),
        "cores within one degree must collapse to a single readout:\n{tight}"
    );
    assert!(
        !tight.contains("· Max"),
        "the collapsed footnote must not carry the avg/max split:\n{tight}"
    );

    set_per_core_temperatures(&mut app, vec![44.0]);
    let single = frame_text(&app, 120, 40);
    assert!(
        !single.contains("· Max") && !single.contains("Cores 44 °C"),
        "a single reporting channel must keep the package line alone:\n{single}"
    );
}

fn set_per_core_temperatures(app: &mut crate::TuiApp, temperatures: Vec<f32>) {
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.per_core_temperature_group =
            ScalarObservationGroup::available(temperatures, 1_000_000);
        snapshot.cpu.apply_scalar_observations(observations);
    });
}

/// Pin the live frequency readout the BogoMIPS assertions scan for.
fn set_frequency(app: &mut crate::TuiApp, mhz: u64) {
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        let mut observations = snapshot.cpu.scalar_observations().clone();
        observations.frequency_mhz = ScalarObservation::available(mhz, 1_000_000);
        snapshot.cpu.apply_scalar_observations(observations);
    });
}
