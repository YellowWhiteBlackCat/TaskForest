//! System / hardware view: a hero identity card, static parameter tiles, and
//! sectioned spec cards (device & OS / processor / memory / storage / graphics)
//! with a copy-to-clipboard affordance.

use gpui::{
    App, ClipboardItem, Div, Entity, InteractiveElement, ParentElement, ScrollHandle,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use std::sync::Arc;

use crate::gpui_app::elements;
use crate::gpui_app::formatting;
use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

pub struct SystemViewData<'a> {
    pub hardware: &'a HardwareInfo,
    pub snapshot: &'a SystemSnapshot,
    /// Latest NPU accelerator inventory (capability `accelerator.npu`). The
    /// section renders only when real devices exist; `None`, an empty list,
    /// and typed failures all leave the page unchanged.
    pub npu_inventory: Option<&'a taskmanager_core::core::npu::NpuInventorySnapshot>,
    /// Shared by refcount: the render rows borrow it and the export pill's
    /// `'static` closure captures a clone of the handle — no per-frame deep
    /// copy of the process table.
    pub processes: Arc<Vec<ProcessItem>>,
    /// Per-window scroll handle for the sectioned spec cards, so the scroll
    /// position never crosses windows.
    pub scroll: &'a ScrollHandle,
    /// Presentation unit preferences captured at render entry; every byte
    /// readout on the page renders through this one value.
    pub units: UnitPreferences,
    /// SMBIOS memory-inventory request session + lane capability feeding the
    /// memory-inventory subsection card beneath the memory section card.
    pub memory_inventory: MemoryInventoryInputs<'a>,
}

/// Format an optional cache capacity. Absence is distinct from an observed zero.
fn fmt_cache_kb(kb: Option<u64>, units: UnitPreferences) -> String {
    kb.map_or_else(formatting::missing_value, |value| {
        units.format_quantity(value * 1024, QuantityFamily::Memory, false)
    })
}

/// Format an optional static clock without using numeric zero as absence.
fn fmt_clock_ghz(mhz: Option<u64>) -> String {
    formatting::optional_ghz(mhz)
}

fn fmt_observed_clock_ghz(mhz: Option<u64>) -> String {
    formatting::optional_ghz(mhz)
}

fn optional_text(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(formatting::missing_value, str::to_string)
}

fn joined_optional_text(first: Option<&str>, second: Option<&str>) -> String {
    let parts = [first, second]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        formatting::missing_value()
    } else {
        parts.join(" ")
    }
}

/// Compose already-structured kernel facts supplied by the native adapter.
fn kernel_display(version: Option<&str>, build_description: Option<&str>) -> String {
    let version = version.map(str::trim).filter(|value| !value.is_empty());
    let build = build_description
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (version, build) {
        (Some(version), Some(build)) => format!("{version} · {build}"),
        (Some(version), None) => version.to_string(),
        (None, Some(build)) => build.to_string(),
        (None, None) => formatting::missing_value(),
    }
}

/// Truncate a boot-args string to ~80 chars with an ellipsis if longer.
/// Delegates to the application layer's shared char-boundary truncation rule
/// (`truncate_text`, the single source for source/detail-line truncation) so
/// the System page and the source lines can never drift apart. Pure (no I/O);
/// unit-tested below.
fn truncate_cmdline(s: &str) -> String {
    const MAX: usize = 80;
    taskmanager_application::truncate_text(s, MAX)
}

mod cards;
mod memory_inventory;
mod sections;
#[cfg(test)]
#[path = "../../tests/gui/gpui_app/system_view/tests.rs"]
mod tests;
use cards::{hero_card, section_card, tile_row};
pub use sections::memory_inventory::MemoryInventoryInputs;
use sections::{build_sections, build_tiles};

