//! Settings form row inventory and painted row geometry.
//!
//! The renderer and the pointer hit-test both consume this module: one field
//! inventory, one wrapped-row measurement, and one scroll offset, so a click
//! can never address a row the frame did not paint.

use ratatui::layout::Rect;
use ratatui::text::Line;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::privilege_center::PrivilegeCenterInputs;
use taskmanager_theme::Skin;

use super::footer::settings_footer_plain;
use super::{
    DEVICE_LABEL_KEYS, GRAPH_LABELS, REFRESH_LABELS, SETTINGS_FIELDS, UNIT_FAMILIES,
    density_display_label, font_display_label, language_display_label, mode_display_label,
    unit_label_key,
};
use crate::ui::process_details::wrapped_content_height;
use crate::ui::text::pad_cells;

/// One painted settings row: the stable field index plus the exact label and
/// value text the renderer paints. The frame plan consumes the same inventory
/// to map a pointer cell back to its field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SettingsFieldRow {
    pub(super) field: usize,
    pub(super) label: String,
    pub(super) value: String,
}

/// The settings form's painted field inventory, in field order. Privilege
/// rows keep the same stable indices (`30..`) the keyboard focus walks.
#[must_use]
pub(super) fn settings_field_rows(app: &crate::TuiApp) -> Vec<SettingsFieldRow> {
    let form = &app.settings_form;
    let mut rows = Vec::with_capacity(SETTINGS_FIELDS);
    rows.push(SettingsFieldRow {
        field: 0,
        label: t("settings.skin").to_owned(),
        value: Skin::ALL[form.skin].label().to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 1,
        label: t("settings.mode").to_owned(),
        value: mode_display_label(form.mode).to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 2,
        label: t("settings.high_contrast").to_owned(),
        value: if form.hc {
            t("settings.on").to_owned()
        } else {
            t("settings.off").to_owned()
        },
    });
    rows.push(SettingsFieldRow {
        field: 3,
        label: t("settings.desktop_ui_font").to_owned(),
        value: font_display_label(form.ui_font).to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 4,
        label: t("settings.desktop_mono_font").to_owned(),
        value: font_display_label(form.mono_font).to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 5,
        label: t("settings.row_density").to_owned(),
        value: density_display_label(form.density).to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 6,
        label: t("settings.language").to_owned(),
        value: language_display_label(form.language).to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 7,
        label: t("settings.refresh_interval").to_owned(),
        value: REFRESH_LABELS[form.refresh].to_owned(),
    });
    for (index, key) in DEVICE_LABEL_KEYS.iter().enumerate() {
        rows.push(SettingsFieldRow {
            field: 8 + index,
            label: t(key).to_owned(),
            value: if form.show[index] {
                t("settings.on").to_owned()
            } else {
                t("settings.off").to_owned()
            },
        });
    }
    for family_index in 0..UNIT_FAMILIES.len() {
        let bytes_on = form.units[family_index * 2];
        let base2_on = form.units[family_index * 2 + 1];
        rows.push(SettingsFieldRow {
            field: 18 + family_index * 2,
            label: t(unit_label_key(family_index, true)).to_owned(),
            value: if bytes_on {
                t("settings.bytes").to_owned()
            } else {
                t("settings.bits").to_owned()
            },
        });
        rows.push(SettingsFieldRow {
            field: 19 + family_index * 2,
            label: t(unit_label_key(family_index, false)).to_owned(),
            value: if base2_on {
                t("settings.base_2").to_owned()
            } else {
                t("settings.base_10").to_owned()
            },
        });
    }
    rows.push(SettingsFieldRow {
        field: 24,
        label: t("settings.gray_zero_values").to_owned(),
        value: if form.gray_zero {
            t("settings.on").to_owned()
        } else {
            t("settings.off").to_owned()
        },
    });
    rows.push(SettingsFieldRow {
        field: 25,
        label: t("settings.graph_data_points").to_owned(),
        value: GRAPH_LABELS[form.graph_points].to_owned(),
    });
    rows.push(SettingsFieldRow {
        field: 26,
        label: t("settings.desktop_notifications").to_owned(),
        value: if form.notify_enabled {
            t("settings.on").to_owned()
        } else {
            t("settings.off").to_owned()
        },
    });
    rows.push(SettingsFieldRow {
        field: 27,
        label: t("settings.quiet_hours_start").to_owned(),
        value: format!("{:02}:00", form.quiet_start),
    });
    rows.push(SettingsFieldRow {
        field: 28,
        label: t("settings.quiet_hours_end").to_owned(),
        value: format!("{:02}:00", form.quiet_end),
    });
    rows.push(SettingsFieldRow {
        field: 29,
        label: t("settings.history_persistence").to_owned(),
        value: if form.history_persistence {
            t("settings.on").to_owned()
        } else {
            t("settings.off").to_owned()
        },
    });
    for (index, privilege) in PrivilegeCenterInputs::from_shell(&app.shell)
        .rows()
        .into_iter()
        .enumerate()
    {
        let value = if privilege.action.is_some() {
            format!(
                "{} · [Enter] {}",
                t(privilege.state.label_key()),
                t("settings.privileges_authorize")
            )
        } else {
            t(privilege.state.label_key()).to_owned()
        };
        rows.push(SettingsFieldRow {
            field: 30 + index,
            label: t(privilege.label_key).to_owned(),
            value,
        });
    }
    rows
}

