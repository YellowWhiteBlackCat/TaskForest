//! Performance page GPU detail block and series graph projection.

use super::*;
use iced::Element;
use taskmanager_application::{CapabilityId, GpuMetrics, SystemSnapshot};
use taskmanager_shell::presentation::gpu_engine_rows::{
    GpuEngineRowsAction, GpuEngineRowsPresentation, present_gpu_engine_rows,
};
use taskmanager_shell::presentation::{
    device_status_i18n_key, gpu_display_identity, missing_value,
};
use taskmanager_theme::tokens;

/// The Performance-page GPU panel readiness.
#[must_use]
pub(crate) fn gpu_section_state(snapshot: Option<&SystemSnapshot>) -> tables::ListState {
    match snapshot {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.gpu.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One GPU's display identity.
#[must_use]
pub(crate) fn gpu_title(gpu: &GpuMetrics) -> String {
    let name = gpu_display_identity(gpu)
        .headline
        .map(str::to_owned)
        .or_else(|| (!gpu.device_id.trim().is_empty()).then(|| gpu.device_id.trim().to_string()));
    match name {
        Some(name) => format!("{}: {name}", t("common.gpu")),
        None => t("common.gpu").to_string(),
    }
}

/// Project one GPU's honest scalar readouts as label/value rows for the Performance page.
#[must_use]
pub(crate) fn gpu_summary_lines(gpu: &GpuMetrics) -> Vec<(String, String)> {
    let observed = super::projection::GpuObservation::from(gpu);
    let mut rows = vec![
        (
            t("device.status").to_string(),
            t(device_status_i18n_key(gpu.device_state.status)).to_string(),
        ),
        (
            t("common.utilization").to_string(),
            gpu_percent_readout(observed.utilization_pct),
        ),
    ];
    if let Some(name) = gpu
        .marketing_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        rows.push((t("gpu.marketing_name").to_string(), name.to_owned()));
    }

    for (label, used, total) in [
        (
            t("gpu.dedicated_vram"),
            observed.dedicated_vram_used_bytes,
            observed.dedicated_vram_total_bytes,
        ),
        (
            t("gpu.shared_vram"),
            observed.shared_vram_used_bytes,
            observed.shared_vram_total_bytes,
        ),
    ] {
        if let Some(used) = used
            && let Some(total) = total
            && total > 0
        {
            rows.push((
                label.to_string(),
                format!("{} / {}", bytes(used), bytes(total)),
            ));
        }
    }
    if let Some(used) = observed.memory_used_bytes
        && let Some(total) = observed.memory_total_bytes
    {
        rows.push((
            t("gpu.vram").to_string(),
            format!("{} / {}", bytes(used), bytes(total)),
        ));
    }

    if let Some(mhz) = observed.frequency_mhz {
        rows.push((t("common.clock").to_string(), format!("{} MHz", mhz)));
    }
    if let Some(mhz) = observed.max_frequency_mhz {
        rows.push((t("gpu.max_clock").to_string(), format!("{} MHz", mhz)));
    }
    if let Some(value) = observed.idle_residency_pct {
        rows.push((
            t("gpu.idle_residency").to_string(),
            gpu_percent_readout(Some(value)),
        ));
    }
    if let Some(value) = observed.temperature_c {
        rows.push((
            t("common.temperature").to_string(),
            format!("{:.0} °C", value.round()),
        ));
    }
    if let Some(watts) = observed.power_w {
        rows.push((t("common.power").to_string(), format!("{:.1} W", watts)));
    }
    if let Some(driver) = gpu.driver.as_deref() {
        rows.push((t("common.driver").to_string(), driver.to_string()));
    }

    for engine in &gpu.engines {
        if !engine.name.trim().is_empty() && engine.usage_pct.is_finite() {
            rows.push((
                engine.name.clone(),
                gpu_percent_readout(Some(engine.usage_pct)),
            ));
        }
    }
    if let Some(reason) = observed.throttle_reason.filter(|reason| !reason.is_empty()) {
        rows.push((t("gpu.throttling").to_string(), reason));
    }
    rows
}

pub(crate) fn gpu_percent_readout(value: Option<f32>) -> String {
    value.map_or_else(missing_value, |value| format!("{:.0}%", value.round()))
}

pub(super) fn gpu_headline_label_value(
    metric: super::projection::GpuHeadlineMetric,
) -> (String, String) {
    use super::projection::{GpuHeadlineKind, GpuHeadlineValue};
    let missing = || missing_value();
    match metric.kind {
        GpuHeadlineKind::Utilization => (
            t("common.utilization").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::UtilizationPercent(value)) => {
                    gpu_percent_readout(Some(value))
                }
                _ => missing(),
            },
        ),
        GpuHeadlineKind::Temperature => (
            t("common.temperature").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::TemperatureC(value)) => {
                    format!("{:.0} °C", value.round())
                }
                _ => missing(),
            },
        ),
        GpuHeadlineKind::Frequency => (
            t("common.clock").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::FrequencyMhz(value)) => format!("{value} MHz"),
                _ => missing(),
            },
        ),
        GpuHeadlineKind::Power => (
            t("common.power").to_owned(),
            match metric.value {
                Some(GpuHeadlineValue::PowerW(value)) => format!("{value:.1} W"),
                _ => missing(),
            },
        ),
    }
}

