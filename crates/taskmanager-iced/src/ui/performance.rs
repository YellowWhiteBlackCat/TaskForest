//! Iced Performance page composition and frontend-local device navigation.
//!
//! This module owns the selector → detail mapping, responsive rail geometry,
//! bounded device windows, and unit preferences. It does not collect provider
//! data; every label and panel reads the current Iced projection.

use iced::Element;
use iced::widget::{column, container, row, scrollable, text};
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens;

use super::responsive::{COMPACT_TOOLBAR_FIVE_COLUMN_MIN_WIDTH, DeviceNavigationPresentation};
use super::{
    VirtualWindow, battery_section, cpu_memory_detail, disk_section, fan, gpu_section,
    network_section, perf_layout, perf_rail, virtual_horizontal_body,
};
use crate::app::{FocusTarget, Message, PerfDevice};
use crate::{focus, theme};

/// Compact detail viewport ownership. CPU and GPU use elastic, non-scrolling
/// aggregate surfaces; device pages with variable-length inventories keep the
/// existing scroll boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompactDetailViewport {
    Elastic,
    Scrollable,
}

#[must_use]
pub(crate) const fn compact_detail_viewport(device: PerfDevice) -> CompactDetailViewport {
    match device {
        PerfDevice::Cpu | PerfDevice::Gpu(_) => CompactDetailViewport::Elastic,
        PerfDevice::Memory
        | PerfDevice::Disk(_)
        | PerfDevice::Network(_)
        | PerfDevice::Battery(_)
        | PerfDevice::Fan(_) => CompactDetailViewport::Scrollable,
    }
}

pub(crate) fn performance_page(
    app: &crate::IcedApp,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let selected = resolved_perf_device(app);
    // One frame-local presentation mapping at the page boundary: the rail is
    // the compact strip or the wide sidebar (responsive.rs owns the seam);
    // every branch below consumes the typed fact instead of re-reading the
    // viewport.
    let navigation = DeviceNavigationPresentation::for_compact_frame(app.compact_layout());
    let selector = performance_sidebar(app, theme_snapshot, selected, navigation);
    let detail = perf_detail(app, selected);
    let detail = column![detail]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);

    match navigation {
        DeviceNavigationPresentation::Strip => {
            // Compact windows stack the rail above the detail. Variable-length
            // device pages retain their independent scroll boundary; CPU and GPU
            // own bounded elastic aggregates and therefore must not show a scrollbar.
            let detail: Element<'_, Message, iced::Theme, iced::Renderer> =
                match compact_detail_viewport(selected) {
                    CompactDetailViewport::Elastic => detail.into(),
                    CompactDetailViewport::Scrollable => scrollable(detail)
                        .width(iced::Length::Fill)
                        .height(iced::Length::Fill)
                        .into(),
                };
            column![selector, detail]
                .spacing(8)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into()
        }
        DeviceNavigationPresentation::Sidebar => {
            if app.performance.sidebar_visible {
                row![selector, detail]
                    .spacing(12)
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .into()
            } else {
                // F9 hid the device sidebar (GPUI parity): the detail panel takes
                // the full width instead of an empty rail.
                row![detail]
                    .spacing(12)
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .into()
            }
        }
    }
}

/// The resolved Performance device for one frame: the selected device when it
/// is still visible, else the first visible device (CPU unless the user hid
/// it too) so the detail panel never renders a device the user asked to hide.
/// Pure seam the headless tests assert on.
#[must_use]
pub(crate) fn resolved_perf_device(app: &crate::IcedApp) -> PerfDevice {
    let available = available_perf_devices(app);
    if available.contains(&app.perf_device()) {
        app.perf_device()
    } else {
        available.first().copied().unwrap_or(PerfDevice::Cpu)
    }
}