/// True when the SMBIOS memory-inventory subsection card renders. The card
/// sits directly beneath the memory section card, so scroll-item arithmetic
/// that targets later cards must count it.
pub(crate) fn memory_inventory_card_is_visible(
    inputs: &MemoryInventoryInputs<'_>,
    units: UnitPreferences,
) -> bool {
    use sections::memory_inventory::{MemoryInventoryModel, memory_inventory_model};
    !matches!(
        memory_inventory_model(inputs, units),
        MemoryInventoryModel::Hidden
    )
}

/// Child index of the Graphics card inside the tracked System scroll column.
/// The hero and parameter tiles occupy the first two slots; omitted sections
/// are folded before this index is derived, matching the actual render order.
/// The memory-inventory subsection card (when visible) adds one child between
/// the memory and storage cards, before Graphics.
pub(super) fn graphics_scroll_item(
    hw: &HardwareInfo,
    snap: &SystemSnapshot,
    npu_inventory: Option<&taskmanager_core::core::npu::NpuInventorySnapshot>,
    smbios: &taskmanager_application::SmbiosMemoryState,
    units: UnitPreferences,
    inventory_card_visible: bool,
) -> Option<usize> {
    const FIXED_LEADING_ITEMS: usize = 2;
    build_sections(hw, snap, npu_inventory, smbios, units)
        .iter()
        .position(|section| section.title_key == "system.section.graphics")
        .map(|index| FIXED_LEADING_ITEMS + index + usize::from(inventory_card_visible))
}