fn gpu_headline_readouts(
    gpu: &GpuMetrics,
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    perf_layout::headline_readouts(
        theme_snapshot,
        super::projection::gpu_headline_metrics(gpu)
            .into_iter()
            .map(gpu_headline_label_value),
    )
}

/// The Performance-page GPU panel: one block per GPU in the shared snapshot.
pub(crate) fn gpu_section(
    app: &crate::IcedApp,
    index: usize,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    let theme_snapshot = app.theme();
    let color = theme::color(theme_snapshot.gpu);
    let rows = match (gpu_section_state(snapshot), snapshot) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("gpu.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot.gpu.get(index) {
            Some(gpu) => vec![gpu_block(GpuBlockProps {
                app,
                gpu,
                color,
                theme: theme_snapshot,
                compact: app.compact_layout(),
                engine_rows: engine_rows_presentation(app, gpu),
            })],
            None => vec![tables::message_panel(theme_snapshot, t("gpu.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    device_rows_panel(rows, theme_snapshot)
}

struct GpuBlockProps<'a> {
    app: &'a crate::IcedApp,
    gpu: &'a GpuMetrics,
    color: iced::Color,
    theme: &'a taskmanager_theme::Theme,
    compact: bool,
    engine_rows: GpuEngineRowsPresentation<'a>,
}

fn gpu_block<'a>(props: GpuBlockProps<'a>) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let GpuBlockProps {
        app,
        gpu,
        color,
        theme: theme_snapshot,
        compact,
        engine_rows,
    } = props;
    let smooth = true;
    let chart_layout = super::projection::GpuChartLayout::from_compact(compact);
    // The shared shell projection (ADR-034): one selection, one availability
    // gate, one explicit per-family state — this view renders exactly this
    // projection and holds no second copy.
    let metric_projection = app
        .shell
        .gpu_chart_metric_projection(&taskmanager_shell::gpu_chart_metric_gate(Some(gpu)));
    let selected = metric_projection.selected;
    let selector = gpu_chart_metric_selector(theme_snapshot, &metric_projection, compact);
    let mut graphs: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![
        selector,
        gpu_chart_metric_graph(app, gpu, selected, color, theme_snapshot, compact),
    ];
    if compact {
        graphs.insert(0, gpu_headline_readouts(gpu, theme_snapshot));
    }
    match chart_layout {
        super::projection::GpuChartLayout::AggregateWithEngines => {
            for engine in chart_layout.engine_charts(gpu) {
                let engine_samples = app.cached_gpu_engine_series(
                    &gpu.device_id,
                    gpu.device_generation.get(),
                    &engine.name,
                );
                graphs.push(device_chart::device_mini_graph_with_height(
                    engine_samples,
                    device_chart::DeviceMetricScale::Percent,
                    color,
                    engine.name.clone(),
                    theme_snapshot,
                    device_chart::ENGINE_DEVICE_CHART_HEIGHT,
                    device_chart::GraphPrefs {
                        smooth,
                        max_override: None,
                        hover: true,
                    },
                ));
            }
        }
        super::projection::GpuChartLayout::AggregateOnly => {}
    }
    if chart_layout.shows_secondary_regions()
        && let Some(vram_panel) = gpu_vram_meters_panel(gpu, theme_snapshot)
    {
        graphs.push(vram_panel);
    }
    let mut stats = gpu_summary_lines(gpu);
    match &engine_rows {
        GpuEngineRowsPresentation::Active(engines) => {
            if engines.is_empty() {
                stats.push((
                    t("gpu.per_engine_title").to_string(),
                    t("gpu.engines_none_reported").to_string(),
                ));
            }
            for engine in *engines {
                stats.push((
                    engine.name.clone(),
                    format!("{:.0}%", engine.utilization_pct.round()),
                ));
            }
        }
        presentation => {
            if let Some(message_key) = presentation.message_key()
                && !matches!(presentation, GpuEngineRowsPresentation::PermissionRequired)
            {
                stats.push((
                    t("gpu.per_engine_title").to_string(),
                    t(message_key).to_string(),
                ));
                if matches!(presentation, GpuEngineRowsPresentation::MissingDependency) {
                    stats.push((String::new(), t("gpu.engines_install_hint").to_string()));
                }
            }
        }
    }
    match engine_rows_toggle_section(theme_snapshot, engine_rows.action()) {
        Some(toggle) => iced::widget::column![
            perf_layout::main_with_stats(
                theme_snapshot,
                gpu_title(gpu),
                t(selected.label_key()).to_string(),
                graphs,
                stats,
                compact,
                perf_layout::DetailExtent::Fill,
            ),
            toggle
        ]
        .spacing(8)
        .into(),
        None => perf_layout::main_with_stats(
            theme_snapshot,
            gpu_title(gpu),
            t(selected.label_key()).to_string(),
            graphs,
            stats,
            compact,
            perf_layout::DetailExtent::Fill,
        ),
    }
}

/// The chart-metric choice pill row (ADR-034): one focusable
/// [`focus::choice_pill`] per available family and one inert muted pill per
/// unavailable family, both projected from the shared
/// [`GpuChartMetricChoiceState`]. Unavailable families stay visible —
/// never hidden, never zeroed.
fn gpu_chart_metric_selector<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    projection: &taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricProjection,
    compact: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricChoiceState;

    let pills: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = projection
        .choices
        .iter()
        .map(|choice| match choice.state {
            GpuChartMetricChoiceState::Selected | GpuChartMetricChoiceState::Selectable => {
                focus::choice_pill(
                    theme_snapshot,
                    crate::app::FocusTarget::GpuChartMetricTab(choice.metric),
                    t(choice.metric.label_key()).to_string(),
                    choice.state == GpuChartMetricChoiceState::Selected,
                    Message::SelectGpuChartMetric(choice.metric),
                )
            }
            // Selected-but-unavailable keeps its selected ink so the honest
            // degradation stays readable, but neither pointer nor keyboard
            // activation exists for an unavailable family.
            GpuChartMetricChoiceState::Unavailable
            | GpuChartMetricChoiceState::SelectedUnavailable => gpu_chart_metric_inert_pill(
                theme_snapshot,
                t(choice.metric.label_key()).to_string(),
                choice.state == GpuChartMetricChoiceState::SelectedUnavailable,
            ),
        })
        .collect();
    if compact {
        // Eight families do not fit one 720px row: split into complete
        // bounded rows so the trailing control is not a clipped fragment.
        crate::ui::chunked_rows(pills, 4)
    } else {
        iced::widget::row(pills).spacing(4).into()
    }
}

