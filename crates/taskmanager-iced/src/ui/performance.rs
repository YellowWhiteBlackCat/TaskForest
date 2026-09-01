//! Iced Performance page composition and frontend-local device navigation.
//!
//! This module owns the selector → detail mapping, responsive rail geometry,
//! bounded device windows, and unit preferences. It does not collect provider
//! data; every label and panel reads the current Iced projection. The page's
//! ONE layout authority is [`PerformancePageBudget::for_perf_frame`]: the
//! typed slot allocation from the real tracked viewport (GPUI `from_frame`
//! parity). No secondary compact-flag derivation remains.

use iced::Element;
use iced::widget::{column, container, row, scrollable, text};
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens;

use super::history_replay;
use super::responsive::{
    COMPACT_TOOLBAR_FIVE_COLUMN_MIN_WIDTH, DeviceNavigationPresentation, PerformancePageBudget,
};
use super::{
    VirtualWindow, battery_section, cpu_memory_detail, disk_section, fan, gpu_section,
    network_section, perf_rail, virtual_horizontal_body,
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
    // The ONE frame-local layout authority: the typed slot allocation from
    // the real tracked viewport. A hidden sidebar collapses navigation to the
    // strip (GPUI parity) so every device stays reachable at any width; the
    // statistics rail is Pinned while capacity allows, Stacked below the
    // main viewport while the main floor survives, and only then Hidden.
    let budget =
        PerformancePageBudget::for_perf_frame(app.viewport_size(), app.performance.sidebar_visible);
    let selected = resolved_perf_device(app);
    let navigation = budget.device_navigation;

    // History-replay entry (GPUI parity): one toggle above the workspace,
    // present ONLY when persistence supplied a query — disabled persistence
    // shows nothing, never a dead button. While open, the replay view
    // replaces every device's live graphs; the rail keeps navigating.
    let replay_open = app.history_replay_entry_available() && app.history_replay_state().is_open();
    let entry_row = history_replay_entry_row(app, theme_snapshot);

    let detail: Element<'_, Message, iced::Theme, iced::Renderer> = if replay_open {
        scrollable(history_replay::render_history_replay(
            theme_snapshot,
            app.history_replay_state(),
            &app.local_time_rules,
        ))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into()
    } else {
        match compact_detail_viewport(selected) {
            CompactDetailViewport::Elastic => perf_detail(app, selected, budget),
            CompactDetailViewport::Scrollable => scrollable(perf_detail(app, selected, budget))
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into(),
        }
    };
    let detail = column![detail]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill);

    let selector = performance_sidebar(app, theme_snapshot, selected, navigation, budget);
    match navigation {
        DeviceNavigationPresentation::Strip => {
            // Strip frames stack the rail above the detail. Variable-length
            // device pages retain their independent scroll boundary; CPU and
            // GPU own bounded elastic aggregates and therefore must not show
            // a scrollbar.
            let mut children: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> =
                Vec::with_capacity(3);
            if let Some(entry) = entry_row {
                children.push(entry);
            }
            children.push(selector);
            children.push(detail.into());
            column(children)
                .spacing(8)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into()
        }
        DeviceNavigationPresentation::Sidebar => {
            // The frame budget only admits the sidebar while it is visible
            // AND every semantic slot stays readable; hidden-sidebar frames
            // collapsed to the strip branch above (F9 GPUI parity).
            let mut detail_stack: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> =
                Vec::with_capacity(2);
            if let Some(entry) = entry_row {
                detail_stack.push(entry);
            }
            detail_stack.push(detail.into());
            row![selector, column(detail_stack).spacing(8)]
                .spacing(12)
                .width(iced::Length::Fill)
                .height(iced::Length::Fill)
                .into()
        }
    }
}

/// The replay toggle row pinned above the Performance workspace. Only the
/// entry-availability fact decides between the live toggle, the
/// startup-unavailable notice, and nothing at all.
fn history_replay_entry_row<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    if app.history_replay_startup_unavailable() {
        return Some(
            row![iced::widget::Space::new().width(iced::Length::Fill)]
                .push(
                    text(t("perf.replay.startup_unavailable"))
                        .size(f32::from(tokens::FONT_11))
                        .color(theme::muted_text_color(theme_snapshot)),
                )
                .width(iced::Length::Fill)
                .height(iced::Length::Fixed(24.0))
                .into(),
        );
    }
    app.history_replay_entry_available().then(|| {
        row![iced::widget::Space::new().width(iced::Length::Fill)]
            .push(focus::dynamic_button(
                theme_snapshot,
                FocusTarget::HistoryReplayToggle,
                t(if app.history_replay_state().is_open() {
                    "perf.replay.back_to_live"
                } else {
                    "perf.replay.toggle"
                })
                .to_string(),
                Message::ToggleHistoryReplay,
                false,
            ))
            .width(iced::Length::Fill)
            .height(iced::Length::Fixed(30.0))
            .into()
    })
}

/// The resolved Performance device for one frame: the selected device when it
/// is still visible, else the first visible device (CPU unless the user hid
/// it too) so the rail never highlights a device the user asked to hide. The
/// DETAIL panel separately renders the explicit disconnected surface when the
/// user's own selection pointed at a device that disappeared (GPUI
/// `selected_device_missing` parity) instead of silently migrating the page.
#[must_use]
pub(crate) fn resolved_perf_device(app: &crate::IcedApp) -> PerfDevice {
    let available = available_perf_devices(app);
    if available.contains(&app.perf_device()) {
        app.perf_device()
    } else {
        available.first().copied().unwrap_or(PerfDevice::Cpu)
    }
}

