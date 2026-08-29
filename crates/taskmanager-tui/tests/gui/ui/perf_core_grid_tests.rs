//! Per-core grid behavior: flat vs typed-topology grouping, the
//! utilization · frequency · temperature cell readout, the profile-aware mini
//! trend, and the narrow-frame/scroll contract over grouped content lines.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::hardware::CpuType;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, ScalarObservationGroup, SystemSnapshot,
};
use taskmanager_shell::fixture::{edit_snapshot, record_demo_history_frame};

use super::*;

/// Render ONLY the per-core grid into a `width × height` TestBackend, pinned
/// to English and serialized against the language-flipping i18n test.
fn grid_text(app: &TuiApp, theme: TuiTheme, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            super::render_core_grid(frame, app, theme, Rect::new(0, 0, width, height));
        })
        .expect("draw");
    terminal.backend().to_string()
}

/// Render the FULL frame for one Performance device, pinned to English.
fn perf_page_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::render(frame, app, TuiTheme::default()))
        .expect("draw");
    terminal.backend().to_string()
}

/// A minimal app whose projection carries a `core_count` per-core history and
/// no hardware inventory (the fail-closed flat starting point).
fn flat_app(usages: &[f32]) -> TuiApp {
    let mut app = TuiApp::new();
    let mut snapshot = SystemSnapshot {
        timestamp_ms: 1_000,
        ..SystemSnapshot::default()
    };
    snapshot.cpu = CpuMetrics::from_observations(CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::available(usages.to_vec(), 1_000),
        ..CpuScalarObservations::default()
    });
    edit_snapshot(&mut app.shell, |slot| {
        *slot = Some(snapshot);
    });
    let seeded = app
        .projection()
        .snapshot
        .clone()
        .expect("fixture snapshot exists");
    record_demo_history_frame(&mut app.shell, &seeded, None, None);
    app
}

#[test]
fn columns_fit_the_terminal_width() {
    assert_eq!(columns_for(120), 3);
    assert_eq!(columns_for(111), 3);
    assert_eq!(columns_for(110), 2);
    assert_eq!(columns_for(74), 2);
    assert_eq!(columns_for(36), 1);
    assert_eq!(columns_for(10), 1);
}

#[test]
fn no_samples_renders_an_honest_dash_never_zero() {
    let cell = core_cell(&[], None, 0, TuiGlyphMode::Unicode);
    assert!(cell.trend.is_empty());
    assert_eq!(cell.readout, "— · — · —");
    assert!(cell.utilization.is_none());
}

#[test]
fn the_cell_readout_is_utilization_frequency_temperature() {
    let mut cpu = CpuMetrics::from_observations(CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::available(vec![50.0], 1),
        per_core_frequency_group: ScalarObservationGroup::available(vec![3_280], 1),
        per_core_temperature_group: ScalarObservationGroup::available(vec![54.0], 1),
        ..CpuScalarObservations::default()
    });
    let complete = core_cell(&[50.0], Some(&cpu), 0, TuiGlyphMode::Unicode);
    assert_eq!(complete.readout, "50% · 3.28 GHz · 54 °C");

    // An observation the core does not report stays an honest dash — never a
    // fabricated 0 MHz / 0 °C stand-in.
    cpu.apply_scalar_observations(CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::available(vec![50.0], 1),
        ..CpuScalarObservations::default()
    });
    let usage_only = core_cell(&[50.0], Some(&cpu), 0, TuiGlyphMode::Unicode);
    assert_eq!(usage_only.readout, "50% · — · —");

    // A non-finite observation is as unavailable as a missing one.
    cpu.apply_scalar_observations(CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::available(vec![f32::NAN], 1),
        per_core_temperature_group: ScalarObservationGroup::available(vec![f32::NAN], 1),
        ..CpuScalarObservations::default()
    });
    let non_finite = core_cell(&[50.0], Some(&cpu), 0, TuiGlyphMode::Unicode);
    assert_eq!(non_finite.readout, "— · — · —");
}

