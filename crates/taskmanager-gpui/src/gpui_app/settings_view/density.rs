//! Row-density chooser row of the Settings modal (Comfortable / Compact).

use gpui::{Context, Div, Entity, ParentElement, Styled, div};

use crate::gpui_app::elements::pill;
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens;
use crate::gpui_app::theme::tokens::RowDensity;
use crate::i18n;

/// Row-density chooser: two pills (Comfortable / Compact) writing
/// `RootView.density` directly — the same pill pattern as the skin/mode rows.
/// Comfortable is the app's standard table geometry (also the cold-start
/// default); Compact tightens the vertical padding + line-height so the same
/// data fits more rows per viewport. Hover overlay mirrors the other pills.
pub(super) fn density_row(
    t: &Theme,
    ent: Entity<RootView>,
    density: RowDensity,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(tokens::SPACE_6)
        .child(pill(
            t,
            "density-comfortable",
            i18n::t("settings.density_comfortable"),
            density == RowDensity::Comfortable,
            hovered == Some(&Hover::Static("density-comfortable")),
            {
                let ent = ent.clone();
                move |_win, cx| {
                    ent.update(cx, |v, cx| {
                        v.set_density(RowDensity::Comfortable, cx);
                    });
                }
            },
            cx.listener(move |v, is_hov: &bool, _win, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static("density-comfortable"))
                    } else {
                        None
                    },
                    cx,
                );
            }),
        ))
        .child(pill(
            t,
            "density-compact",
            i18n::t("settings.density_compact"),
            density == RowDensity::Compact,
            hovered == Some(&Hover::Static("density-compact")),
            {
                let ent = ent.clone();
                move |_win, cx| {
                    ent.update(cx, |v, cx| {
                        v.set_density(RowDensity::Compact, cx);
                    });
                }
            },
            cx.listener(move |v, is_hov: &bool, _win, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static("density-compact"))
                    } else {
                        None
                    },
                    cx,
                );
            }),
        ))
}
