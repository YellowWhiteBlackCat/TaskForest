//! Rich Performance-page device rail — the iced counterpart of the gpui
//! devices sidebar, in iced's own card language. Each monitored device (CPU,
//! Memory, every disk, NIC, GPU, battery, fan channel) renders one live card:
//! a bounded identity heading, two caption lines of honest typed observations
//! (the same facts the gpui sidebar carries — usage/temperature, used/total,
//! active-time + throughput, send/recv + SSID/signal/link, VRAM + utilization,
//! charge %, RPM), and a mini sparkline of that device's OWN per-device
//! `LiveGraphHistory` window stroked in its category color. Selecting a card is
//! the same frontend-local `PerfDevice` selection the pill rail used; the
//! detail panel on the right is unchanged.
//!
//! Honesty rules match the detail panels: every scalar comes from the typed
//! `current_*` observation accessors, an unavailable observation renders "—"
//! (or is omitted for gated pieces like VRAM/clock), and a window with fewer
//! than two finite samples strokes no polyline — never a fabricated flat
//! line. Non-healthy devices append a localized status badge to the second
//! caption line (the shared `device_status_i18n_key` mapping), so a degraded
//! disk or permission-denied GPU reads differently from a healthy one at a
//! glance. The caption projections are pure and table-tested; the canvas
//! program reuses the `process_sparkline`/`trend_strip` cross-frame
//! fingerprint-cache pattern (clear only when the immutable snapshot
//! generation or auto-scale max actually moved).

use std::cell::RefCell;
use std::rc::Rc;

use iced::widget::canvas::{self, Cache, Geometry};
use iced::widget::{column, container, row, text};
use iced::{Color, Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::power::PowerSupplySnapshot;
use taskmanager_core::core::sensors::{SensorCenterSnapshot, SensorQuantity, SensorReading};

use taskmanager_shell::ShellApp;
use taskmanager_theme::{Theme, tokens};

use super::UnitPrefs;
use super::{VirtualWindow, virtual_table_body, virtual_table_row};
use crate::app::{FocusTarget, Message, PerfDevice};
use crate::focus;
use crate::perf_chart::{SeriesGeneration, line_path, series_point_runs_for};
use crate::theme;
use crate::trend_strip::finite_peak;
use taskmanager_ui_contract::IconId;

/// The semantic icon introducing one rail device family. Battery/Fan have no
/// registered glyph yet, so their rows render without the tile rather than
/// wearing an unrelated mark (contract gap tracked for new IconId assets).
fn rail_icon(device: &PerfDevice) -> Option<IconId> {
    match device {
        PerfDevice::Cpu => Some(IconId::Cpu),
        PerfDevice::Memory => Some(IconId::Memory),
        PerfDevice::Disk(_) => Some(IconId::Disk),
        PerfDevice::Network(_) => Some(IconId::Network),
        PerfDevice::Gpu(_) => Some(IconId::Gpu),
        PerfDevice::Battery(_) | PerfDevice::Fan(_) => None,
    }
}

mod captions;

pub(crate) use captions::{
    battery_rail_caption, battery_rail_heading, battery_rail_subtitle, cpu_rail_caption,
    cpu_rail_heading, cpu_rail_subtitle, disk_rail_caption, disk_rail_heading, disk_rail_subtitle,
    fan_rail_caption, fan_rail_heading, fan_rail_subtitle, gpu_rail_caption, gpu_rail_heading,
    gpu_rail_subtitle, mem_rail_caption, network_category_label, nic_rail_caption,
    nic_rail_heading, nic_rail_subtitle,
};

/// Sparkline canvas height inside one rail card. The spark rides the RIGHT
/// edge of the card (GPUI sidebar composition) at a fixed width.
const SPARK_HEIGHT: f32 = 30.0;
/// Fixed spark width inside one rail card (the GPUI sidebar's right-edge
/// spark reads at roughly this size).
const RAIL_SPARK_WIDTH: f32 = 40.0;
/// Measured width budget for one rail fact line: the sidebar width minus the
/// icon tile, spark, spacings and card padding, minus a small safety margin
/// for the measure-vs-paint rounding delta. Lines are ellipsized to this
/// measured budget instead of wrapping (GPUI sidebar truncation parity).
const FACTS_WIDTH_BUDGET: f32 = 124.0;
/// Stroke width of the rail sparkline (mirrors the trend strip).
const SPARK_STROKE_WIDTH: f32 = 1.4;
/// The percentage ceiling shared by every percentage-typed series.
const PERCENT_MAX: f32 = 100.0;
/// Fixed desktop rail-card extent. A fixed contract makes offscreen cards
/// representable by spacers instead of measuring every card.
pub(crate) const RAIL_CARD_HEIGHT: f32 = 104.0;
/// Fixed compact selector extent. It lets a horizontal viewport represent
/// offscreen device identities with leading/trailing spacers.
pub(crate) const COMPACT_DEVICE_ITEM_WIDTH: f32 = 156.0;

/// The semantic category one rail row belongs to — resolves the sparkline
/// stroke color from the theme's per-category graph accents (the same tokens
/// the gpui sidebar tints each device's sparkline with).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RailCategory {
    Cpu,
    Memory,
    Disk,
    Network,
    Gpu,
    Battery,
    Fan,
}