/// The explicitly-unavailable pill: the shared muted text in the same pill
/// geometry as its enabled siblings, with no interaction and no focus stop.
/// A selected-but-unavailable family keeps a bold weight so the honest
/// degradation stays readable.
fn gpu_chart_metric_inert_pill<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    label: String,
    selected: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let text = iced::widget::text(label)
        .size(f32::from(tokens::FONT_12))
        .style(move |_| iced::widget::text::Style { color: Some(muted) });
    let text = if selected {
        text.font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..iced::Font::DEFAULT
        })
    } else {
        text
    };
    container(text)
        .padding([f32::from(tokens::SPACE_4), f32::from(tokens::SPACE_10)])
        .into()
}

/// The headline chart for the selected family: the shared shell dispatch
/// over the device's live windows, with the scale of the family's unit —
/// an unavailable selected family yields its gaps (never a fabricated
/// zero line).
fn gpu_chart_metric_graph<'a>(
    app: &crate::IcedApp,
    gpu: &GpuMetrics,
    metric: taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric,
    color: iced::Color,
    theme_snapshot: &taskmanager_theme::Theme,
    compact: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricUnit;
    let scale = match metric.unit() {
        GpuChartMetricUnit::Percent => device_chart::DeviceMetricScale::Percent,
        GpuChartMetricUnit::Watts => device_chart::DeviceMetricScale::Watts,
        GpuChartMetricUnit::Celsius => device_chart::DeviceMetricScale::Celsius,
        GpuChartMetricUnit::Megahertz => device_chart::DeviceMetricScale::Megahertz,
    };
    device_chart::device_mini_graph_fill(
        app.cached_gpu_chart_metric_series(&gpu.device_id, gpu.device_generation.get(), metric),
        scale,
        color,
        t(metric.label_key()).to_string(),
        theme_snapshot,
        compact,
        device_chart::GraphPrefs {
            smooth: true,
            max_override: None,
            hover: true,
        },
    )
}

