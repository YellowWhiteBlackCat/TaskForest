//! Shared presentation: command/page help metadata, monochrome icon glyphs,
//! and the single-source byte/duration formatters every frontend renders
//! (ADR-020 single-source rule; the GPUI, TUI and iced frontends all call
//! these — never a per-frontend copy).

use taskmanager_application::{
    AppPage, CommandId, DeviceStatus, DiskMetrics, FailureKind, GpuMetrics, KeyCode,
    LocalTimeRulesObservation, Modifiers, PriorityTier, SmartAvailability, default_bindings, i18n,
};
use taskmanager_ui_contract::{IconId, MessageKey, descriptor, page_descriptors, page_shortcut};

pub mod gpu_chart_metric;
pub mod gpu_engine_rows;

/// The single missing-value placeholder every frontend renders for an
/// uncollected-but-applicable observation (an em dash). "求同": one spelling,
/// one semantic — frontends that prefer hiding a row entirely simply omit it
/// instead of substituting their own placeholder text.
pub const MISSING_VALUE: &str = "—";

/// Owned form of [`MISSING_VALUE`] for `String` readouts.
#[must_use]
pub fn missing_value() -> String {
    MISSING_VALUE.to_owned()
}

/// Format a byte count the same way every frontend does: binary units
/// (`KiB`/`MiB`/`GiB`), one decimal place above the byte tier. This is the
/// SINGLE implementation — frontends must not re-format byte counts.
#[must_use]
pub fn bytes(value: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = value as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{value:.0} B")
    }
}

/// Format an optional resource value without collapsing an unavailable
/// observation into the numeric zero. This is shared by the TUI and Iced
/// Apps projections for typed PSS and swap columns.
#[must_use]
pub fn optional_bytes(value: Option<u64>) -> String {
    value.map_or_else(missing_value, bytes)
}

/// Format an uptime the same way every frontend does: `1d 01h 00m` past a
/// day, `00h 01m` otherwise. The SINGLE implementation.
#[must_use]
pub fn duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h {minutes:02}m")
    } else {
        format!("{hours:02}h {minutes:02}m")
    }
}

/// Format an optional count (threads / fds) without collapsing an unavailable
/// observation into a believable zero: `None` renders an honest dash.
#[must_use]
pub fn optional_count(value: Option<u32>) -> String {
    value.map_or_else(missing_value, |count| count.to_string())
}

/// Format an optional second-duration (CPU time) through the shared
/// [`duration`] helper, with `None` rendering an honest dash.
#[must_use]
pub fn optional_duration(value: Option<u64>) -> String {
    value.map_or_else(missing_value, duration)
}

/// Format an optional niceness value, signing a positive priority (`+10`) so it
/// is visually distinct from a measured zero; `None` renders an honest dash.
#[must_use]
pub fn optional_nice(value: Option<i32>) -> String {
    value.map_or_else(missing_value, |nice| {
        if nice > 0 {
            format!("+{nice}")
        } else {
            nice.to_string()
        }
    })
}

/// Locale label for a scheduling-priority tier — the SINGLE fold (§4.0 同一律)
/// shared by every frontend's menus, toasts, and confirmations. The tier
/// word, not a raw Unix nice number, is the honest cross-platform phrasing
/// (each platform adapter maps the tier to its native primitive).
#[must_use]
pub fn priority_tier_label(tier: PriorityTier) -> &'static str {
    i18n::t(tier.i18n_key())
}

/// Format an injected Unix process start instant as the user's local `HH:MM`.
/// Missing rules, a missing/zero epoch, and out-of-range values are all honest
/// dashes; UTC is used only when the injected rule set itself is fixed UTC.
#[must_use]
pub fn start_clock_local(epoch_secs: Option<u64>, rules: &LocalTimeRulesObservation) -> String {
    let Some(epoch_secs) = epoch_secs.filter(|seconds| *seconds != 0) else {
        return missing_value();
    };
    let Some(epoch_secs) = i64::try_from(epoch_secs).ok() else {
        return missing_value();
    };
    rules
        .date_time_at(epoch_secs)
        .map_or_else(missing_value, |local| {
            format!("{:02}:{:02}", local.hour(), local.minute())
        })
}

/// Format injected epoch milliseconds as a local civil timestamp. A local-
/// time capability failure stays a dash instead of relabeling UTC as local.
#[must_use]
pub fn local_timestamp(epoch_millis: u64, rules: &LocalTimeRulesObservation) -> String {
    let epoch_seconds = epoch_millis / 1_000;
    taskmanager_application::process_details_vm::format_local_timestamp_seconds(
        epoch_seconds,
        rules,
    )
    .unwrap_or_else(missing_value)
}

