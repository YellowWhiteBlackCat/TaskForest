use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::{
    DeviceGeneration, DeviceId, GpuEngine, GpuEngineRowsSnapshot, GpuMetrics,
    GpuScalarObservations, GpuThrottleReason, ScalarObservation, SystemSnapshot,
};

fn observed_gpu() -> GpuMetrics {
    let mut gpu = GpuMetrics::new("card0", "Intel");
    // The row and its seeded ring share one generation (the demo history
    // fixture ingests at generation 1; a generation-scoped read refuses an
    // unbound 0).
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(55.0, 1),
        temperature_c: ScalarObservation::available(63.0, 1),
        frequency_mhz: ScalarObservation::available(1_800, 1),
        max_frequency_mhz: ScalarObservation::available(2_100, 1),
        power_w: ScalarObservation::available(120.0, 1),
        idle_residency_pct: ScalarObservation::available(74.0, 1),
        dedicated_vram_used_bytes: ScalarObservation::available(1 << 30, 1),
        dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 1),
        shared_vram_used_bytes: ScalarObservation::available(512 << 20, 1),
        shared_vram_total_bytes: ScalarObservation::available(2 << 30, 1),
        ..Default::default()
    });
    gpu.apply_throttle_observation(ScalarObservation::available(
        vec![GpuThrottleReason::HardwareThermalLimit],
        1,
    ));
    gpu.driver = Some("xe".into());
    gpu.marketing_name = Some("Arc B390".into());
    gpu.engines = vec![GpuEngine {
        name: "Render/3D".into(),
        usage_pct: 42.0,
        ..GpuEngine::default()
    }];
    gpu
}

fn history_for(snapshot: &SystemSnapshot) -> taskmanager_shell::ShellApp {
    let mut shell = taskmanager_shell::ShellApp::new();
    taskmanager_shell::fixture::record_demo_history_frame(&mut shell, snapshot, None, None);
    taskmanager_shell::fixture::record_demo_history_frame(&mut shell, snapshot, None, None);
    shell
}

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn joined(lines: &[Line<'_>]) -> String {
    lines.iter().map(line_text).collect::<Vec<_>>().join("\n")
}

#[test]
fn full_fact_strip_keeps_every_current_gpu_scalar_together() {
    let text = joined(&gpu_fact_lines(&[observed_gpu()], GpuFactDensity::Full));
    for fact in [
        "55.0%",
        "63°C",
        "1800 MHz",
        "2100 MHz",
        "120.0 W",
        "74.0%",
        "1.0 GiB",
        "8.0 GiB",
        "512.0 MiB",
        "2.0 GiB",
        "xe",
        "thermal",
    ] {
        assert!(text.contains(fact), "GPU fact strip lost {fact:?}:\n{text}");
    }
}

#[test]
fn compact_fact_strip_keeps_primary_values_in_two_rows() {
    let lines = gpu_fact_lines(&[observed_gpu()], GpuFactDensity::Compact);
    assert_eq!(lines.len(), 2);
    let text = joined(&lines);
    for fact in ["55.0%", "63°C", "1800 MHz", "120.0 W", "74.0%"] {
        assert!(
            text.contains(fact),
            "compact fact strip lost primary value {fact:?}:\n{text}"
        );
    }
}

#[test]
fn standard_engine_projection_includes_live_and_pmu_rows() {
    let gpu = observed_gpu();
    let snapshot = SystemSnapshot {
        gpu: vec![gpu],
        ..SystemSnapshot::default()
    };
    let shell = history_for(&snapshot);
    let pmu = GpuEngineRowsSnapshot::success(
        DeviceId::new("card0"),
        vec![taskmanager_application::GpuEngineMetric {
            name: "Render Ring".into(),
            kind: taskmanager_application::GpuEngineKind::Unknown,
            utilization_pct: 43.0,
        }],
    );
    let lines = gpu_engine_lines(
        &snapshot.gpu,
        &shell.history,
        TuiTheme::default(),
        60,
        taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsPresentation::Active(
            &pmu.engines,
        ),
    );
    let text = joined(&lines);
    assert!(text.contains("Render/3D") && text.contains("42.0%"));
    assert!(text.contains("Render Ring") && text.contains("43.0%"));
}

#[test]
fn engine_without_history_is_an_honest_placeholder() {
    let gpu = observed_gpu();
    let lines = gpu_engine_lines(
        &[gpu],
        &LiveGraphHistory::default(),
        TuiTheme::default(),
        60,
        taskmanager_shell::presentation::gpu_engine_rows::GpuEngineRowsPresentation::PermissionRequired,
    );
    let engine = lines
        .iter()
        .map(line_text)
        .find(|line| line.contains("Render/3D"))
        .expect("engine row");
    assert!(engine.contains("42.0%"));
    assert!(engine.contains('·'));
    assert!(!engine.contains('█'));
}

#[test]
fn compact_layout_gives_all_non_fact_rows_to_the_primary_chart() {
    let area = Rect::new(0, 0, 54, 6);
    assert_eq!(
        GpuPanelLayout::resolve(area, 2, 7, 3),
        GpuPanelLayout::Compact {
            facts: Rect::new(0, 0, 54, 2),
            graph: Rect::new(0, 2, 54, 4),
        }
    );
}

#[test]
fn standard_layout_adds_engines_only_after_a_ten_row_chart() {
    let area = Rect::new(0, 0, 120, 26);
    assert_eq!(
        GpuPanelLayout::resolve(area, 2, 7, 2),
        GpuPanelLayout::Standard {
            facts: Rect::new(0, 0, 120, 7),
            graph: Rect::new(0, 7, 120, 15),
            engines: Some(Rect::new(0, 22, 120, 4)),
        }
    );
    let short = GpuPanelLayout::resolve(Rect::new(0, 0, 120, 19), 2, 7, 8);
    assert!(
        short.graph().height >= MIN_STANDARD_GRAPH_HEIGHT,
        "engine detail must never compress the primary chart: {short:?}"
    );
}

#[test]
fn utilization_chart_uses_the_real_per_device_history() {
    taskmanager_test_support::pin_english();
    let snapshot = SystemSnapshot {
        gpu: vec![observed_gpu()],
        ..SystemSnapshot::default()
    };
    let shell = history_for(&snapshot);
    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_gpu_metric_chart(
                frame,
                &snapshot.gpu,
                &shell.history,
                TuiTheme::default(),
                frame.area(),
                taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric::DEFAULT,
            );
        })
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(text.contains("GPU · Utilization"));
    assert!(text.contains("100%") && text.contains("50%") && text.contains("0%"));
    assert!(
        text.chars()
            .any(|cell| ['█', '▇', '▆', '▅', '▄', '▃', '▂', '▁'].contains(&cell)),
        "dominant chart must paint real utilization samples:\n{text}"
    );
}

