//! GPUI builders for the System page's visual composition: hero identity
//! card, static hardware-parameter tiles, and sectioned spec cards (icon header, rows,
//! meters, feature chips). Pure layout over the typed sections from
//! [`super::sections`] — no telemetry access happens here.

use gpui::{Div, ParentElement, Styled, div, px, relative};
use taskmanager_ui_contract::IconId;

use taskmanager_application::i18n;
use taskmanager_theme::{Color, Theme};
use taskmanager_ui::data::key_value_row::KeyValueRow;
use taskmanager_ui::layout::AdaptiveGrid;
use taskmanager_ui::primitives::card_surface::CardSurface;

use super::sections::{SystemMeter, SystemSection, SystemTile};
use taskmanager_theme::tokens;

/// Hero identity card: a large tinted icon block, the device's display name,
/// and a dim hostname · OS · kernel subtitle line. The card replaces the old
/// bare headline as the page's anchor.
pub(super) fn hero_card(theme: &Theme, title: &str, subtitle: &str, badge: Option<&str>) -> Div {
    let mut title_col = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_2)
        .child(
            div()
                .text_size(tokens::FONT_20)
                .font_weight(tokens::FONT_WEIGHT_EXTRA_BOLD.into())
                .text_color(theme.fg)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(subtitle.to_string()),
        );
    if let Some(badge) = badge {
        title_col = title_col.child(
            div()
                .mt(tokens::SPACE_2)
                .px(tokens::SPACE_8)
                .py(tokens::SPACE_2)
                .rounded(tokens::control_radius(theme))
                .bg(theme.sidebar_bg)
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(badge.to_string()),
        );
    }
    CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_12)
        .radius(tokens::control_radius(theme))
        .bordered(false)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_12)
                .child(icon_block(theme, IconId::System, 40.0, theme.accent))
                .child(title_col),
        )
        .render()
}

/// One static hardware-parameter tile: small icon + dim title, the big identity
/// or capacity value, and a dim parameter note. Live telemetry stays on the
/// Performance pages.
pub(super) fn tile_row(theme: &Theme, tiles: &[SystemTile]) -> Div {
    let mut cards = Vec::with_capacity(tiles.len());
    for tile in tiles {
        let tile_content = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(tokens::SPACE_8)
            .min_w(px(0.0))
            .child(icon_block(theme, tile.icon, 28.0, theme.fg_dim))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(tokens::SPACE_1)
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(tokens::FONT_11)
                            .text_color(theme.fg_dim)
                            .child(tile.title.clone()),
                    )
                    // Truncating text lives in a flex-row wrapper (the
                    // title-row pattern): `truncate()` on a bare flex-column
                    // child poisons gpui's nowrap text measure cache and the
                    // value hard-clips mid-glyph at narrow tile widths.
                    .child(
                        div().flex().flex_row().min_w(px(0.0)).child(
                            crate::gpui_app::elements::truncated_text(&tile.value)
                                .text_size(tokens::FONT_16)
                                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                                .text_color(theme.fg),
                        ),
                    )
                    .child(
                        div().flex().flex_row().min_w(px(0.0)).child(
                            crate::gpui_app::elements::truncated_text(&tile.note)
                                .text_size(tokens::FONT_11)
                                .text_color(theme.fg_dim),
                        ),
                    ),
            );
        cards.push(
            CardSurface::new(theme.palette())
                .background(theme.sidebar_card_bg)
                .padding(tokens::SPACE_10)
                .radius(tokens::control_radius(theme))
                .bordered(false)
                .child(tile_content)
                .render()
                .w_full(),
        );
    }
    AdaptiveGrid::new(px(180.0))
        .gap(tokens::SPACE_10)
        .children(cards)
        .render()
}

/// One section card: icon + localized header, spec rows, meters, then chips.
pub(super) fn section_card(theme: &Theme, section: &SystemSection) -> Div {
    let mut content = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(section_header(
            theme,
            section.icon,
            i18n::t(section.title_key),
        ));
    for (label, value) in &section.rows {
        content = content.child(spec_row(theme, label, value));
    }
    // Meter fill follows the section's semantic color (memory capacity vs
    // battery charge), both token-sourced.
    let fill = if section.icon == IconId::Health {
        theme.disk
    } else {
        theme.memory
    };
    for meter in &section.meters {
        content = content.child(meter_bar(theme, meter, fill));
    }
    if !section.chips.is_empty() {
        let mut chip_row = div().flex().flex_row().flex_wrap().gap(tokens::SPACE_6);
        for chip in &section.chips {
            chip_row = chip_row.child(feature_chip(theme, chip));
        }
        content = content.child(chip_row);
    }
    CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_12)
        .radius(tokens::control_radius(theme))
        .bordered(false)
        .child(content)
        .render()
}

/// Section header: a small tinted icon block + bold title, separated from the
/// rows by extra vertical rhythm.
fn section_header(theme: &Theme, icon: IconId, title: &str) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(tokens::SPACE_8)
        .pb(tokens::SPACE_2)
        .child(icon_block(theme, icon, 20.0, theme.accent))
        .child(
            div()
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(theme.fg)
                .child(title.to_string()),
        )
}

/// Icon in a rounded tinted square — the shared 图文并茂 primitive of the page.
fn icon_block(theme: &Theme, icon: IconId, size: f32, tint: Color) -> Div {
    div()
        .flex()
        .size(px(size))
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .rounded(tokens::control_radius(theme))
        .bg(tint.with_alpha(0.12))
        .child(taskmanager_icons::icon(icon).size(px(size * 0.55)))
}

/// One spec row: dim label left, value right.
fn spec_row(theme: &Theme, label: &str, value: &str) -> Div {
    KeyValueRow::new(label, value, theme.palette())
        .selectable_value(gpui::ElementId::Name(
            format!("system-spec-value:{label}").into(),
        ))
        .render()
}

/// One horizontal meter: label + note around a thin rounded track with a
/// clamped fill. pct is 0..=100.
fn meter_bar(theme: &Theme, m: &SystemMeter, fill: Color) -> Div {
    let pct = m
        .pct
        .map(|value| value.clamp(0.0, 100.0) / 100.0)
        .unwrap_or(0.0);
    let mut track = div()
        .h(px(6.0))
        .w_full()
        .rounded(px(3.0))
        .bg(theme.sidebar_bg);
    if m.pct.is_some() {
        track = track.child(div().h_full().w(relative(pct)).rounded(px(3.0)).bg(fill));
    }
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_2)
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .gap(tokens::SPACE_8)
                .child(
                    div()
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(m.label.clone()),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(m.note.clone()),
                ),
        )
        .child(track)
}

/// One static feature chip (instruction set, badges) — never interactive.
fn feature_chip(theme: &Theme, label: &str) -> Div {
    div()
        .px(tokens::SPACE_8)
        .py(tokens::SPACE_2)
        .rounded(tokens::control_radius(theme))
        .bg(theme.sidebar_bg)
        .text_size(tokens::FONT_11)
        .text_color(theme.fg_dim)
        .child(label.to_string())
}
