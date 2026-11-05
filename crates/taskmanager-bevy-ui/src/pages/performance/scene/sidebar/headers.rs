//! Header scenes for each performance device target.

use crate::pages::performance::metrics::cpu::cpu_field_text;
use crate::pages::performance::metrics::{
    battery_header_view_model, battery_sidebar_caption, disk_header_view_model,
    disk_sidebar_caption, fan_header_view_model, fan_sidebar_caption, gpu_header_view_model,
    gpu_sidebar_caption, memory_header_view_model, memory_sidebar_caption,
    network_header_view_model, network_sidebar_caption,
};
use crate::pages::performance::{
    CpuField, DynField, DynText, PerformanceDeviceTarget, PerformanceHeaderTarget,
};
use crate::palette::{space_2, space_8};
use crate::widgets::bounded_text::BoundedText;
use crate::window::{Role, TextRole};
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::text::{LineBreak, TextLayout};
use bevy::ui::prelude::*;
use bevy::ui::widget::Text;
use taskforest_core::core::metrics::{DiskMetrics, GpuMetrics, NetworkMetrics};
use taskforest_core::core::power::BatteryInfo;
use taskforest_core::core::sensors::SensorReading;
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;

pub(super) fn cpu_caption_scene(shell: &ShellApp) -> Box<dyn Scene> {
    let (brand, line2) = crate::pages::performance::metrics::cpu_sidebar_caption(shell);
    caption_pair_scene(brand, line2, PerformanceDeviceTarget::Cpu)
}

fn caption_pair_scene(
    cap1: String,
    cap2: String,
    target: PerformanceDeviceTarget,
) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            // The caption lines are bounded slots: each line is measured to
            // its own content, so the ellipsis system can see an overflow.
            // A stretched line would report the column's width and never
            // look too wide.
            align_items: AlignItems::FlexStart,
            overflow: Overflow::clip_x(),
        }
        Children [
            Text(cap1)
            TextRole(Role::Caption)
            TextLayout { linebreak: LineBreak::NoWrap }
            BoundedText
            DynText(DynField::SidebarCaption1({ target.clone() })) --
            Text(cap2)
            TextRole(Role::Caption)
            TextLayout { linebreak: LineBreak::NoWrap }
            BoundedText
            DynText(DynField::SidebarCaption2({ target }))
        ]
    })
}

pub(super) fn device_disk_caption_scene(disk: &DiskMetrics) -> Box<dyn Scene> {
    let (cap1, cap2) = disk_sidebar_caption(disk);
    caption_pair_scene(
        cap1,
        cap2,
        PerformanceDeviceTarget::Disk(disk.device_id.clone()),
    )
}

pub(super) fn device_network_caption_scene(nic: &NetworkMetrics) -> Box<dyn Scene> {
    let (cap1, cap2) = network_sidebar_caption(nic);
    caption_pair_scene(
        cap1,
        cap2,
        PerformanceDeviceTarget::Network(nic.device_id.to_string()),
    )
}

pub(super) fn device_gpu_caption_scene(gpu: &GpuMetrics) -> Box<dyn Scene> {
    let (cap1, cap2) = gpu_sidebar_caption(gpu);
    caption_pair_scene(
        cap1,
        cap2,
        PerformanceDeviceTarget::Gpu(gpu.device_id.clone()),
    )
}

pub(super) fn device_battery_caption_scene(battery: &BatteryInfo) -> Box<dyn Scene> {
    let (cap1, cap2) = battery_sidebar_caption(battery);
    caption_pair_scene(
        cap1,
        cap2,
        PerformanceDeviceTarget::Battery(battery.id.clone()),
    )
}

pub(super) fn device_fan_caption_scene(fan: &SensorReading) -> Box<dyn Scene> {
    let (cap1, cap2) = fan_sidebar_caption(fan);
    caption_pair_scene(
        cap1,
        cap2,
        PerformanceDeviceTarget::Fan(fan.id().to_owned()),
    )
}