impl RailCategory {
    fn stroke(self, theme: &Theme) -> Color {
        let token = match self {
            Self::Cpu => theme.cpu,
            Self::Memory => theme.memory,
            Self::Disk => theme.disk,
            Self::Network => theme.network,
            Self::Gpu => theme.gpu,
            Self::Battery => theme.battery,
            Self::Fan => theme.fan,
        };
        taskmanager_theme::iced::color(token)
    }
}

/// One rail card's fully projected content. Built by the pure
/// [`rail_rows`] seam; the renderer adds theme + selection at draw time.
#[derive(Clone, Debug)]
pub(crate) struct RailRow {
    pub(crate) device: PerfDevice,
    pub(crate) heading: String,
    /// Precise hardware model subtitle (CPU brand, disk/NIC/GPU/battery
    /// model) rendered under the generic identity heading.
    pub(crate) subtitle: String,
    pub(crate) cap1: String,
    pub(crate) cap2: String,
    pub(crate) samples: Rc<[f32]>,
    /// The value mapping to the top of the sparkline frame (`100.0` for
    /// percentages; the finite peak for auto-scaled bytes/sec / RPM series).
    pub(crate) max: f32,
    pub(crate) category: RailCategory,
    /// The unit family the tooltip readout formats its samples in.
    pub(crate) value_format: RailValueFormat,
}

/// The unit family of a rail series — drives the hover tooltip's current /
/// average / peak spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RailValueFormat {
    Percent,
    BytesPerSec,
    Rpm,
}

/// Everything the pure [`rail_rows`] projection reads. Cheap references into
/// the shared shell state plus the resolved unit-preference pairs, so the
/// headless tests can drive the same seam the renderer hits at runtime.
pub(crate) struct RailInputs<'a> {
    pub(crate) snapshot: Option<&'a SystemSnapshot>,
    pub(crate) power: Option<&'a PowerSupplySnapshot>,
    pub(crate) sensors: Option<&'a SensorCenterSnapshot>,
    pub(crate) shell: &'a ShellApp,
    /// Optional renderer-cache snapshots aligned with the `devices` slice
    /// passed to [`rail_rows`]. Pure projection tests may omit them and read
    /// the bounded history directly.
    pub(crate) device_samples: Option<&'a [Option<Rc<[f32]>>]>,
    pub(crate) cpu_samples: Rc<[f32]>,
    pub(crate) memory_samples: Rc<[f32]>,
    pub(crate) memory_units: UnitPrefs,
    pub(crate) drive_units: UnitPrefs,
    pub(crate) network_units: UnitPrefs,
}

/// Project every visible device's rail card in tab order. A device whose data
/// disappeared mid-frame (snapshot without that index) is dropped — the
/// selector list and the rail are derived from the same snapshot in the same
/// render, so this only guards a torn refresh.
pub(crate) fn rail_rows(devices: &[PerfDevice], inputs: &RailInputs<'_>) -> Vec<RailRow> {
    let fans: Vec<&SensorReading> = inputs
        .sensors
        .map(|sensors| {
            sensors
                .readings
                .iter()
                .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
                .collect()
        })
        .unwrap_or_default();
    devices
        .iter()
        .enumerate()
        .filter_map(|(index, device)| {
            let cached = inputs
                .device_samples
                .and_then(|samples| samples.get(index))
                .and_then(Option::as_ref);
            rail_row(*device, inputs, &fans, cached)
        })
        .collect()
}