/// Format a temperature in °C the way every badge/graph readout does
/// (whole degrees). This is the SINGLE °C spelling (ADR-020) — frontends
/// must not re-format the unit.
#[must_use]
pub fn temperature_c(value: f32) -> String {
    format!("{value:.0} °C")
}

/// Renderer-neutral GPU identity for titles and device rails.
///
/// A resolved product name is the most specific identity, while `brand` is a
/// stable generic vendor/driver label and therefore becomes its qualifier.
/// The native driver (`xe`, `amdgpu`, `nvidia`, …) is deliberately absent from
/// this projection: it remains a separate technical fact in device details.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuDisplayIdentity<'a> {
    pub headline: Option<&'a str>,
    pub qualifier: Option<&'a str>,
}

/// Select the most specific honest GPU identity without inventing a product
/// name when native identity resolution did not provide one.
#[must_use]
pub fn gpu_display_identity(gpu: &GpuMetrics) -> GpuDisplayIdentity<'_> {
    let marketing_name = gpu
        .marketing_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let brand = (!gpu.brand.trim().is_empty()).then(|| gpu.brand.trim());

    match (marketing_name, brand) {
        (Some(headline), Some(brand)) if !headline.eq_ignore_ascii_case(brand) => {
            GpuDisplayIdentity {
                headline: Some(headline),
                qualifier: Some(brand),
            }
        }
        (Some(headline), _) => GpuDisplayIdentity {
            headline: Some(headline),
            qualifier: None,
        },
        (None, Some(headline)) => GpuDisplayIdentity {
            headline: Some(headline),
            qualifier: None,
        },
        (None, None) => GpuDisplayIdentity::default(),
    }
}

/// Format a temperature in °C with one decimal — the health-page convention
/// (a per-surface display choice, deliberately more precise than the
/// badge/graph [`temperature_c`]).
#[must_use]
pub fn temperature_c_precise(value: f32) -> String {
    format!("{value:.1} °C")
}

/// Format a fan speed in RPM the way every badge/graph readout does
/// (rounded to a whole number). The SINGLE RPM spelling for `f32` samples.
#[must_use]
pub fn fan_rpm(value: f32) -> String {
    format!("{value:.0} RPM")
}

/// Format an integral fan speed (`u32` sensor value) as a bare integer +
/// `RPM` — the health-page convention, which shows the exact reading rather
/// than a rounded one.
#[must_use]
pub fn fan_rpm_i(value: u32) -> String {
    format!("{value} RPM")
}

/// Format a power draw in watts with one decimal — the badge/graph
/// convention. The SINGLE W spelling for `f32` samples.
#[must_use]
pub fn power_w(value: f32) -> String {
    format!("{value:.1} W")
}

/// Format a power draw in watts with two decimals — the health-page
/// convention (a per-surface display choice, more precise than
/// [`power_w`]).
#[must_use]
pub fn power_w_precise(value: f32) -> String {
    format!("{value:.2} W")
}

/// Format a clock frequency in MHz (whole numbers) — the badge/graph
/// convention. The SINGLE MHz spelling.
#[must_use]
pub fn megahertz(value: f32) -> String {
    format!("{value:.0} MHz")
}

/// Finite statistics for one renderer-facing history graph.
///
/// Every frontend uses the same reduction: provider gaps are ignored, the
/// latest field is the newest finite sample (not necessarily the last array
/// element), and an all-gap/empty window returns `None`. Keeping this in the
/// shell prevents the TUI, Iced, and GPUI summaries from disagreeing about
/// what a graph's "latest", "average", or "peak" means.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphSummary {
    pub latest: f32,
    pub average: f32,
    pub minimum: f32,
    pub maximum: f32,
    pub sample_count: usize,
}

/// Reduce one graph sample window without turning provider gaps into values.
#[must_use]
pub fn graph_summary(samples: &[f32]) -> Option<GraphSummary> {
    let mut latest = None;
    let mut sum = 0.0_f32;
    let mut count = 0_usize;
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;

    for &value in samples {
        if !value.is_finite() {
            continue;
        }
        latest = Some(value);
        sum += value;
        count = count.saturating_add(1);
        minimum = minimum.min(value);
        maximum = maximum.max(value);
    }

    let latest = latest?;
    let average = sum / count as f32;
    if !average.is_finite() {
        return None;
    }
    Some(GraphSummary {
        latest,
        average,
        minimum,
        maximum,
        sample_count: count,
    })
}