/// GPUI-shaped device rail for the Iced Performance page. The active device
/// owns the detail card on the right. Wide windows render the information-dense
/// rail ([`perf_rail::device_cards`]: identity heading + two caption lines +
/// that device's own history sparkline per card, vertically windowed so many
/// disks/NICs stay reachable); compact windows use a separate horizontally
/// windowed pill strip so offscreen identities do not enter the element tree.
fn performance_sidebar<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
    selected: PerfDevice,
    navigation: DeviceNavigationPresentation,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let devices = available_perf_devices(app);
    // The page maps the frame's device navigation presentation once; the
    // remaining compact flags below are geometry consequences of that fact,
    // not a second breakpoint read.
    let compact = navigation == DeviceNavigationPresentation::Strip;
    let rail: Element<'a, Message, iced::Theme, iced::Renderer> = match navigation {
        DeviceNavigationPresentation::Strip => {
            let (scroll_x, viewport_width) = app.performance_rail_horizontal_scroll();
            let window = VirtualWindow::for_columns(
                devices.len(),
                scroll_x,
                viewport_width,
                perf_rail::COMPACT_DEVICE_ITEM_WIDTH,
                0.0,
            );
            let body = virtual_horizontal_body(window, iced::Length::Fixed(44.0), |start, end| {
                devices
                    .get(start..end)
                    .unwrap_or(&[])
                    .iter()
                    .map(|device| {
                        container(focus::choice_pill(
                            theme_snapshot,
                            FocusTarget::PerfDeviceTab(*device),
                            compact_performance_sidebar_label(app, *device),
                            *device == selected,
                            Message::SelectPerfDevice(*device),
                        ))
                        .width(iced::Length::Fixed(perf_rail::COMPACT_DEVICE_ITEM_WIDTH))
                        .into()
                    })
                    .collect()
            });
            scrollable(body)
                .id(app.performance_rail_scroll_id())
                .direction(iced::widget::scrollable::Direction::Horizontal(
                    iced::widget::scrollable::Scrollbar::default(),
                ))
                .height(iced::Length::Fixed(44.0))
                .width(iced::Length::Fill)
                .on_scroll(Message::PerformanceRailScrolled)
                .into()
        }
        DeviceNavigationPresentation::Sidebar => {
            let (scroll_y, viewport_height) = app.performance_rail_vertical_scroll();
            let window = VirtualWindow::for_rows(
                devices.len(),
                scroll_y,
                viewport_height,
                perf_rail::RAIL_CARD_HEIGHT,
                0.0,
            );
            // The wide rail scrolls vertically so lower devices stay reachable;
            // only the card window's captions/history facts enter the element tree.
            scrollable(perf_rail::device_cards(
                app,
                theme_snapshot,
                &devices,
                selected,
                window,
            ))
            .id(app.performance_rail_scroll_id())
            .direction(iced::widget::scrollable::Direction::Vertical(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .height(iced::Length::Fill)
            .width(iced::Length::Fill)
            .on_scroll(Message::PerformanceRailScrolled)
            .into()
        }
    };
    // The column must Fill the panel too: a Shrink column would hand the
    // scrollable an unbounded height and the rail would overflow the panel
    // instead of scrolling inside it.
    let list = column![
        text(t("sidebar.devices")).size(f32::from(tokens::FONT_14)),
        rail
    ]
    .spacing(8)
    .height(if compact {
        iced::Length::Shrink
    } else {
        iced::Length::Fill
    });
    container(list)
        .width(if compact {
            iced::Length::Fill
        } else {
            iced::Length::Fixed(perf_layout::geometry_contract(false).sidebar_width)
        })
        .height(if compact {
            iced::Length::Shrink
        } else {
            iced::Length::Fill
        })
        .padding(8)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

pub(crate) fn chunked_rows<'a>(
    items: Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
    columns: usize,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let columns = columns.max(1);
    let mut rows = Vec::with_capacity(chunk_count(items.len(), columns));
    let mut current = Vec::with_capacity(columns);
    for item in items {
        current.push(item);
        if current.len() == columns {
            rows.push(row(std::mem::take(&mut current)).spacing(5).into());
        }
    }
    if !current.is_empty() {
        rows.push(row(current).spacing(5).into());
    }
    column(rows).spacing(5).width(iced::Length::Fill).into()
}

pub(crate) fn chunk_count(item_count: usize, columns: usize) -> usize {
    item_count.div_ceil(columns.max(1))
}

/// Column count for the wrapped chrome's action toolbar. The 560px flip
/// point is the frame budget's toolbar threshold (responsive.rs), not a
/// local literal.
pub(crate) fn compact_toolbar_columns(width: f32) -> usize {
    if width < COMPACT_TOOLBAR_FIVE_COLUMN_MIN_WIDTH {
        3
    } else {
        5
    }
}