pub(super) fn cpu_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Text(t("common.cpu")) TextRole(Role::Heading) --
                Node { flex_grow: 1.0 } --
                Text(cpu_field_text(shell, CpuField::Brand))
                TextRole(Role::Body)
                DynText(DynField::Cpu(CpuField::Brand))
            ] --
            Text(t("cpu.utilization_over_60s")) TextRole(Role::Caption)
        ]
    }
}

pub(super) fn memory_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    let model = memory_header_view_model(shell);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(space_8()),
        }
        Children [
            Text(t("common.memory")) TextRole(Role::Heading) --
            Node { flex_grow: 1.0 } --
            Text({model.vital}) TextRole(Role::Body)
        ]
    }
}

/// Two-line memory caption for the sidebar device row: "used / total" and
/// "pct", matching GPUI's two-caption layout so wide fractions never wrap or
/// clip unexplained characters.
pub(super) fn memory_device_caption_scene(shell: &ShellApp) -> Box<dyn Scene> {
    let (cap1, cap2) = memory_sidebar_caption(shell);
    caption_pair_scene(cap1, cap2, PerformanceDeviceTarget::Memory)
}

pub(super) fn disk_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    let model = disk_header_view_model(shell);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Text({model.title}) TextRole(Role::Heading) --
                Node { flex_grow: 1.0 } --
                Text({model.subtitle}) TextRole(Role::Body)
            ] --
            Text({model.vital}) TextRole(Role::Caption)
        ]
    }
}

pub(super) fn network_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    let model = network_header_view_model(shell);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Text({model.title}) TextRole(Role::Heading) --
                Node { flex_grow: 1.0 } --
                Text({model.subtitle}) TextRole(Role::Body)
            ] --
            Text({model.vital}) TextRole(Role::Caption)
        ]
    }
}

pub(super) fn gpu_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    let model = gpu_header_view_model(shell);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Text({model.title}) TextRole(Role::Heading) --
                Node { flex_grow: 1.0 } --
                Text({model.subtitle}) TextRole(Role::Body)
            ] --
            Text({model.vital}) TextRole(Role::Caption)
        ]
    }
}

pub(super) fn battery_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    let model = battery_header_view_model(shell);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Text({model.title}) TextRole(Role::Heading) --
                Node { flex_grow: 1.0 } --
                Text({model.subtitle}) TextRole(Role::Body)
            ] --
            Text({model.vital}) TextRole(Role::Caption)
        ]
    }
}

pub(super) fn fan_header_scene(shell: &ShellApp) -> impl Scene + use<> {
    let model = fan_header_view_model(shell);
    bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
        }
        Children [
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(space_8()),
            }
            Children [
                Text({model.title}) TextRole(Role::Heading) --
                Node { flex_grow: 1.0 } --
                Text({model.subtitle}) TextRole(Role::Body)
            ] --
            Text("") TextRole(Role::Caption)
        ]
    }
}

pub(super) fn all_device_headers_scene(shell: &ShellApp) -> Vec<Box<dyn Scene>> {
    vec![
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::Flex,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Cpu)
            Children [
                @{ cpu_header_scene(shell) }
            ]
        }),
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::None,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Memory)
            Children [
                @{ memory_header_scene(shell) }
            ]
        }),
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::None,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Disk(String::new()))
            Children [
                @{ disk_header_scene(shell) }
            ]
        }),
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::None,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Network(String::new()))
            Children [
                @{ network_header_scene(shell) }
            ]
        }),
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::None,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Gpu(String::new()))
            Children [
                @{ gpu_header_scene(shell) }
            ]
        }),
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::None,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Battery(String::new()))
            Children [
                @{ battery_header_scene(shell) }
            ]
        }),
        Box::new(bsn! {
            Node {
                width: percent(100),
                display: Display::None,
            }
            PerformanceHeaderTarget(PerformanceDeviceTarget::Fan(String::new()))
            Children [
                @{ fan_header_scene(shell) }
            ]
        }),
    ]
}