#[test]
fn the_mini_trend_is_the_bounded_recent_window_in_the_selected_repertoire() {
    let samples: Vec<f32> = (0..24).map(|step| step as f32 * 4.0).collect();
    let cell = core_cell(&samples, None, 0, TuiGlyphMode::Unicode);
    // The trend renders the bounded recent window, one glyph per sample.
    assert_eq!(cell.trend.chars().count(), super::CELL_TREND_CHARS);
    assert!(
        cell.trend
            .chars()
            .all(|glyph| crate::ui::sparkline::SPARKLINE_BLOCKS.contains(&glyph)),
        "unicode trend must stay on the shared ramp: {:?}",
        cell.trend
    );

    let ascii = core_cell(&samples, None, 0, TuiGlyphMode::Ascii);
    assert_eq!(ascii.trend.chars().count(), super::CELL_TREND_CHARS);
    assert!(
        ascii
            .trend
            .chars()
            .all(|glyph| crate::ui::sparkline::SPARKLINE_ASCII_BLOCKS.contains(&glyph)),
        "ascii trend must degrade onto the shared ladder: {:?}",
        ascii.trend
    );
    assert!(
        ascii.trend.chars().any(|glyph| glyph != ' '),
        "a varying series must paint visible ascii ink: {:?}",
        ascii.trend
    );
}

/// A pinned core (≥85%) wears the danger color, a busy one (≥60%) the warn
/// color, an idle one the good color — so hotspots are scannable at a glance.
#[test]
fn tier_color_tracks_the_load_band() {
    let theme = TuiTheme::default();
    assert_eq!(tier_color(theme, None), theme.dim);
    assert_eq!(tier_color(theme, Some(5.0)), theme.good);
    assert_eq!(tier_color(theme, Some(60.0)), theme.warn);
    assert_eq!(tier_color(theme, Some(85.0)), theme.danger);
    assert_eq!(tier_color(theme, Some(99.0)), theme.danger);
    // Out-of-range values clamp into the nearest band before tinting.
    assert_eq!(tier_color(theme, Some(150.0)), theme.danger);
}

/// The demo fixture's per-core vectors are exactly as long as the topology it
/// declares (one fact, one authority): the 2026-08-29 inventory flagged the
/// old 4-value vector contradicting the declared 16-physical/22-logical host.
#[test]
fn demo_core_vectors_match_the_declared_topology() {
    let app = taskmanager_shell::demo_app();
    let snapshot = app.projection().snapshot.as_ref().expect("demo snapshot");
    let hardware = app.projection().hardware.as_ref().expect("demo hardware");
    let declared = snapshot.cpu.logical_cores.expect("logical cores");
    assert_eq!(declared, 22);
    assert_eq!(snapshot.cpu.current_core_usage_len(), declared);
    assert_eq!(snapshot.cpu.current_core_frequency_len(), declared);
    assert_eq!(snapshot.cpu.current_core_temperature_len(), declared);
    assert_eq!(hardware.cpu_types.len(), declared);
    assert_eq!(hardware.cpu_cores, Some(declared));
    let p_cores = hardware
        .cpu_types
        .iter()
        .filter(|core_type| **core_type == CpuType::Performance)
        .count();
    let e_cores = hardware
        .cpu_types
        .iter()
        .filter(|core_type| **core_type == CpuType::Efficient)
        .count();
    let lp_cores = hardware
        .cpu_types
        .iter()
        .filter(|core_type| **core_type == CpuType::LowPower)
        .count();
    assert_eq!((p_cores, e_cores, lp_cores), (12, 8, 2));
}

/// With `hardware.cpu_types` present the matrix groups under typed headers
/// that each carry their core count, in the fixed P → E → LP-E order.
#[test]
fn grouped_grid_headers_carry_the_typed_labels_and_counts() {
    let app = crate::demo_app();
    let text = grid_text(&app, TuiTheme::default(), 120, 40);
    for header in [
        "Performance cores 12",
        "Efficiency cores 8",
        "Low-power E-cores 2",
    ] {
        assert!(
            text.contains(header),
            "grouped grid lost header {header:?}:\n{text}"
        );
    }
    // The header counts cover the whole declared topology, grouped cells
    // start at the first core, and the flat title stays the single authority.
    assert!(text.contains("C00"), "first cell missing:\n{text}");
}

/// The same grouped topology reads localized headers under the Zh locale.
#[test]
fn grouped_grid_headers_localize() {
    let app = crate::demo_app();
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::Zh);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            super::render_core_grid(frame, &app, TuiTheme::default(), Rect::new(0, 0, 120, 40));
        })
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains(t("cpu.performance_cores")),
        "zh P-core header missing:\n{text}"
    );
}

