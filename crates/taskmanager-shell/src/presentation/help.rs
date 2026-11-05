//! Shared command/page help and icon/status presentation.
//!
//! These resolvers are the single renderer-neutral spelling for shortcuts,
//! terminal glyphs and localized device/failure messages. Moving them out of
//! `presentation.rs` keeps the formatters and this presentation table within
//! the per-file source budget without changing their public addresses.

use taskforest_core::core::device_state::DeviceStatus;
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::metrics::{DiskMetrics, SmartAvailability};
use taskmanager_application::{AppPage, CommandId, KeyCode, Modifiers, default_bindings, i18n};
use taskmanager_ui_contract::{IconId, MessageKey, descriptor, page_descriptors, page_shortcut};

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
        // The session / runtime / lock semantics. The TUI has no bitmap
        // plates, so every semantic id needs a monochrome fallback here; the
        // glyphs are distinct from the ones above.
        IconId::Terminal => "▮",
        IconId::Display => "▭",
        IconId::Docker => "◧",
        IconId::Podman => "◨",
        IconId::Lock => "▩",
        IconId::Remote => "☁",
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
        || disk
            .smart_temperature_sensors_c
            .iter()
            .any(|value| value.is_finite())
        || disk.smart_critical_warning.is_some()
        || disk.smart_temp_critical_c.is_some()
        || disk.smart_percent_used.is_some()
        || disk.smart_available_spare_pct.is_some()
        || disk.smart_available_spare_threshold_pct.is_some()
        || disk.smart_power_on_hours.is_some()
        || disk.smart_unsafe_shutdowns.is_some()
}
