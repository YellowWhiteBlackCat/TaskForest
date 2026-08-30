//! Mission Center-compatible unit/base choices for Performance readouts.

use gpui::{Context, Div, Entity, IntoElement, ParentElement, Styled, div};

use crate::gpui_app::elements::pill;
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

#[derive(Clone, Copy)]
enum UnitChoice {
    MemoryBytes,
    MemoryBits,
    MemoryBase2,
    MemoryBase10,
    DriveBytes,
    DriveBits,
    DriveBase2,
    DriveBase10,
    NetworkBytes,
    NetworkBits,
    NetworkBase2,
    NetworkBase10,
}

#[derive(Clone, Copy)]
struct UnitPillSpec {
    id: &'static str,
    label: &'static str,
    active: bool,
    choice: UnitChoice,
}

#[derive(Clone, Copy)]
struct UnitOptionSpec {
    id: &'static str,
    label_key: &'static str,
    choice: UnitChoice,
}

#[derive(Clone, Copy)]
struct UnitRowSpec {
    title_key: &'static str,
    first: UnitOptionSpec,
    second: UnitOptionSpec,
}

pub(super) fn units_group(
    t: &Theme,
    ent: Entity<RootView>,
    units: UnitPreferences,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_12)
        .child(unit_row(
            t,
            ent.clone(),
            UnitRowSpec {
                title_key: "settings.memory_usage_unit",
                first: UnitOptionSpec {
                    id: "memory-unit-bytes",
                    label_key: "settings.bytes",
                    choice: UnitChoice::MemoryBytes,
                },
                second: UnitOptionSpec {
                    id: "memory-unit-bits",
                    label_key: "settings.bits",
                    choice: UnitChoice::MemoryBits,
                },
            },
            units.memory_use_bytes,
            hovered,
            cx,
        ))
        .child(unit_row(
            t,
            ent.clone(),
            UnitRowSpec {
                title_key: "settings.memory_usage_base",
                first: UnitOptionSpec {
                    id: "memory-base-2",
                    label_key: "settings.base_2",
                    choice: UnitChoice::MemoryBase2,
                },
                second: UnitOptionSpec {
                    id: "memory-base-10",
                    label_key: "settings.base_10",
                    choice: UnitChoice::MemoryBase10,
                },
            },
            units.memory_use_base2,
            hovered,
            cx,
        ))
        .child(unit_row(
            t,
            ent.clone(),
            UnitRowSpec {
                title_key: "settings.drive_usage_unit",
                first: UnitOptionSpec {
                    id: "drive-unit-bytes",
                    label_key: "settings.bytes",
                    choice: UnitChoice::DriveBytes,
                },
                second: UnitOptionSpec {
                    id: "drive-unit-bits",
                    label_key: "settings.bits",
                    choice: UnitChoice::DriveBits,
                },
            },
            units.drive_use_bytes,
            hovered,
            cx,
        ))
        .child(unit_row(
            t,
            ent.clone(),
            UnitRowSpec {
                title_key: "settings.drive_usage_base",
                first: UnitOptionSpec {
                    id: "drive-base-2",
                    label_key: "settings.base_2",
                    choice: UnitChoice::DriveBase2,
                },
                second: UnitOptionSpec {
                    id: "drive-base-10",
                    label_key: "settings.base_10",
                    choice: UnitChoice::DriveBase10,
                },
            },
            units.drive_use_base2,
            hovered,
            cx,
        ))
        .child(unit_row(
            t,
            ent.clone(),
            UnitRowSpec {
                title_key: "settings.network_usage_unit",
                first: UnitOptionSpec {
                    id: "network-unit-bytes",
                    label_key: "settings.bytes",
                    choice: UnitChoice::NetworkBytes,
                },
                second: UnitOptionSpec {
                    id: "network-unit-bits",
                    label_key: "settings.bits",
                    choice: UnitChoice::NetworkBits,
                },
            },
            units.network_use_bytes,
            hovered,
            cx,
        ))
        .child(unit_row(
            t,
            ent,
            UnitRowSpec {
                title_key: "settings.network_usage_base",
                first: UnitOptionSpec {
                    id: "network-base-2",
                    label_key: "settings.base_2",
                    choice: UnitChoice::NetworkBase2,
                },
                second: UnitOptionSpec {
                    id: "network-base-10",
                    label_key: "settings.base_10",
                    choice: UnitChoice::NetworkBase10,
                },
            },
            units.network_use_base2,
            hovered,
            cx,
        ))
}

fn unit_row(
    t: &Theme,
    ent: Entity<RootView>,
    spec: UnitRowSpec,
    first_active: bool,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .flex_1()
                .min_w(gpui::px(0.0))
                .text_size(tokens::FONT_13)
                .text_color(t.fg)
                .child(i18n::t(spec.title_key)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(tokens::SPACE_6)
                .child(unit_pill(
                    t,
                    ent.clone(),
                    UnitPillSpec {
                        id: spec.first.id,
                        label: i18n::t(spec.first.label_key),
                        active: first_active,
                        choice: spec.first.choice,
                    },
                    hovered,
                    cx,
                ))
                .child(unit_pill(
                    t,
                    ent,
                    UnitPillSpec {
                        id: spec.second.id,
                        label: i18n::t(spec.second.label_key),
                        active: !first_active,
                        choice: spec.second.choice,
                    },
                    hovered,
                    cx,
                )),
        )
}

fn unit_pill(
    t: &Theme,
    ent: Entity<RootView>,
    spec: UnitPillSpec,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    pill(
        t,
        spec.id,
        spec.label,
        spec.active,
        hovered == Some(&Hover::Static(spec.id)),
        move |_win, cx| {
            ent.update(cx, |view, cx| match spec.choice {
                UnitChoice::MemoryBytes => view.set_memory_use_bytes(true, cx),
                UnitChoice::MemoryBits => view.set_memory_use_bytes(false, cx),
                UnitChoice::MemoryBase2 => view.set_memory_use_base2(true, cx),
                UnitChoice::MemoryBase10 => view.set_memory_use_base2(false, cx),
                UnitChoice::DriveBytes => view.set_drive_use_bytes(true, cx),
                UnitChoice::DriveBits => view.set_drive_use_bytes(false, cx),
                UnitChoice::DriveBase2 => view.set_drive_use_base2(true, cx),
                UnitChoice::DriveBase10 => view.set_drive_use_base2(false, cx),
                UnitChoice::NetworkBytes => view.set_network_use_bytes(true, cx),
                UnitChoice::NetworkBits => view.set_network_use_bytes(false, cx),
                UnitChoice::NetworkBase2 => view.set_network_use_base2(true, cx),
                UnitChoice::NetworkBase10 => view.set_network_use_base2(false, cx),
            });
        },
        cx.listener(move |view, is_hovered: &bool, _win, cx| {
            view.set_hover(
                if *is_hovered {
                    Some(Hover::Static(spec.id))
                } else {
                    None
                },
                cx,
            );
        }),
    )
}