/// No `cpu_types` inventory → fail-closed flat layout: no typed headers, the
/// historical one-cell-per-core rows, never a guessed grouping.
#[test]
fn missing_cpu_types_falls_back_to_the_flat_layout() {
    let app = flat_app(&[30.0, 60.0, 90.0]);
    let text = grid_text(&app, TuiTheme::default(), 120, 40);
    assert!(
        !text.contains("Performance cores"),
        "flat layout must not invent typed headers:\n{text}"
    );
    for core in ["C00", "C01", "C02"] {
        assert!(text.contains(core), "flat grid lost {core}:\n{text}");
    }
    assert!(
        text.contains("30% · — · —"),
        "each cell carries the three-segment readout with honest dashes:\n{text}"
    );
}

/// A history lane that outgrew a stale inventory classifies as `Unknown` and
/// still renders — under the plain "Cores" header, never dropped.
#[test]
fn lanes_past_a_stale_inventory_group_as_unknown() {
    let mut app = flat_app(&[30.0, 60.0, 90.0]);
    taskmanager_shell::fixture::edit_hardware(&mut app.shell, |hardware| {
        let info =
            hardware.get_or_insert_with(taskmanager_core::core::hardware::HardwareInfo::default);
        info.cpu_types = vec![CpuType::Performance];
    });
    let text = grid_text(&app, TuiTheme::default(), 120, 40);
    assert!(
        text.contains("Performance cores 1"),
        "the single classified core groups alone:\n{text}"
    );
    assert!(
        text.contains("Cores 2"),
        "the two unclassified cores group as Unknown with a count:\n{text}"
    );
}

/// The mini trend is painted in the frame under the Unicode repertoire and
/// degrades onto the ASCII ladder under an ASCII profile — same shape, no
/// Unicode blocks leaking into an ASCII terminal.
#[test]
fn the_cell_trend_paints_and_degrades_with_the_terminal_profile() {
    let app = crate::demo_app();
    let unicode = grid_text(&app, TuiTheme::default(), 120, 40);
    assert!(
        unicode
            .chars()
            .any(|glyph| crate::ui::sparkline::SPARKLINE_BLOCKS.contains(&glyph)),
        "unicode grid must paint shared-ramp trends:\n{unicode}"
    );

    let ascii_theme = TuiTheme {
        terminal: crate::TuiTerminalProfile {
            color: crate::TuiColorMode::TrueColor,
            glyphs: crate::TuiGlyphMode::Ascii,
        },
        ..TuiTheme::default()
    };
    let ascii = grid_text(&app, ascii_theme, 120, 40);
    assert!(
        !ascii
            .chars()
            .any(|glyph| crate::ui::sparkline::SPARKLINE_BLOCKS.contains(&glyph)),
        "ascii grid must not paint unicode ramp blocks:\n{ascii}"
    );
    assert!(
        ascii.chars().any(
            |glyph| crate::ui::sparkline::SPARKLINE_ASCII_BLOCKS.contains(&glyph) && glyph != ' '
        ),
        "ascii grid must paint visible ladder ink:\n{ascii}"
    );
}

/// Narrow frame: exactly one cell column, grouped content lines scroll as a
/// whole (headers move with their rows), and the last core stays reachable.
#[test]
fn narrow_grouped_grid_scrolls_to_the_last_core() {
    let mut app = crate::demo_app();
    let first = grid_text(&app, TuiTheme::default(), 37, 8);
    assert!(
        first.contains("C00"),
        "viewport must start at C00:\n{first}"
    );
    assert!(
        !first.contains("C21"),
        "a 22-core topology must exceed one narrow viewport:\n{first}"
    );

    app.scroll_cpu_cores(99);
    let tail = grid_text(&app, TuiTheme::default(), 37, 8);
    assert!(
        tail.contains("C21"),
        "scroll must expose the final core across group headers:\n{tail}"
    );
}

/// The seeded demo history feeds the CPU and Memory main charts: each frame
/// proves the real chart surface — the y-axis "50%" label only paints on the
/// rendered Chart, never on the cold-start placeholder path — so the honest
/// "Collecting samples…" state that a single sample forces cannot appear.
#[test]
fn seeded_demo_main_graphs_never_stay_on_the_collecting_placeholder() {
    for device in [crate::PerfDevice::Cpu, crate::PerfDevice::Memory] {
        let mut app = crate::demo_app();
        let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
            taskmanager_application::AppPage::Performance,
        ));
        app.select_perf_device(device);
        let text = perf_page_text(&app, 120, 40);
        assert!(
            !text.contains("Collecting samples"),
            "{device:?} main graph stayed on the collecting placeholder:\n{text}"
        );
        assert!(
            text.contains("50%"),
            "{device:?} main graph must render the seeded history chart:\n{text}"
        );
    }
}