/// Peak over a finite-sample window, floored by the optional current
/// observation (a series with no history still reports the live reading).
/// Returns `None` when neither the window nor the current value holds data —
/// the honest absence callers render as a dash, never a fabricated zero.
/// This is the SINGLE peak fold (ADR-020): the GPUI Properties dialog and the
/// TUI Properties modal must not disagree about what "peak" means.
#[must_use]
pub fn peak_of(samples: &[f32], current: Option<f32>) -> Option<f32> {
    let mut peak = current.map(|value| value.max(0.0));
    for &sample in samples {
        if sample.is_finite() {
            peak = Some(peak.map_or(sample, |known| known.max(sample)));
        }
    }
    peak.map(|value| value.max(0.0))
}

/// Localized `"{value} (peak {peak})"` join for the process Properties
/// performance readouts. Either side may be missing: a missing peak renders
/// the bare value, a missing value with a known peak keeps the dash, and no
/// data at all renders the shared dash — never `0.0` invented from `None`.
#[must_use]
pub fn value_with_peak(value: Option<String>, peak: Option<String>) -> String {
    match (&value, &peak) {
        (None, None) => missing_value(),
        (Some(value), None) => value.clone(),
        _ => i18n::t("common.value_with_peak")
            .replace("{value}", value.as_deref().unwrap_or(MISSING_VALUE))
            .replace("{peak}", peak.as_deref().unwrap_or(MISSING_VALUE)),
    }
}

/// Percent-encode a string for a URL query component: RFC 3986 unreserved
/// characters pass through, every other byte becomes `%XX`. Pure (no native
/// command) so it lives here in the shell; the frontend builds a search URL and
/// emits `PlatformEffect::OpenUrl`, and the platform adapter owns the spawn.
#[must_use]
pub fn url_encode_query(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                output.push(char::from(byte));
            }
            _ => {
                output.push('%');
                output.push_str(&format!("{byte:02X}"));
            }
        }
    }
    output
}

/// The web-search URL for a process name (Google), query-percent-encoded so a
/// multi-word name does not break the URL.
#[must_use]
pub fn search_url_for(name: &str) -> String {
    format!("https://www.google.com/search?q={}", url_encode_query(name))
}

/// One discoverable shortcut row ready for a terminal renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandHelp {
    pub command: CommandId,
    pub icon: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub shortcut: &'static str,
}

/// Shared page-navigation presentation consumed by terminal and desktop
/// renderer adapters. Labels and descriptions resolve through the shared
/// i18n catalog, so every frontend renders the same localized copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageHelp {
    pub page: AppPage,
    pub command: CommandId,
    pub icon: IconId,
    pub label: &'static str,
    pub description: &'static str,
    pub shortcut: &'static str,
}

/// Complete shared shortcut help. Keeping this derived from `CommandId::ALL`
/// makes missing presentation metadata a test failure instead of a silent UI gap.
#[must_use]
pub fn command_help() -> Vec<CommandHelp> {
    CommandId::ALL
        .into_iter()
        .map(|command| {
            let metadata = descriptor(command);
            CommandHelp {
                command,
                icon: metadata.icon.map_or("·", icon_glyph),
                label: message(metadata.label),
                description: message(metadata.description),
                shortcut: shortcut(command),
            }
        })
        .collect()
}

/// Complete shared page navigation presentation in canonical route order.
#[must_use]
pub fn page_help() -> [PageHelp; 7] {
    page_descriptors().map(|page| {
        let shortcut = page_shortcut(page.page).map_or("", |binding| {
            chord_label(binding.chord.key, binding.chord.modifiers)
        });
        PageHelp {
            page: page.page,
            command: page.command,
            icon: page.icon,
            label: page_label(page.page),
            description: message(page.description),
            shortcut,
        }
    })
}

/// Localized page-tab label resolved through the shared i18n catalog — the
/// visible tab text rendered by every `page_help()` consumer (e.g. the TUI
/// header). Tab labels use the `tab.*` keys; command copy (labels and
/// descriptions) resolves through the per-command `command.*` keys carried
/// by the application spec table.
fn page_label(page: AppPage) -> &'static str {
    match page {
        AppPage::Performance => i18n::t("tab.performance"),
        AppPage::Applications => i18n::t("tab.apps"),
        AppPage::Services => i18n::t("tab.services"),
        AppPage::System => i18n::t("tab.system"),
        AppPage::Startup => i18n::t("tab.startup"),
        AppPage::Users => i18n::t("tab.users"),
        AppPage::AppHistory => i18n::t("tab.apphistory_short"),
    }
}