fn rail_row(
    device: PerfDevice,
    inputs: &RailInputs<'_>,
    fans: &[&SensorReading],
    cached_samples: Option<&Rc<[f32]>>,
) -> Option<RailRow> {
    let snapshot = inputs.snapshot?;
    Some(match device {
        PerfDevice::Cpu => {
            let (cap1, cap2) = cpu_rail_caption(&snapshot.cpu);
            RailRow {
                device,
                heading: cpu_rail_heading(&snapshot.cpu),
                subtitle: cpu_rail_subtitle(&snapshot.cpu),
                cap1,
                cap2,
                samples: Rc::clone(&inputs.cpu_samples),
                max: PERCENT_MAX,
                category: RailCategory::Cpu,
                value_format: RailValueFormat::Percent,
            }
        }
        PerfDevice::Memory => {
            let (cap1, cap2) = mem_rail_caption(&snapshot.memory, inputs.memory_units);
            RailRow {
                device,
                heading: t("common.memory").to_string(),
                subtitle: String::new(),
                cap1,
                cap2,
                samples: Rc::clone(&inputs.memory_samples),
                max: PERCENT_MAX,
                category: RailCategory::Memory,
                value_format: RailValueFormat::Percent,
            }
        }
        PerfDevice::Disk(index) => {
            let disk = snapshot.disks.get(index)?;
            let (cap1, cap2) = disk_rail_caption(disk, inputs.drive_units);
            let samples = rail_series(inputs, cached_samples, |shell| {
                shell
                    .history
                    .disk_bytes_per_sec_for(disk.device_id.as_str(), disk.device_generation.get())
            });
            let max = finite_peak(&samples);
            RailRow {
                device,
                heading: disk_rail_heading(disk),
                subtitle: disk_rail_subtitle(disk),
                cap1,
                cap2,
                samples,
                max,
                category: RailCategory::Disk,
                value_format: RailValueFormat::BytesPerSec,
            }
        }
        PerfDevice::Network(index) => {
            let nic = snapshot.networks.get(index)?;
            let (cap1, cap2) = nic_rail_caption(nic, inputs.network_units);
            let samples = rail_series(inputs, cached_samples, |shell| {
                shell
                    .history
                    .network_bytes_per_sec_for(&nic.device_id, nic.device_generation.get())
            });
            let max = finite_peak(&samples);
            RailRow {
                device,
                heading: nic_rail_heading(nic),
                subtitle: nic_rail_subtitle(nic),
                cap1,
                cap2,
                samples,
                max,
                category: RailCategory::Network,
                value_format: RailValueFormat::BytesPerSec,
            }
        }
        PerfDevice::Gpu(index) => {
            let gpu = snapshot.gpu.get(index)?;
            let (cap1, cap2) = gpu_rail_caption(gpu, inputs.memory_units);
            RailRow {
                device,
                heading: gpu_rail_heading(gpu, index),
                subtitle: gpu_rail_subtitle(gpu),
                cap1,
                cap2,
                samples: rail_series(inputs, cached_samples, |shell| {
                    shell
                        .history
                        .gpu_usage_pct_for(gpu.device_id.as_str(), gpu.device_generation.get())
                }),
                max: PERCENT_MAX,
                category: RailCategory::Gpu,
                value_format: RailValueFormat::Percent,
            }
        }
        PerfDevice::Battery(index) => {
            let battery = inputs.power?.batteries.get(index)?;
            let (cap1, cap2) = battery_rail_caption(battery);
            RailRow {
                device,
                heading: battery_rail_heading(battery, index),
                subtitle: battery_rail_subtitle(battery),
                cap1,
                cap2,
                samples: rail_series(inputs, cached_samples, |shell| {
                    shell.history.battery_capacity_pct_for(battery.id.as_str())
                }),
                max: PERCENT_MAX,
                category: RailCategory::Battery,
                value_format: RailValueFormat::Percent,
            }
        }
        PerfDevice::Fan(index) => {
            let fan = fans.get(index).copied()?;
            let (cap1, cap2) = fan_rail_caption(fan);
            let samples = rail_series(inputs, cached_samples, |shell| {
                shell.history.fan_rpm_for(fan.id())
            });
            let max = finite_peak(&samples);
            RailRow {
                device,
                heading: fan_rail_heading(index),
                subtitle: fan_rail_subtitle(fan),
                cap1,
                cap2,
                samples,
                max,
                category: RailCategory::Fan,
                value_format: RailValueFormat::Rpm,
            }
        }
    })
}

fn rail_series(
    inputs: &RailInputs<'_>,
    cached: Option<&Rc<[f32]>>,
    load: impl FnOnce(&ShellApp) -> Vec<f32>,
) -> Rc<[f32]> {
    if let Some(samples) = cached {
        return Rc::clone(samples);
    }
    Rc::from(load(inputs.shell).into_boxed_slice())
}

// --- Renderer -----------------------------------------------------------

