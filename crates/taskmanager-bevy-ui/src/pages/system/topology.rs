//! The SMBIOS motherboard slot and memory-channel topology.
//!
//! The accepted memory inventory names its slots ("ChannelA-DIMM0"), but a
//! list of names is not a board. This module projects the inventory into a
//! slot map and the channels the platform named: one cell per board slot —
//! filled for a populated slot, hollow for a free one — and one row per
//! memory channel with the slots it carries.
//!
//! The map follows the platform's own type-17 order, so a free slot keeps
//! the position the platform enumerated it in. A channel the platform named
//! for a populated module is stated; a free slot has no channel of its own
//! in the accepted inventory, so none is invented for it.

use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, FlexDirection, FlexWrap, Node, Overflow, percent, px, rem,
};
use taskforest_core::core::metrics::SmbiosMemorySnapshot;
use taskmanager_application::i18n::t;

use super::{SystemFactGroup, SystemFactRow, fact_row_scene, section_card_scene};
use crate::palette::{UiPalette, space_4};

/// The marker on one cell of the board's slot map.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MemorySlotCell {
    /// Whether the cell marks a populated slot.
    pub(crate) populated: bool,
}

/// The board's slot map and the channels the inventory named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemoryTopology {
    /// The board's slots, in the platform's type-17 order.
    pub(crate) slots: Vec<MemorySlot>,
    /// The channels the populated modules named, in first-seen order.
    pub(crate) channels: Vec<MemoryChannel>,
}

/// One board slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemorySlot {
    /// The slot's type-17 index.
    pub(crate) index: u32,
    /// The populated module's locator, when the slot carries one.
    pub(crate) locator: Option<String>,
    /// Whether a module populates the slot.
    pub(crate) populated: bool,
}

/// One memory channel and the slots it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemoryChannel {
    /// The channel's own name, as the slot locator spells it ("ChannelA").
    pub(crate) name: String,
    /// The slot names the channel carries ("DIMM0").
    pub(crate) slots: Vec<String>,
}

/// Split a slot locator into its channel and slot names.
///
/// The platform's own locator is the only naming authority: a locator that
/// does not name a channel leaves the module in the board map without a
/// channel rather than guessing one.
fn split_locator(locator: Option<&str>) -> Option<(String, String)> {
    let locator = locator.map(str::trim).filter(|value| !value.is_empty())?;
    let (channel, slot) = locator.split_once('-')?;
    let channel = channel.trim();
    let slot = slot.trim();
    if channel.is_empty() || slot.is_empty() {
        return None;
    }
    Some((channel.to_owned(), slot.to_owned()))
}

/// Project the accepted inventory into the board's slot map and channels.
#[must_use]
pub(crate) fn memory_topology(snapshot: &SmbiosMemorySnapshot) -> MemoryTopology {
    let mut slots = Vec::new();
    for index in 0..snapshot.slots_total.max(snapshot.modules.len() as u32) {
        let module = snapshot.modules.iter().find(|module| module.slot == index);
        slots.push(MemorySlot {
            index,
            locator: module.and_then(|module| module.locator.clone()),
            populated: module.is_some(),
        });
    }
    let mut channels: Vec<MemoryChannel> = Vec::new();
    for module in &snapshot.modules {
        let Some((channel, slot)) = split_locator(module.locator.as_deref()) else {
            continue;
        };
        match channels.iter_mut().find(|entry| entry.name == channel) {
            Some(entry) => entry.slots.push(slot),
            None => channels.push(MemoryChannel {
                name: channel,
                slots: vec![slot],
            }),
        }
    }
    MemoryTopology { slots, channels }
}

/// The board map: one cell per slot, filled for a populated slot.
fn slot_map_scene(topology: &MemoryTopology, palette: &UiPalette) -> Box<dyn Scene> {
    let cells: Vec<Box<dyn Scene>> = topology
        .slots
        .iter()
        .map(|slot| {
            let fill = if slot.populated {
                palette.accent
            } else {
                palette.content_bg
            };
            Box::new(bsn! {
                Node {
                    width: rem(palette.icon_rem),
                    height: rem(palette.icon_rem),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(px(palette.control_radius_px)),
                }
                BackgroundColor({ fill })
                MemorySlotCell { populated: { slot.populated } }
                Children []
            }) as Box<dyn Scene>
        })
        .collect();
    Box::new(bsn! {
        Node {
            width: percent(100),
            min_width: px(0.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(space_4()),
            row_gap: px(space_4()),
            overflow: Overflow::clip(),
        }
        Children [ { cells } ]
    })
}

/// The channel rows: one row per memory channel the platform named.
fn channel_rows(topology: &MemoryTopology, palette: &UiPalette) -> Vec<Box<dyn Scene>> {
    topology
        .channels
        .iter()
        .map(|channel| {
            Box::new(fact_row_scene(
                &SystemFactRow {
                    group: SystemFactGroup::MemoryInventory,
                    label: channel.name.clone(),
                    value: channel.slots.join(" · "),
                },
                palette,
            )) as Box<dyn Scene>
        })
        .collect()
}

/// The topology card of the accepted memory inventory.
///
/// The card is absent when the inventory stated no slot at all: an empty
/// board map would read as a board without slots.
pub(crate) fn topology_card_scene(
    topology: &MemoryTopology,
    width: Option<f32>,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    if topology.slots.is_empty() {
        return None;
    }
    let mut rows = vec![slot_map_scene(topology, palette)];
    rows.extend(channel_rows(topology, palette));
    Some(Box::new(section_card_scene(
        t("system.memory_slots").to_owned(),
        rows,
        width,
        palette,
    )) as Box<dyn Scene>)
}