fn shortcut(command: CommandId) -> &'static str {
    default_bindings()
        .iter()
        .find(|binding| binding.command == command)
        .map_or("", |binding| {
            chord_label(binding.chord.key, binding.chord.modifiers)
        })
}

const fn chord_label(key: KeyCode, modifiers: Modifiers) -> &'static str {
    match (key, modifiers.control, modifiers.alt, modifiers.shift) {
        (KeyCode::F, true, false, false) => "Ctrl+F",
        (KeyCode::Space, true, false, false) => "Ctrl+Space",
        (KeyCode::Digit1, false, true, false) => "Alt+1",
        (KeyCode::Digit2, false, true, false) => "Alt+2",
        (KeyCode::Digit3, false, true, false) => "Alt+3",
        (KeyCode::Digit4, false, true, false) => "Alt+4",
        (KeyCode::Digit5, false, true, false) => "Alt+5",
        (KeyCode::Digit6, false, true, false) => "Alt+6",
        (KeyCode::Digit7, false, true, false) => "Alt+7",
        (KeyCode::Digit8, false, true, false) => "Alt+8",
        (KeyCode::Tab, false, false, true) => "Shift+Tab",
        (KeyCode::Tab, false, false, false) => "Tab",
        (KeyCode::PageUp, false, false, false) => "PageUp",
        (KeyCode::PageDown, false, false, false) => "PageDown",
        (KeyCode::ArrowUp, false, false, false) => "Up",
        (KeyCode::ArrowDown, false, false, false) => "Down",
        (KeyCode::ArrowLeft, false, false, false) => "Left",
        (KeyCode::ArrowRight, false, false, false) => "Right",
        (KeyCode::Home, false, false, false) => "Home",
        (KeyCode::End, false, false, false) => "End",
        (KeyCode::F1, false, false, false) => "F1",
        (KeyCode::F5, false, false, false) => "F5",
        (KeyCode::F9, false, false, false) => "F9",
        (KeyCode::A, true, false, false) => "Ctrl+A",
        (KeyCode::C, true, false, false) => "Ctrl+C",
        (KeyCode::Delete, false, false, false) => "Delete",
        (KeyCode::Enter, false, false, false) => "Enter",
        (KeyCode::Escape, false, false, false) => "Escape",
        _ => "",
    }
}

/// Resolve one command message key through the shared i18n catalog. The
/// key strings come from the application command spec table (single
/// source); a key missing from the catalogs degrades to the key literal,
/// never a panic.
fn message(key: MessageKey) -> &'static str {
    match key {
        MessageKey::CommandLabel(command) => i18n::t(command.label_key()),
        MessageKey::CommandDescription(command) => i18n::t(command.description_key()),
    }
}

/// Monochrome terminal fallback for every semantic icon.
#[must_use]
pub const fn icon_glyph(icon: IconId) -> &'static str {
    match icon {
        IconId::Cpu => "◇",
        IconId::Memory => "▤",
        IconId::Disk => "▱",
        IconId::Network => "↔",
        IconId::Gpu => "▣",
        IconId::Process => "≡",
        IconId::Service => "⚙",
        IconId::Startup => "↗",
        IconId::User => "○",
        IconId::Health => "♡",
        IconId::Alert => "△",
        IconId::Export => "⇥",
        IconId::Settings => "⌘",
        IconId::Search => "⌕",
        IconId::More => "…",
        IconId::NavigateUp => "↑",
        IconId::NavigateDown => "↓",
        IconId::Focus => "◎",
        IconId::Performance => "⌁",
        IconId::Applications => "▦",
        IconId::Services => "⚙",
        IconId::System => "□",
        IconId::Users => "○",
        IconId::Refresh => "↻",
        IconId::EndTask => "×",
        IconId::Properties => "☷",
        IconId::Close => "×",
        IconId::Pause => "Ⅱ",
        IconId::Sidebar => "▥",
        IconId::CircleCheck => "✓",
        IconId::CircleX => "×",
        IconId::TriangleAlert => "△",
        IconId::History => "⌛",
    }
}