/// The demo per-core cell reads its three segments from the fixture's typed
/// per-core facts: utilization · GHz · °C, tied to the C00 seed values.
#[test]
fn demo_core_cells_render_the_three_segment_readout() {
    let app = crate::demo_app();
    let text = grid_text(&app, TuiTheme::default(), 120, 40);
    assert!(
        text.contains("52% · 4.82 GHz · 58 °C"),
        "the first core cell must read utilization · frequency · temperature:\n{text}"
    );
}

/// The fixture is TOPOLOGY-DRIVEN, not pinned to one core count: seeding a
/// different hybrid shape (4P+12E, no SMT, no LP-E) yields a self-consistent
/// snapshot whose grid groups carry the right counts. A future host with any
/// other composition is one spec literal away — the renderer never hardcodes
/// a topology.
#[test]
fn an_alternative_topology_seeds_a_self_consistent_grouped_grid() {
    use taskmanager_application::{AppAction, AppPage};
    use taskmanager_core::core::metrics::{
        CpuMetrics, CpuScalarObservations, ScalarObservation, ScalarObservationGroup,
        SystemSnapshot,
    };
    use taskmanager_shell::fixture::{edit_hardware, edit_snapshot, record_demo_history_frame};

    let topology = taskmanager_shell::fixture::CpuTopologySpec {
        clusters: vec![
            taskmanager_shell::fixture::CpuClusterSpec {
                kind: CpuType::Performance,
                physical_cores: 4,
                threads_per_core: 1,
            },
            taskmanager_shell::fixture::CpuClusterSpec {
                kind: CpuType::Efficient,
                physical_cores: 12,
                threads_per_core: 1,
            },
        ],
    };
    let timestamp = 1_000_u64;
    let mut snapshot = SystemSnapshot {
        timestamp_ms: timestamp,
        ..SystemSnapshot::default()
    };
    snapshot.cpu = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(37.4, timestamp),
        core_usage_group: ScalarObservationGroup::available(topology.core_usage(), timestamp),
        per_core_frequency_group: ScalarObservationGroup::available(
            topology.frequencies_mhz(),
            timestamp,
        ),
        per_core_temperature_group: ScalarObservationGroup::available(
            topology.temperatures_c(),
            timestamp,
        ),
        frequency_mhz: ScalarObservation::available(3_284, timestamp),
        temperature_c: ScalarObservation::available(54.0, timestamp),
        ..CpuScalarObservations::default()
    });
    snapshot.cpu.logical_cores = Some(topology.logical_cores());
    snapshot.cpu.physical_cores = Some(topology.physical_cores());

    // A fresh app (not `demo_app`): the demo seeds its own 22-core topology,
    // and this test proves the ELASTIC path with a different shape.
    let mut app = TuiApp::new();
    edit_snapshot(&mut app.shell, |slot| *slot = Some(snapshot.clone()));
    let seeded = app
        .projection()
        .snapshot
        .clone()
        .expect("fixture snapshot exists");
    // Two ingested frames clear the honest cold-start gate and give every
    // per-core lane its trend.
    record_demo_history_frame(&mut app.shell, &seeded, None, None);
    record_demo_history_frame(&mut app.shell, &seeded, None, None);
    edit_hardware(&mut app.shell, |hardware| {
        // A fresh app owns no fixture hardware: seed the topology's own
        // hardware facts (the grid groups fail closed without `cpu_types`).
        *hardware = Some(taskmanager_core::core::hardware::HardwareInfo {
            cpu_types: topology.cpu_types(),
            cpu_cores: Some(topology.logical_cores()),
            ..taskmanager_core::core::hardware::HardwareInfo::default()
        });
    });
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));

    let text = grid_text(&app, TuiTheme::default(), 120, 40);
    assert!(
        text.contains("Performance cores 4"),
        "4P topology must paint the P-group header with count 4:\n{text}"
    );
    assert!(
        text.contains("Efficiency cores 12"),
        "12E topology must paint the E-group header with count 12:\n{text}"
    );
    assert!(
        !text.contains("Low-power E-cores"),
        "a topology without LP-E cores must not paint an LP-E group:\n{text}"
    );
    assert!(
        text.contains("52% · 4.82 GHz · 58 °C"),
        "the first P-thread cell must read its topology-derived readout:\n{text}"
    );
    assert!(
        text.contains("C15") && !text.contains("C16"),
        "16 logical CPUs must paint exactly C00–C15:\n{text}"
    );
}