pub fn render_system(theme: &Theme, data: SystemViewData<'_>, entity: Entity<RootView>) -> Div {
    let SystemViewData {
        hardware: hw,
        snapshot: snap,
        npu_inventory,
        processes: procs,
        scroll,
        units,
        memory_inventory,
    } = data;
    // ── Sectioned spec data (pure builders, unit-tested in sections.rs) ──
    let sections = build_sections(hw, snap, npu_inventory, memory_inventory.state, units);
    let tiles = build_tiles(hw, snap, units);

    // Mirror the rendered sections into a plain key:value summary for the
    // clipboard: every section's rows in page order.
    let mut summary = i18n::t("system.title").to_string();
    for section in &sections {
        summary.push_str(&format!("\n[{}]", i18n::t(section.title_key)));
        for (k, v) in &section.rows {
            summary.push_str(&format!("\n{k}: {v}"));
        }
        for meter in &section.meters {
            summary.push_str(&format!("\n{}: {}", meter.label, meter.note));
        }
        if !section.chips.is_empty() {
            summary.push_str(&format!(
                "\n{}: {}",
                i18n::t("system.field.instruction_features"),
                section.chips.join(" \u{b7} ")
            ));
        }
    }

    // Parent: flex_col with [header, scroll_col]. The header stays pinned; the
    // rows scroll beneath it. The parent's gap spaces header from scroll_col.
    let mut col = div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .flex_1()
        .min_h(px(0.0));
    // Header: "System" headline on the left, action pills on the right.
    //
    // `render_system` remains a stateless free fn: the copy action writes directly
    // to gpui's clipboard, while export publishes its outcome through the supplied
    // RootView entity. Hover state is still held false because this view does not
    // currently receive RootView's shared hover slot.
    //
    // Export freezes the rendered facts into a typed request. Serialization
    // and transactional publication happen only on the app-host worker.
    let snap_for_export = snap.clone();
    let entity_for_export = entity.clone();
    let entity_for_diagnostics = entity.clone();
    let entity_for_about = entity.clone();
    let entity_for_app_about = entity.clone();
    let actions = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .min_w(px(0.0))
        .child(elements::pill(
            theme,
            "about-open",
            i18n::t("about.open"),
            false,
            false,
            move |_win, cx| {
                entity_for_app_about.update(cx, |view, cx| {
                    view.show_about();
                    view.hovered = None;
                    cx.notify();
                });
            },
            move |_hov: &bool, _win: &mut Window, _cx: &mut App| {},
        ))
        .child(elements::pill(
            theme,
            "system-about",
            i18n::t("system_about.open"),
            false,
            false,
            move |_win, cx| {
                entity_for_about.update(cx, |view, cx| {
                    view.show_system_about();
                    view.hovered = None;
                    cx.notify();
                });
            },
            move |_hov: &bool, _win: &mut Window, _cx: &mut App| {},
        ))
        .child(elements::pill(
            theme,
            "system-copy",
            i18n::t("common.copy"),
            false,
            false,
            move |_win, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(summary.clone()));
            },
            move |_hov: &bool, _win: &mut Window, _cx: &mut App| {},
        ))
        .child(elements::pill(
            theme,
            "system-export",
            i18n::t("system.export_snapshot"),
            false,
            false,
            move |_win, cx| {
                entity_for_export.update(cx, |view, cx| {
                    view.request_snapshot_export(snap_for_export.clone(), &procs);
                    cx.notify();
                });
            },
            move |_hov: &bool, _win: &mut Window, _cx: &mut App| {},
        ))
        .child(elements::pill(
            theme,
            "system-diagnostics",
            i18n::t("diagnostics.action"),
            false,
            false,
            move |_win, cx| {
                entity_for_diagnostics.update(cx, |view, cx| {
                    view.open_diagnostic_preview();
                    cx.notify();
                });
            },
            move |_hov: &bool, _win: &mut Window, _cx: &mut App| {},
        ));
    col = col.child(
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_8,
            ))
            .pb(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_10,
            ))
            .child(
                div()
                    .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_26))
                    .font_weight(taskmanager_ui::theme_binding::font_weight(
                        tokens::FONT_WEIGHT_EXTRA_BOLD,
                    ))
                    .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                    .child(i18n::t("system.title")),
            )
            .child(actions),
    );
    // Scrollable body. Stateful (id) so overflow_y_scroll is legal; flex_1 +
    // min_h(0) bounds its height so it can scroll within the flex_col parent.
    // Composition: hero identity card → live tiles → sectioned spec cards.
    let mut scroll_col = div()
        .id("system-scroll")
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .flex_1()
        .min_h(px(0.0))
        .pr(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_16,
        ));
    // Hero: product name (hostname/OS fallbacks) + a dim identity subtitle.
    let hero_title = hw
        .product_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| hw.hostname.clone())
        .unwrap_or_else(|| i18n::t("system.title").to_string());
    let os_line = joined_optional_text(hw.os_name.as_deref(), hw.os_version.as_deref());
    let hero_subtitle = [
        hw.hostname
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty()),
        os_line
            .trim()
            .ne(taskmanager_shell::presentation::MISSING_VALUE)
            .then_some(os_line.as_str()),
        hw.kernel_version
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let virt_badge = hw
        .virt
        .as_deref()
        .map(|virt| format!("{}: {virt}", i18n::t("common.virtualization")));
    scroll_col = scroll_col
        .child(hero_card(
            theme,
            &hero_title,
            &hero_subtitle,
            virt_badge.as_deref(),
        ))
        .child(tile_row(theme, &tiles));
    for section in &sections {
        scroll_col = scroll_col.child(section_card(theme, section));
        // The SMBIOS memory-inventory subsection rides directly beneath the
        // memory section card (it renders no element while `Hidden`).
        if section.title_key == "system.section.memory" {
            scroll_col = scroll_col.child(memory_inventory::render_memory_inventory(
                theme,
                &memory_inventory,
                units,
            ));
        }
    }
    let scroll_col = scroll_col.overflow_y_scroll().track_scroll(scroll);
    let scroll_panel = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .child(scroll_col)
        .child(
            taskmanager_ui::primitives::scrollbar::rail::ScrollbarRail::vertical(
                "system-scrollbar",
                "tm-system-scrollbar",
                std::rc::Rc::new(scroll.clone()),
                theme.palette(),
            ),
        );
    col.child(scroll_panel)
}