/// The localized-status i18n key for one [`DeviceStatus`] variant. Shared so
/// every frontend renders the same status text for the same `DeviceState::status`
/// (ADR-027 single-source; previously GPUI-local in `perf_views::smart_status`).
#[must_use]
pub fn device_status_i18n_key(status: DeviceStatus) -> &'static str {
    match status {
        DeviceStatus::Healthy => "device.healthy",
        DeviceStatus::Stale => "device.stale",
        DeviceStatus::PermissionDenied => "device.permission_denied",
        DeviceStatus::MissingTool => "device.missing_tool",
        DeviceStatus::Unsupported => "device.unsupported",
    }
}

/// The actionable-guidance i18n key for one non-healthy [`DeviceStatus`] (the
/// localized copy lives in `locales/*.json` under `device.*`). Shared so every
/// frontend renders the same actionable guidance for the same status.
#[must_use]
pub fn device_action_i18n_key(status: DeviceStatus) -> &'static str {
    match status {
        DeviceStatus::PermissionDenied => "device.action_permission",
        DeviceStatus::MissingTool => "device.action_missing_tool",
        DeviceStatus::Stale => "device.action_stale",
        DeviceStatus::Unsupported => "device.action_unsupported",
        DeviceStatus::Healthy => "device.healthy",
    }
}

/// The localized failure detail for one typed control failure (single source
/// for the `feedback.*` copy table; mirrors gpui's `control_error_detail` so
/// every frontend renders the same actionable reason). `RequiresEscalation`
/// folds into the denial copy — the escalation prompt itself is the action.
#[must_use]
pub fn control_error_detail(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unsupported => i18n::t("feedback.unsupported"),
        FailureKind::TemporarilyUnavailable => i18n::t("feedback.provider_unavailable"),
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            i18n::t("feedback.permission_denied")
        }
        FailureKind::MissingDependency => i18n::t("feedback.provider_unavailable"),
        FailureKind::TimedOut => i18n::t("feedback.timed_out"),
        FailureKind::Rejected => i18n::t("feedback.request_rejected"),
        FailureKind::IdentityChanged => i18n::t("feedback.target_changed"),
        FailureKind::ProviderFault => i18n::t("feedback.provider_failed"),
    }
}

/// The localized-availability i18n key for one [`SmartAvailability`] variant.
#[must_use]
pub fn smart_availability_i18n_key(availability: SmartAvailability) -> &'static str {
    match availability {
        SmartAvailability::Available => "disk.smart_available",
        SmartAvailability::Unsupported => "disk.smart_unsupported",
        SmartAvailability::Unavailable => "disk.smart_unavailable",
        SmartAvailability::MissingTool => "disk.smart_missing_tool",
        SmartAvailability::PermissionDenied => "device.permission_denied",
    }
}

/// The effective [`DeviceStatus`] for a disk: the SMART lifecycle status when it
/// is authoritative, otherwise the availability fallback — so a provider that
/// never opened reads as `Stale` / `PermissionDenied` / `MissingTool` rather
/// than silently `Healthy`. Shared so iced and GPUI never disagree on which
/// status to render for the same disk.
#[must_use]
pub fn effective_smart_status(disk: &DiskMetrics) -> DeviceStatus {
    if disk.smart_state.status != DeviceStatus::Unsupported
        || disk.smart_availability == SmartAvailability::Unsupported
    {
        return disk.smart_state.status;
    }
    match disk.smart_availability {
        SmartAvailability::Available => DeviceStatus::Healthy,
        SmartAvailability::Unsupported => DeviceStatus::Unsupported,
        SmartAvailability::Unavailable => DeviceStatus::Stale,
        SmartAvailability::MissingTool => DeviceStatus::MissingTool,
        SmartAvailability::PermissionDenied => DeviceStatus::PermissionDenied,
    }
}

/// Whether a disk exposes any SMART readout worth rendering. Shared so both
/// frontends hide the SMART section for the same disks.
#[must_use]
pub fn has_smart_fields(disk: &DiskMetrics) -> bool {
    disk.smart_temperature_c.is_some()
        || disk.smart_critical_warning.is_some()
        || disk.smart_temp_critical_c.is_some()
        || disk.smart_percent_used.is_some()
        || disk.smart_power_on_hours.is_some()
}

/// Whether a disk's SMART section should render at all. A provider that could
/// not supply usable SMART telemetry (unsupported, unavailable, missing tool,
/// or permission denied) yields no section: nothing to show beats an
/// unavailable status row for a fact the host cannot read.
#[must_use]
pub fn smart_section_visible(disk: &DiskMetrics) -> bool {
    has_smart_fields(disk) || disk.smart_availability == SmartAvailability::Available
}

#[cfg(test)]
#[path = "../tests/headless/shell_presentation.rs"]
mod tests;