/// Keep compact Performance device pills fully visible. A fixed column count
/// is preferable to a horizontal scrollbar here: the selector is a small
/// finite vocabulary, so wrapping the pills preserves every identity and
/// keeps the scroll affordance reserved for detail content that can genuinely
/// overflow.
/// Build the same device-indexed navigation model as GPUI's sidebar. Dynamic
/// rows are derived from the current snapshot order; the index is part of the
/// frontend-local selection so two disks/NICs/GPUs can never collapse into one
/// Iced page. The persisted per-family visibility preferences (GPUI Settings
/// `devices` group) filter the rail: a hidden family drops out of the list
/// entirely, and a selection whose device disappeared falls back to CPU on the
/// next render.
pub(crate) fn available_perf_devices(app: &crate::IcedApp) -> Vec<PerfDevice> {
    let prefs = app.preferences();
    let mut devices = Vec::new();
    if prefs.show_cpu {
        devices.push(PerfDevice::Cpu);
    }
    if prefs.show_memory {
        devices.push(PerfDevice::Memory);
    }
    if let Some(snapshot) = app.shell.projection().snapshot.as_ref() {
        if prefs.show_disks {
            devices.extend(
                snapshot
                    .disks
                    .iter()
                    .enumerate()
                    .map(|(index, _)| PerfDevice::Disk(index)),
            );
        }
        if prefs.show_network {
            let categories = network_visibility(prefs);
            devices.extend(
                snapshot
                    .networks
                    .iter()
                    .enumerate()
                    .filter(|(_, network)| categories.allows(network.adapter_type()))
                    .map(|(index, _)| PerfDevice::Network(index)),
            );
        }
        if prefs.show_gpus {
            devices.extend(
                snapshot
                    .gpu
                    .iter()
                    .enumerate()
                    .map(|(index, _)| PerfDevice::Gpu(index)),
            );
        }
    }
    // Battery / Fan have no visibility toggle in the GPUI Settings devices
    // group (the ten toggles cover CPU/Memory/Disks/Network±subclasses/GPUs),
    // so their rail entries stay unconditional, matching GPUI.
    if let Some(power) = app.shell.projection().power_supplies.as_ref() {
        devices.extend(
            power
                .batteries
                .iter()
                .enumerate()
                .map(|(index, _)| PerfDevice::Battery(index)),
        );
    }
    if let Some(sensors) = app.shell.projection().sensors.as_ref() {
        devices.extend(
            sensors
                .readings
                .iter()
                .filter(|reading| {
                    reading.quantity() == &taskmanager_application::SensorQuantity::FanSpeed
                })
                .enumerate()
                .map(|(index, _)| PerfDevice::Fan(index)),
        );
    }
    devices
}

/// One quantity family's resolved unit pair (bytes-vs-bits, base-2-vs-base-10),
/// bundled so the device-block builders stay under clippy's argument budget.
#[derive(Clone, Copy, Debug)]
pub(crate) struct UnitPrefs {
    pub(crate) use_bytes: bool,
    pub(crate) use_base2: bool,
}

impl Default for UnitPrefs {
    /// The product default: binary bytes (bytes on the 1024 ladder).
    fn default() -> Self {
        Self {
            use_bytes: true,
            use_base2: true,
        }
    }
}

/// The resolved per-category network visibility (GPUI `sidebar::NetworkFilter`
/// parity): Ethernet + the `other` bucket (loopback, unclassified) collapse
/// onto the Wired/Other toggles the same way the GPUI sidebar filters them.
#[derive(Clone, Copy)]
struct NetworkVisibility {
    wired: bool,
    wireless: bool,
    vpn: bool,
    virtual_devices: bool,
    other: bool,
}

impl NetworkVisibility {
    const fn allows(self, adapter_type: taskmanager_application::NetworkAdapterType) -> bool {
        match adapter_type {
            taskmanager_application::NetworkAdapterType::Ethernet => self.wired,
            taskmanager_application::NetworkAdapterType::WiFi => self.wireless,
            taskmanager_application::NetworkAdapterType::Vpn => self.vpn,
            taskmanager_application::NetworkAdapterType::Virtual => self.virtual_devices,
            taskmanager_application::NetworkAdapterType::Unknown
            | taskmanager_application::NetworkAdapterType::Loopback
            | taskmanager_application::NetworkAdapterType::Other => self.other,
        }
    }
}