/// Build the viewport-windowed rail card column for the wide Performance
/// layout. Compact windows use the horizontal pill strip in
/// `performance_sidebar`; this surface is the information-dense desktop rail.
pub(crate) fn device_cards<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a Theme,
    devices: &[PerfDevice],
    selected: PerfDevice,
    window: VirtualWindow,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let rows = app.performance_rail_rows(devices, window);
    let key = rail_widget_key(app, theme_snapshot, devices, selected, window);
    let table_theme = *theme_snapshot;
    iced::widget::lazy(key, move |_| {
        let rows = Rc::clone(&rows);
        virtual_table_body(window, Length::Fill, move |start, end| {
            rows.get(start.saturating_sub(window.start)..end.saturating_sub(window.start))
                .unwrap_or(&[])
                .iter()
                .map(|rail_row| {
                    let is_selected = rail_row.device == selected;
                    virtual_table_row(
                        device_card(rail_row.clone(), table_theme, is_selected),
                        RAIL_CARD_HEIGHT,
                    )
                })
                .collect()
        })
    })
    .into()
}

fn rail_widget_key(
    app: &crate::IcedApp,
    theme_snapshot: &Theme,
    devices: &[PerfDevice],
    selected: PerfDevice,
    window: VirtualWindow,
) -> u64 {
    super::lazy_key::LazyKey::new("perf-rail")
        .revision(app.shell.projection().system_revision)
        .theme(theme_snapshot)
        .geometry(window)
        .field(app.shell.history.revision())
        .field((app.memory_use_bytes(), app.memory_use_base2()))
        .field((app.drive_use_bytes(), app.drive_use_base2()))
        .field((app.network_use_bytes(), app.network_use_base2()))
        .field((selected.key(), selected.index()))
        .field(
            devices
                .iter()
                .map(|device| (device.key(), device.index()))
                .collect::<Vec<_>>(),
        )
        .finish()
}

/// Format one rail sample in its unit family for the hover tooltip.
#[must_use]
pub(crate) fn format_rail_value(value: f32, format: RailValueFormat) -> String {
    match format {
        RailValueFormat::Percent => format!("{:.0}%", value.round()),
        RailValueFormat::BytesPerSec => format!(
            "{}/s",
            super::quantity_text_pref(value.max(0.0) as u64, true, true)
        ),
        RailValueFormat::Rpm => format!("{:.0} RPM", value.round()),
    }
}

/// The hover tooltip's three readouts: current sample, window average, and
/// historical peak. Pure so the headless tests drive the same seam the card
/// renders; empty when the window has no finite sample.
#[must_use]
pub(crate) fn rail_tooltip_lines(row: &RailRow) -> Vec<String> {
    let finite: Vec<f32> = row
        .samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite.is_empty() {
        return Vec::new();
    }
    let current = finite[finite.len() - 1];
    let average = finite.iter().sum::<f32>() / finite.len() as f32;
    let peak = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    vec![
        format!(
            "{}: {}",
            t("common.current"),
            format_rail_value(current, row.value_format)
        ),
        format!(
            "{}: {}",
            t("common.average"),
            format_rail_value(average, row.value_format)
        ),
        format!(
            "{}: {}",
            t("common.peak"),
            format_rail_value(peak, row.value_format)
        ),
    ]
}