/// The chart follows the shared selection: the same frame that flips the
/// family flips the title and the axis unit (ADR-034 stage 2 — “TUI 摘要
/// 单位与循环键同波更新”). The fixture GPU carries power readings, so the
/// power family paints a watts axis, not the fixed percent ladder.
#[test]
fn selected_metric_flips_title_and_axis_unit_in_the_same_frame() {
    taskmanager_test_support::pin_english();
    let snapshot = SystemSnapshot {
        gpu: vec![observed_gpu()],
        ..SystemSnapshot::default()
    };
    let shell = history_for(&snapshot);
    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_gpu_metric_chart(
                frame,
                &snapshot.gpu,
                &shell.history,
                TuiTheme::default(),
                frame.area(),
                taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric::Power,
            );
        })
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains("GPU · Power"),
        "the chart title must name the selected family:\n{text}"
    );
    assert!(
        text.contains(" W"),
        "the axis must summarize in the family's unit:\n{text}"
    );
    assert!(
        !text.contains("100%"),
        "a watts family must not keep the percent ladder:\n{text}"
    );
    assert!(
        text.chars()
            .any(|cell| ['█', '▇', '▆', '▅', '▄', '▃', '▂', '▁', '▀'].contains(&cell)),
        "the selected family must paint its real samples:\n{text}"
    );
}

/// A selected family with no trustworthy samples renders the explicit
/// collecting/dash placeholder — never a fabricated flat line at zero
/// (ADR-034: 不可用序列保持显式不可用投影).
#[test]
fn unavailable_selected_family_keeps_the_honest_dash_projection() {
    taskmanager_test_support::pin_english();
    let snapshot = SystemSnapshot {
        gpu: vec![observed_gpu()],
        ..SystemSnapshot::default()
    };
    let shell = history_for(&snapshot);
    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_gpu_metric_chart(
                frame,
                &snapshot.gpu,
                &shell.history,
                TuiTheme::default(),
                frame.area(),
                // The fixture observes every split VRAM pair but never the
                // overall memory pair — exactly one honest unavailable
                // family.
                taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric::Memory,
            );
        })
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        text.contains("GPU · Memory"),
        "the unavailable family stays named, not hidden:\n{text}"
    );
    assert!(text.contains(t("perf.collecting_samples")));
    assert!(
        !text
            .chars()
            .any(|cell| ['█', '▇', '▆', '▅', '▄', '▃', '▂', '▁', '▀'].contains(&cell)),
        "no fabricated samples may paint for an unobserved family:\n{text}"
    );
}

#[test]
fn gpu_line_viewport_clamps_navigation_and_reaches_the_tail() {
    assert_eq!(
        GpuLineViewport::resolve(17, usize::MAX, 4),
        GpuLineViewport {
            start: 13,
            end: 17,
            total: 17
        }
    );
}
