//! Open-files sub-section of the Process Properties insights dialog.
//!
//! Renders the per-process file-descriptor list collected from
//! `/proc/<pid>/fd` (see
//! `taskmanager-platform-linux::engine::process::telemetry::open_files`). Mirrors
//! the gpu_engines card: a typed collection state is shown verbatim — an
//! `EACCES` on a foreign-uid process's fd directory renders as the typed
//! "Permission denied" status, never a silent omission — a healthy process with
//! no readable descriptors is an explicit "no open files" message, and a
//! populated list renders a scrollable mono-font list. A descriptor whose
//! readlink failed keeps its row with a typed "unreadable" marker rather than
//! being dropped or fabricated.
//!
//! The card's copy comes from [`ProcessInsightsLabels`] (supplied by the
//! Properties caller), matching the open-files label slots the chrome already
//! threads through.

use gpui::{Div, ParentElement, Styled, div, px};
use taskmanager_core::core::process_telemetry::{OpenFileEntry, ProcessTelemetrySnapshot};

use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use super::ProcessInsightsLabels;
use crate::gpui_app::theme::mono_font_with_fallback;

/// Render one descriptor as `fd  target`, with the target falling back to the
/// typed "unreadable" marker when procfs could not resolve the symlink.
fn format_open_file(entry: &OpenFileEntry, unreadable: &str) -> String {
    let target = entry
        .target
        .clone()
        .unwrap_or_else(|| unreadable.to_string());
    format!("{}  {}", entry.fd, target)
}

/// The open-files card. Surfaces the per-process descriptor list or an explicit
/// typed message when the source is unavailable, denied, or empty.
pub(in crate::gpui_app::process_insights::view) fn open_files_card(
    theme: &Theme,
    snapshot: &ProcessTelemetrySnapshot,
    labels: &ProcessInsightsLabels,
    width: f32,
) -> Div {
    let open_files = &snapshot.open_files;
    if open_files.state.status != DeviceStatus::Healthy {
        return super::card(theme, labels.open_files, width).child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(super::status_label(open_files.state.status, labels).to_string()),
        );
    }
    let mut content = super::card(theme, labels.open_files, width);
    if open_files.entries.is_empty() {
        return content.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(labels.no_open_files.to_string()),
        );
    }
    let header = if open_files.unreadable_count > 0 {
        format!(
            "{} · {} · {} {}",
            labels.open_files,
            open_files.entries.len(),
            open_files.unreadable_count,
            labels.unreadable,
        )
    } else {
        format!("{} · {}", labels.open_files, open_files.entries.len())
    };
    content = content.child(
        div()
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
            .child(header),
    );
    let (shown, hidden) = super::capped_card_rows(open_files.entries.len());
    content = content.child(
        div()
            .flex()
            .flex_col()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_3,
            ))
            .children(
                open_files
                    .entries
                    .iter()
                    .take(shown)
                    .map(|entry| format_open_file(entry, labels.unreadable))
                    .map(|line| {
                        div()
                            .min_w(px(0.0))
                            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_10))
                            .font(mono_font_with_fallback(theme))
                            .whitespace_normal()
                            .child(line)
                    }),
            ),
    );
    if hidden > 0 {
        content = content.child(crate::gpui_app::elements::more_rows_hint(theme, hidden));
    }
    content
}

#[cfg(test)]
#[path = "../../../../tests/gui/gpui_gpui_app_process_insights_view_open_files_tests.rs"]
mod tests;