fn engine_rows_presentation<'a>(
    app: &'a crate::IcedApp,
    gpu: &'a GpuMetrics,
) -> GpuEngineRowsPresentation<'a> {
    present_gpu_engine_rows(
        app.shell.gpu_engine_rows_state(),
        &taskmanager_application::DeviceId::new(gpu.device_id.clone()),
        app.shell
            .projection()
            .capability_status(&CapabilityId::TELEMETRY_GPU_ENGINES),
    )
}

fn engine_rows_toggle_section<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    action: GpuEngineRowsAction,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    match action {
        GpuEngineRowsAction::Disable => Some(crate::focus::button(
            theme_snapshot,
            crate::app::FocusTarget::GpuEngineRowsToggle,
            t("common.disable"),
            Message::ToggleGpuEngines,
            true,
        )),
        GpuEngineRowsAction::Enable => Some(
            iced::widget::row![
                crate::focus::ghost_button(
                    theme_snapshot,
                    crate::app::FocusTarget::GpuEngineRowsToggle,
                    t("gpu.enable_per_engine"),
                    Message::ToggleGpuEngines,
                ),
                iced::widget::text(t("gpu.engines_requires_auth")).size(f32::from(tokens::FONT_11)),
            ]
            .spacing(8)
            .into(),
        ),
        GpuEngineRowsAction::Reauthorize | GpuEngineRowsAction::Recheck => {
            Some(crate::focus::ghost_button(
                theme_snapshot,
                crate::app::FocusTarget::GpuEngineRowsToggle,
                t(match action {
                    GpuEngineRowsAction::Reauthorize => "gpu.engines_reauthorize",
                    _ => "gpu.engines_recheck",
                }),
                Message::ToggleGpuEngines,
            ))
        }
        GpuEngineRowsAction::None => None,
    }
}

/// Dedicated visual progress bars and percentage meters for Dedicated VRAM and Shared VRAM.
///
/// Each meter's bar renders through the shared `progress` component: a
/// measured ratio arrives as `Some(clamped)` and paints the accent tone; an
/// unobserved pair never reaches a bar at all (the meter stays absent —
/// unavailable data must not masquerade as a measured 0%).
pub(crate) fn gpu_vram_meters_panel<'a>(
    gpu: &'a GpuMetrics,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let observed = super::projection::GpuObservation::from(gpu);
    let mut bars: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();

    for (label, used_opt, total_opt) in [
        (
            t("gpu.dedicated_vram"),
            observed.dedicated_vram_used_bytes,
            observed.dedicated_vram_total_bytes,
        ),
        (
            t("gpu.shared_vram"),
            observed.shared_vram_used_bytes,
            observed.shared_vram_total_bytes,
        ),
    ] {
        if let (Some(used), Some(total)) = (used_opt, total_opt)
            && total > 0
        {
            let pct = (used as f32 / total as f32).clamp(0.0, 1.0);
            let used_str = bytes(used);
            let total_str = bytes(total);

            let header = iced::widget::row![
                iced::widget::text(label).size(f32::from(tokens::FONT_12)),
                iced::widget::text(format!("{used_str} / {total_str} ({:.1}%)", pct * 100.0))
                    .size(f32::from(tokens::FONT_11))
                    .style(move |_| iced::widget::text::Style {
                        color: Some(theme::muted_text_color(theme_snapshot)),
                    }),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);

            let progress_bar =
                components::progress(theme_snapshot, Some(pct), components::BadgeTone::Accent);

            bars.push(
                iced::widget::column![header, progress_bar]
                    .spacing(4)
                    .into(),
            );
        }
    }

    if bars.is_empty() {
        None
    } else {
        Some(
            iced::widget::container(iced::widget::column(bars).spacing(6))
                .padding(8)
                .style(move |_| theme::panel_style(theme_snapshot))
                .into(),
        )
    }
}