/// Whether the user's selected device vanished from a still-visible family
/// (index out of the live snapshot's range). A family the user hid by
/// preference is intent, not disconnection, and stays `false`.
#[must_use]
pub(crate) fn selection_disconnected(app: &crate::IcedApp) -> bool {
    let projection = app.shell.projection();
    let Some(snapshot) = projection.snapshot.as_ref() else {
        return false;
    };
    let prefs = app.preferences();
    match app.perf_device() {
        PerfDevice::Cpu | PerfDevice::Memory => false,
        PerfDevice::Disk(index) => prefs.show_disks && index >= snapshot.disks.len(),
        PerfDevice::Network(index) => prefs.show_network && index >= snapshot.networks.len(),
        PerfDevice::Gpu(index) => prefs.show_gpus && index >= snapshot.gpu.len(),
        PerfDevice::Battery(index) => projection
            .power_supplies
            .as_ref()
            .is_some_and(|power| index >= power.batteries.len()),
        PerfDevice::Fan(index) => projection.sensors.as_ref().is_some_and(|sensors| {
            index
                >= sensors
                    .readings
                    .iter()
                    .filter(|reading| {
                        reading.quantity()
                            == &taskmanager_core::core::sensors::SensorQuantity::FanSpeed
                    })
                    .count()
        }),
    }
}

/// The explicit disconnected-device surface (GPUI `disconnected_device`
/// parity): the page names the loss and waits for the hardware identity
/// instead of silently rendering another device's facts.
fn disconnected_device<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    container(
        column![
            text(t("device.disconnected"))
                .size(f32::from(tokens::FONT_15))
                .font(crate::theme_binding::ui_font_weight(
                    theme_snapshot,
                    tokens::FONT_WEIGHT_HEADER,
                )),
            text(t("device.reconnect_hint"))
                .size(f32::from(tokens::FONT_12))
                .color(theme::muted_text_color(theme_snapshot)),
        ]
        .spacing(8)
        .width(iced::Length::Fill),
    )
    .max_width(460.0)
    .padding(18.0)
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .center_x(iced::Length::Fill)
    .center_y(iced::Length::Fill)
    .style(move |_| theme::card_style(theme_snapshot))
    .into()
}

/// GPUI-shaped device rail for the Iced Performance page. The active device
/// owns the detail card on the right. Wide windows render the information-dense
/// rail ([`perf_rail::device_cards`]: identity heading + two caption lines +
/// that device's own history sparkline per card, vertically windowed so many
/// disks/NICs stay reachable); strip frames use a separate horizontally
/// windowed pill strip so offscreen identities do not enter the element tree.
fn performance_sidebar<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
    selected: PerfDevice,
    navigation: DeviceNavigationPresentation,
    budget: PerformancePageBudget,
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
                // The nested rail scrollbar is thinned to the stats-rail
                // gauge (4px): a default-width bar inside the card stack read
                // as a broken double-segment element against the outer page
                // scrollbar (ICED-024 目检项 21).
                iced::widget::scrollable::Scrollbar::new()
                    .width(4)
                    .scroller_width(4),
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
            iced::Length::Fixed(budget.sidebar_width)
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
                    reading.quantity() == &taskmanager_core::core::sensors::SensorQuantity::FanSpeed
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
    const fn allows(
        self,
        adapter_type: taskmanager_core::core::metrics::NetworkAdapterType,
    ) -> bool {
        match adapter_type {
            taskmanager_core::core::metrics::NetworkAdapterType::Ethernet => self.wired,
            taskmanager_core::core::metrics::NetworkAdapterType::WiFi => self.wireless,
            taskmanager_core::core::metrics::NetworkAdapterType::Vpn => self.vpn,
            taskmanager_core::core::metrics::NetworkAdapterType::Virtual => self.virtual_devices,
            taskmanager_core::core::metrics::NetworkAdapterType::Unknown
            | taskmanager_core::core::metrics::NetworkAdapterType::Loopback
            | taskmanager_core::core::metrics::NetworkAdapterType::Other => self.other,
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

/// Render ONLY the selected resource's detail panel through the frame's
/// typed budget. The disconnected seam comes first: when the user's own
/// selection pointed at a device that vanished, the page names the loss
/// explicitly instead of silently rendering the fallback device's facts.
fn perf_detail(
    app: &crate::IcedApp,
    device: PerfDevice,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    if selection_disconnected(app) {
        let theme_snapshot = app.theme();
        return disconnected_device(theme_snapshot);
    }
    match perf_detail_kind(device) {
        PerfDetail::CpuOrMemory => cpu_memory_detail(app, device, budget),
        PerfDetail::Disk => disk_section(app, device.index().unwrap_or(0), budget),
        PerfDetail::Network => network_section(app, device.index().unwrap_or(0), budget),
        PerfDetail::Gpu => gpu_section(app, device.index().unwrap_or(0), budget),
        PerfDetail::Battery => battery_section(app, device.index().unwrap_or(0), budget),
        PerfDetail::Fan => fan::fan_section(app, device.index().unwrap_or(0), budget),
    }
}