fn network_visibility(prefs: &crate::app::PresentationPreferences) -> NetworkVisibility {
    NetworkVisibility {
        wired: prefs.show_network_wired,
        wireless: prefs.show_network_wireless,
        vpn: prefs.show_network_vpn,
        virtual_devices: prefs.show_network_virtual,
        other: prefs.show_network_other,
    }
}

/// Compose the compact two-column sidebar label from the page's data-layer
/// projection. The renderer owns only localized copy and bounded layout text;
/// [`crate::app::IcedApp::performance_sidebar_detail`] owns the observation
/// fold so the rail never becomes a second data source.
pub(crate) fn performance_sidebar_label(app: &crate::IcedApp, device: PerfDevice) -> String {
    let label = perf_device_label(device);
    app.performance_sidebar_detail(device)
        .map_or_else(|| label.to_owned(), |detail| format!("{label}  {detail}"))
}

/// Compact selector labels keep the stable device family plus a bounded
/// identity/value. The detail card still owns the full model/interface name;
/// the selector only needs enough text to distinguish siblings without
/// forcing a horizontal scrollbar or clipping a neighboring pill.
fn compact_performance_sidebar_label(app: &crate::IcedApp, device: PerfDevice) -> String {
    bounded_sidebar_label(&performance_sidebar_label(app, device), 18)
}

pub(crate) fn bounded_sidebar_label(label: &str, max_chars: usize) -> String {
    let chars: Vec<char> = label.chars().collect();
    if chars.len() <= max_chars {
        return label.to_owned();
    }
    let take = max_chars.saturating_sub(1).max(1);
    format!("{}…", chars.into_iter().take(take).collect::<String>())
}

/// The localized label for one selector tab. Reuses the existing common/sidebar
/// catalog keys verbatim — no new locale entries (CPU/Memory/Gpu/Disk via
/// `common.*`, Network via `sidebar.network`).
#[must_use]
pub(crate) fn perf_device_label(device: PerfDevice) -> &'static str {
    match device {
        PerfDevice::Cpu => t("common.cpu"),
        PerfDevice::Memory => t("common.memory"),
        PerfDevice::Disk(_) => t("common.disk"),
        PerfDevice::Network(_) => t("sidebar.network"),
        PerfDevice::Gpu(_) => t("common.gpu"),
        PerfDevice::Battery(_) => t("common.battery"),
        PerfDevice::Fan(_) => t("common.fan"),
    }
}

/// Which detail panel one selector value renders — the single selector→panel
/// mapping that both the renderer ([`perf_detail`]) and the headless tests
/// agree on (mirrors [`sort_arrow`]/[`apps_columns`] as a pure seam, so the
/// assertion needs no pixel read-back). `Cpu` and `Memory` share one dispatch
/// entry but immediately render distinct fixed projections.
#[must_use]
pub(crate) fn perf_detail_kind(device: PerfDevice) -> PerfDetail {
    match device {
        PerfDevice::Cpu | PerfDevice::Memory => PerfDetail::CpuOrMemory,
        PerfDevice::Disk(_) => PerfDetail::Disk,
        PerfDevice::Network(_) => PerfDetail::Network,
        PerfDevice::Gpu(_) => PerfDetail::Gpu,
        PerfDevice::Battery(_) => PerfDetail::Battery,
        PerfDevice::Fan(_) => PerfDetail::Fan,
    }
}

/// The detail-panel kind one selector value renders (the test seam for
/// [`perf_detail_kind`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerfDetail {
    CpuOrMemory,
    Disk,
    Network,
    Gpu,
    Battery,
    Fan,
}

/// Render ONLY the selected resource's detail panel. The sections are the
/// unchanged existing fns; this dispatches on [`perf_detail_kind`] (the single
/// selector→panel mapping) so the page displays one device at a time (MC's
/// model) instead of stacking all.
fn perf_detail(
    app: &crate::IcedApp,
    device: PerfDevice,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    match perf_detail_kind(device) {
        PerfDetail::CpuOrMemory => cpu_memory_detail(app, device),
        PerfDetail::Disk => disk_section(app, device.index().unwrap_or(0)),
        PerfDetail::Network => network_section(app, device.index().unwrap_or(0)),
        PerfDetail::Gpu => gpu_section(app, device.index().unwrap_or(0)),
        PerfDetail::Battery => battery_section(app, device.index().unwrap_or(0)),
        PerfDetail::Fan => fan::fan_section(app, device.index().unwrap_or(0)),
    }
}