/// One rail card: a full-width focusable button (the same keyboard path the
/// pill rail used) whose content stacks the bounded heading, the two caption
/// lines, and the category-colored sparkline of the device's own history
/// window. Pointer + keyboard activation both resolve to the same frontend
/// local `SelectPerfDevice` message.
fn device_card(
    row: RailRow,
    theme_snapshot: Theme,
    selected: bool,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let tooltip_lines = rail_tooltip_lines(&row);
    let RailRow {
        device,
        heading,
        subtitle,
        cap1,
        cap2,
        samples,
        max,
        category,
        value_format: _,
    } = row;
    let muted = theme::muted_text_color(&theme_snapshot);
    // GPUI-sidebar composition (ICED-024 S1): an icon tile introduces the
    // device family on the left, the fact block owns the middle, and the
    // spark rides the right edge — instead of stacking everything vertically.
    let icon_tile = rail_icon(&device).map(|id| {
        let surface = theme_snapshot;
        container(crate::icons::icon(&surface, id, 16.0))
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0))
            .center_x(Length::Fixed(30.0))
            .center_y(Length::Fixed(30.0))
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(taskmanager_theme::iced::color(
                    surface.palette().surface,
                ))),
                border: iced::Border {
                    color: taskmanager_theme::iced::color(surface.palette().border),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..iced::widget::container::Style::default()
            })
    });
    let mut facts = column![
        text(crate::text_metrics::truncate_to_width(
            &heading,
            FACTS_WIDTH_BUDGET,
            f32::from(tokens::FONT_13),
        ))
        .size(f32::from(tokens::FONT_13))
        .font(taskmanager_theme::iced::ui_font_weight(
            &theme_snapshot,
            tokens::FONT_WEIGHT_HEADER,
        ))
    ]
    .spacing(2)
    .width(Length::Fill);
    if !subtitle.is_empty() {
        facts = facts.push(
            text(crate::text_metrics::truncate_to_width(
                &subtitle,
                FACTS_WIDTH_BUDGET,
                f32::from(tokens::FONT_11),
            ))
            .size(f32::from(tokens::FONT_11))
            .color(muted),
        );
    }
    if !cap1.is_empty() {
        facts = facts.push(
            text(crate::text_metrics::truncate_to_width(
                &cap1,
                FACTS_WIDTH_BUDGET,
                f32::from(tokens::FONT_11),
            ))
            .size(f32::from(tokens::FONT_11))
            .color(muted),
        );
    }
    if !cap2.is_empty() {
        facts = facts.push(
            text(crate::text_metrics::truncate_to_width(
                &cap2,
                FACTS_WIDTH_BUDGET,
                f32::from(tokens::FONT_11),
            ))
            .size(f32::from(tokens::FONT_11))
            .color(muted),
        );
    }
    let spark = canvas::Canvas::new(RailSpark {
        samples,
        color: category.stroke(&theme_snapshot),
        max,
    })
    .width(Length::Fixed(RAIL_SPARK_WIDTH))
    .height(Length::Fixed(SPARK_HEIGHT));
    let mut content = row![];
    if let Some(tile) = icon_tile {
        content = content.push(tile);
    }
    content = content.push(facts).push(spark);
    content = content
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center);
    let card = focus::device_rail_card_owned(
        theme_snapshot,
        FocusTarget::PerfDeviceTab(device),
        content.into(),
        selected,
        Message::SelectPerfDevice(device),
    );
    if tooltip_lines.is_empty() {
        return card;
    }
    // The tooltip wears the shared panel surface (the dialog/tooltip/card
    // family style) instead of the bare default popup — surface fill + border
    // token, so the readout reads over any chart it crosses.
    let tooltip_surface = theme_snapshot;
    let tooltip_content: Element<'static, Message, iced::Theme, iced::Renderer> = container(
        column(
            tooltip_lines
                .into_iter()
                .map(|line| {
                    text(line)
                        .size(f32::from(tokens::FONT_11))
                        .color(theme::muted_text_color(&theme_snapshot))
                        .into()
                })
                .collect::<Vec<Element<'static, Message, iced::Theme, iced::Renderer>>>(),
        )
        .spacing(2),
    )
    .padding([6, 8])
    .style(move |_| theme::panel_style(&tooltip_surface))
    .into();
    iced::widget::tooltip(
        card,
        tooltip_content,
        iced::widget::tooltip::Position::FollowCursor,
    )
    .into()
}

/// The rail sparkline: one polyline, no caption (the card's text column owns
/// the labels). A window with fewer than two finite samples strokes nothing.
struct RailSpark {
    samples: Rc<[f32]>,
    color: Color,
    max: f32,
}

/// The cached identity of the sparkline geometry: immutable snapshot
/// generation and auto-scale max (the single-series form of the trend
/// strip's fingerprint — see that module for the rationale).
#[derive(Clone, Default, PartialEq, Debug)]
struct RailSparkFingerprint {
    samples: SeriesGeneration,
    max_bits: u32,
}

impl RailSpark {
    fn fingerprint(&self) -> RailSparkFingerprint {
        RailSparkFingerprint {
            samples: SeriesGeneration::new(&self.samples),
            max_bits: self.max.to_bits(),
        }
    }
}

#[derive(Default)]
struct RailSparkState {
    cache: Cache,
    fingerprint: RefCell<RailSparkFingerprint>,
}

impl canvas::Program<Message> for RailSpark {
    type State = RailSparkState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let current = self.fingerprint();
        if *state.fingerprint.borrow() != current {
            *state.fingerprint.borrow_mut() = current;
            state.cache.clear();
        }
        // The cache closure is synchronous, so a cached rail repaint does not
        // need another copy of the device history window.
        let samples = &self.samples;
        let color = self.color;
        let max = self.max;
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            for points in series_point_runs_for(samples, frame.size(), max) {
                if let Some(path) = line_path(&points) {
                    frame.stroke(
                        &path,
                        canvas::Stroke::default()
                            .with_width(SPARK_STROKE_WIDTH)
                            .with_color(color),
                    );
                }
            }
        });
        vec![geometry]
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_rail/tests.rs"]
mod tests;
