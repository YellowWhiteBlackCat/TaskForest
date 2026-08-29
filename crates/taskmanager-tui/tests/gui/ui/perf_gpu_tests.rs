use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::{
    GpuEngine, GpuEngineRowsSnapshot, GpuMetrics, GpuScalarObservations, GpuThrottleReason,
    ScalarObservation, SystemSnapshot,
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

// test-intent: behavior
/// The full fact strip names every proven graphics-API version and the PCI
/// slot (GPUI gpu_stats parity, §2.5 B-1..B-3). The core `GpuGraphicsApi`
/// contract says consumers must OMIT an unproven version row — never render
/// an inferred or dash placeholder — so a partially proven API keeps only its
/// proven row, and a GPU without any proven capability or slot renders
/// neither row.
#[test]
fn full_fact_strip_names_proven_graphics_apis_and_pci_slot() {
    taskmanager_test_support::pin_english();
    let mut proven = observed_gpu();
    proven.graphics_api = Some(taskmanager_core::core::metrics::GpuGraphicsApi {
        opengl_version: Some("4.6".into()),
        vulkan_version: Some("1.3.290".into()),
    });
    proven.pci_slot = Some("0000:03:00.0".into());
    let text = joined(&gpu_fact_lines(&[proven], GpuFactDensity::Full));
    for fact in [
        format!("{} 4.6", t("gpu.opengl_version")),
        format!("{} 1.3.290", t("gpu.vulkan_version")),
        format!("{} 0000:03:00.0", t("gpu.pci_slot")),
    ] {
        assert!(
            text.contains(&fact),
            "GPU fact strip lost {fact:?}:\n{text}"
        );
    }

    // Unavailable path: only the OpenGL context proved usable, so the Vulkan
    // row is omitted outright instead of rendering a dash placeholder.
    let mut partial = observed_gpu();
    partial.graphics_api = Some(taskmanager_core::core::metrics::GpuGraphicsApi {
        opengl_version: Some("4.6".into()),
        vulkan_version: None,
    });
    let partial_text = joined(&gpu_fact_lines(&[partial], GpuFactDensity::Full));
    assert!(
        partial_text.contains(t("gpu.opengl_version")),
        "the proven OpenGL version must still render:\n{partial_text}"
    );
    assert!(
        !partial_text.contains(t("gpu.vulkan_version")),
        "an unproven Vulkan version must omit its row, never render a dash:\n{partial_text}"
    );

    // A GPU with no proven API and no PCI slot renders neither row.
    let unproven_text = joined(&gpu_fact_lines(&[observed_gpu()], GpuFactDensity::Full));
    assert!(!unproven_text.contains(t("gpu.opengl_version")));
    assert!(!unproven_text.contains(t("gpu.vulkan_version")));
    assert!(!unproven_text.contains(t("gpu.pci_slot")));
}

// test-intent: behavior
/// The capability rows resolve their labels through the shared catalog in the
/// active locale: the same proven facts render the English copy under En and
/// the Chinese copy under Zh (one `t()` truth per locale, never hardcoded).
#[test]
fn gpu_capability_rows_render_the_active_locale_copy() {
    let mut gpu = observed_gpu();
    gpu.graphics_api = Some(taskmanager_core::core::metrics::GpuGraphicsApi {
        opengl_version: Some("4.6".into()),
        vulkan_version: Some("1.3.290".into()),
    });
    gpu.pci_slot = Some("0000:03:00.0".into());
    let keys = ["gpu.opengl_version", "gpu.vulkan_version", "gpu.pci_slot"];
    let guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let en_text = joined(&gpu_fact_lines(
        std::slice::from_ref(&gpu),
        GpuFactDensity::Full,
    ));
    let en_labels: Vec<&'static str> = keys.iter().map(|key| t(key)).collect();

    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::Zh);
    let zh_text = joined(&gpu_fact_lines(
        std::slice::from_ref(&gpu),
        GpuFactDensity::Full,
    ));
    let zh_labels: Vec<&'static str> = keys.iter().map(|key| t(key)).collect();
    drop(guard);

    for (key, en, zh) in keys
        .iter()
        .zip(en_labels.iter())
        .zip(zh_labels.iter())
        .map(|((key, en), zh)| (*key, *en, *zh))
    {
        assert_ne!(en, zh, "{key} must translate to distinct En/Zh copy");
        assert!(
            en_text.contains(en),
            "En strip must paint {en:?}:\n{en_text}"
        );
        assert!(
            zh_text.contains(zh),
            "Zh strip must paint {zh:?}:\n{zh_text}"
        );
    }
}

// test-intent: behavior
/// The standard-layout threshold stays exact: at its minimum height the whole
/// ten-row full fact strip fits above a chart that keeps
/// `MIN_STANDARD_GRAPH_HEIGHT`, so the parity rows can never be clipped away.
#[test]
fn standard_threshold_fits_every_full_fact_row_above_the_min_chart() {
    let area = Rect::new(0, 0, 120, STANDARD_LAYOUT_HEIGHT);
    let layout = GpuPanelLayout::resolve(area, 2, 10, 0);
    assert_eq!(
        layout.facts().height,
        10,
        "the fully-observed fact strip must fit whole: {layout:?}"
    );
    assert!(
        layout.graph().height >= MIN_STANDARD_GRAPH_HEIGHT,
        "the primary chart keeps its minimum: {layout:?}"
    );
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
        vec![taskmanager_core::core::metrics::GpuEngineMetric {
            name: "Render Ring".into(),
            kind: taskmanager_core::core::metrics::GpuEngineKind::Unknown,
            utilization_pct: 43.0,
        }],
    );
    let lines = gpu_engine_lines(
        &snapshot.gpu,
        &shell,
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
        &taskmanager_shell::ShellApp::new(),
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
                &shell,
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
                &shell,
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
                &shell,
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