/// The exact painted text of one form row (focus marker included so the
/// wrapped height always matches the renderer's).
fn field_row_text(label: &str, value: &str) -> String {
    format!("  {}{}", pad_cells(label, 16), value)
}

/// The settings overlay's painted body geometry: the scroll offset and every
/// field's absolute row span. The renderer and the pointer hit-test both
/// consume this one value, so a click can never address a row the frame did
/// not paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SettingsOverlayLayout {
    pub(crate) scroll: u16,
    /// The scrollable form body (the popup inner area above the footer).
    /// Only cells inside it can address a field.
    pub(crate) body: Rect,
    spans: [Option<(u16, u16)>; SETTINGS_FIELDS],
}

impl SettingsOverlayLayout {
    /// The field whose painted rows contain `row`, if any.
    #[must_use]
    pub(crate) fn field_at(&self, row: u16) -> Option<usize> {
        self.spans
            .iter()
            .position(|span| span.is_some_and(|(start, end)| start <= row && row < end))
    }
}

/// Resolve the settings form's painted rows for the frame plan.
#[must_use]
pub(crate) fn settings_overlay_layout(
    app: &crate::TuiApp,
    popup: Rect,
    focused_field: Option<usize>,
) -> SettingsOverlayLayout {
    let inner = Rect {
        x: popup.x.saturating_add(1),
        y: popup.y.saturating_add(1),
        width: popup.width.saturating_sub(2),
        height: popup.height.saturating_sub(2),
    };
    let footer_height = settings_footer_plain(
        &app.settings_form,
        focused_field,
        inner,
        app.first_run.view().info.is_some(),
    )
    .height;
    let body = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let mut spans = [None; SETTINGS_FIELDS];
    if inner.width == 0 || body.height == 0 {
        return SettingsOverlayLayout {
            scroll: 0,
            body,
            spans,
        };
    }
    let rows = settings_field_rows(app);
    let mut field_line_top = [None; SETTINGS_FIELDS];
    let mut field_line_height = [0_usize; SETTINGS_FIELDS];
    let mut cursor = 0_usize;
    let mut paint_row = |height: usize| {
        let top = cursor;
        cursor = cursor.saturating_add(height);
        top
    };
    for row in rows.iter().filter(|row| row.field < 30) {
        let line = Line::from(field_row_text(&row.label, &row.value));
        let height = wrapped_content_height(std::slice::from_ref(&line), inner.width);
        field_line_top[row.field] = Some(paint_row(height));
        field_line_height[row.field] = height;
    }
    // The renderer always separates the form fields from the permission
    // group with one blank row and the group header.
    paint_row(1);
    let header = Line::from(format!("── {} ──", t("settings.privileges")));
    paint_row(wrapped_content_height(
        std::slice::from_ref(&header),
        inner.width,
    ));
    for row in rows.iter().filter(|row| row.field >= 30) {
        let line = Line::from(field_row_text(&row.label, &row.value));
        let height = wrapped_content_height(std::slice::from_ref(&line), inner.width);
        field_line_top[row.field] = Some(paint_row(height));
        field_line_height[row.field] = height;
    }
    let visible_rows = usize::from(body.height.saturating_sub(1));
    let scroll = focused_field
        .and_then(|field| {
            field_line_top
                .get(field)
                .copied()
                .flatten()
                .map(|top| top.saturating_add(field_line_height[field]))
        })
        .unwrap_or(0)
        .saturating_sub(visible_rows);
    let body_top = usize::from(inner.y);
    for field in 0..SETTINGS_FIELDS {
        let Some(top) = field_line_top[field] else {
            continue;
        };
        let bottom = top.saturating_add(field_line_height[field]);
        let visible_start = top.max(scroll);
        let visible_end = bottom.min(scroll.saturating_add(usize::from(body.height)));
        if visible_start < visible_end {
            let start = u16::try_from(body_top + (visible_start - scroll)).unwrap_or(u16::MAX);
            let end = u16::try_from(body_top + (visible_end - scroll)).unwrap_or(u16::MAX);
            spans[field] = Some((start, end));
        }
    }
    SettingsOverlayLayout {
        scroll: u16::try_from(scroll).unwrap_or(u16::MAX),
        body,
        spans,
    }
}
