//! The typed hardware topology graph: the board, its CPU sockets, the
//! SMBIOS memory channels, and the PCIe-attached devices, folded into one
//! node-and-edge tree.
//!
//! Pure data; the OS is never touched here. Every node is the platform's
//! own word: a socket count the topology provider did not supply is one
//! explicit unknown node, a locator that does not name a channel keeps its
//! raw text as an unassigned channel, and a GPU without a stated slot stays
//! an unknown PCI device. A lane that has not answered contributes no nodes
//! at all — a waiting inventory is the honest waiting state, never a
//! fabricated zero.
//!
//! The card renders the board, socket, and PCIe rows beside the existing
//! memory topology card; the channel nodes ride in the graph so the tree is
//! complete, but their rows stay in the slot-and-channel card that already
//! owns them.

use bevy::scene::Scene;
use taskforest_core::core::hardware::HardwareInfo;
use taskforest_core::core::metrics::{GpuMetrics, SmbiosMemorySnapshot, SystemSnapshot};
use taskforest_core::core::storage::StorageInterconnect;
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::missing_value;

use super::topology::split_locator;
use super::{SystemFactGroup, SystemFactRow, fact_row_scene, section_card_scene};
use crate::palette::UiPalette;

/// The one root node every other node descends from.
const BOARD_ID: &str = "board";

/// What kind of hardware fact one node states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HardwareTopologyKind {
    /// The motherboard or baseboard itself.
    Board,
    /// One CPU socket (physical package).
    Socket,
    /// One SMBIOS memory channel and the slots it carries.
    MemoryChannel,
    /// One device attached over PCIe.
    PcieDevice,
}

/// Whether the owning lane stated the node's fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HardwareTopologyPresence {
    /// The platform stated the fact.
    Known,
    /// The lane answered, but this fact was absent.
    Unknown,
}

/// One node of the hardware topology tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HardwareTopologyNode {
    /// Stable, unique id within the graph (`board`, `socket:0`,
    /// `channel:ChannelA`, `pci:0000:00:02.0`).
    pub(crate) id: String,
    /// The kind of hardware fact the node states.
    pub(crate) kind: HardwareTopologyKind,
    /// Display label of the node.
    pub(crate) label: String,
    /// The parent node's id; `None` only for the board root.
    pub(crate) parent: Option<String>,
    /// The node's primary detail fact, when the platform stated one.
    pub(crate) detail: Option<String>,
    /// Whether the node's fact is known or explicitly unknown.
    pub(crate) presence: HardwareTopologyPresence,
}

/// The folded hardware topology: one board with its sockets, channels, and
/// PCIe devices as a parents-before-children node list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HardwareTopologyGraph {
    /// All nodes, parents before children.
    pub(crate) nodes: Vec<HardwareTopologyNode>,
}

impl HardwareTopologyGraph {
    /// Whether the graph states at least one known fact beyond the board.
    ///
    /// A board whose every child is unknown stays unrendered: the hardware
    /// fact card already carries those facts, and an all-dash card would
    /// read as content.
    #[must_use]
    pub(crate) fn states_known_facts(&self) -> bool {
        self.nodes
            .iter()
            .skip(1)
            .any(|node| node.presence == HardwareTopologyPresence::Known)
    }

    fn push(&mut self, node: HardwareTopologyNode) {
        self.nodes.push(node);
    }
}

/// Fold the topology from the lanes that have answered.
///
/// Every node keeps its platform-stated fact; a fact the lane answered
/// without becomes an explicit unknown node, never a guess.
#[must_use]
pub(crate) fn hardware_topology_graph(
    hardware: Option<&HardwareInfo>,
    smbios: Option<&SmbiosMemorySnapshot>,
    telemetry: Option<&SystemSnapshot>,
) -> HardwareTopologyGraph {
    let mut graph = HardwareTopologyGraph::default();
    let arrived = hardware.is_some() || smbios.is_some() || telemetry.is_some();
    let presence = if arrived {
        HardwareTopologyPresence::Known
    } else {
        HardwareTopologyPresence::Unknown
    };
    graph.push(HardwareTopologyNode {
        id: BOARD_ID.to_owned(),
        kind: HardwareTopologyKind::Board,
        label: t("common.hardware").to_owned(),
        parent: None,
        detail: hardware.and_then(|hardware| hardware.motherboard_model.clone()),
        presence,
    });
    fold_sockets(&mut graph, hardware);
    fold_channels(&mut graph, smbios);
    fold_pcie_devices(&mut graph, telemetry);
    graph
}

/// Fold the CPU sockets. A stated count becomes one node per socket, with
/// the CPU brand on the first; a count the topology provider did not supply
/// becomes one explicit unknown node.
fn fold_sockets(graph: &mut HardwareTopologyGraph, hardware: Option<&HardwareInfo>) {
    let Some(hardware) = hardware else {
        return;
    };
    let socket_label = t("common.socket").to_owned();
    match hardware.sockets {
        Some(count) if count > 0 => {
            for index in 0..count {
                let detail = (index == 0)
                    .then(|| hardware.cpu_brand.clone())
                    .flatten()
                    .filter(|brand| !brand.trim().is_empty());
                graph.push(HardwareTopologyNode {
                    id: format!("socket:{index}"),
                    kind: HardwareTopologyKind::Socket,
                    label: format!("{socket_label} {index}"),
                    parent: Some(BOARD_ID.to_owned()),
                    detail,
                    presence: HardwareTopologyPresence::Known,
                });
            }
        }
        _ => {
            graph.push(HardwareTopologyNode {
                id: "socket:unknown".to_owned(),
                kind: HardwareTopologyKind::Socket,
                label: socket_label,
                parent: Some(BOARD_ID.to_owned()),
                detail: None,
                presence: HardwareTopologyPresence::Unknown,
            });
        }
    }
}

/// Fold the memory channels the SMBIOS locators named. A locator that does
/// not name a channel keeps its raw text as an unassigned channel, exactly
/// like the board's slot map keeps the module.
fn fold_channels(graph: &mut HardwareTopologyGraph, smbios: Option<&SmbiosMemorySnapshot>) {
    let Some(smbios) = smbios else {
        return;
    };
    let mut channels: Vec<(String, Vec<String>)> = Vec::new();
    let mut unassigned: Vec<String> = Vec::new();
    for module in &smbios.modules {
        match split_locator(module.locator.as_deref()) {
            Some((channel, slot)) => match channels.iter_mut().find(|(name, _)| name == &channel) {
                Some((_, slots)) => slots.push(slot),
                None => channels.push((channel, vec![slot])),
            },
            None => {
                let stated = module
                    .locator
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{} {}", t("system.slot"), module.slot));
                unassigned.push(stated);
            }
        }
    }
    let channel_label = t("net.channel").to_owned();
    for (name, slots) in channels {
        graph.push(HardwareTopologyNode {
            id: format!("channel:{name}"),
            kind: HardwareTopologyKind::MemoryChannel,
            label: name,
            parent: Some(BOARD_ID.to_owned()),
            detail: Some(slots.join(" · ")),
            presence: HardwareTopologyPresence::Known,
        });
    }
    if !unassigned.is_empty() {
        graph.push(HardwareTopologyNode {
            id: "channel:unknown".to_owned(),
            kind: HardwareTopologyKind::MemoryChannel,
            label: channel_label,
            parent: Some(BOARD_ID.to_owned()),
            detail: Some(unassigned.join(" · ")),
            presence: HardwareTopologyPresence::Unknown,
        });
    }
}

/// Fold the PCIe-attached devices: every GPU (a stated slot is its own node,
/// a missing slot an explicit unknown one) and every disk whose outer
/// interconnect is PCIe, tunneled or native.
fn fold_pcie_devices(graph: &mut HardwareTopologyGraph, telemetry: Option<&SystemSnapshot>) {
    let Some(telemetry) = telemetry else {
        return;
    };
    for gpu in &telemetry.gpu {
        let (id, detail, presence) = match gpu.pci_slot.as_deref() {
            Some(slot) => (
                format!("pci:{slot}"),
                Some(slot.to_owned()),
                HardwareTopologyPresence::Known,
            ),
            None => (
                format!("gpu:{}", gpu.device_id),
                None,
                HardwareTopologyPresence::Unknown,
            ),
        };
        graph.push(HardwareTopologyNode {
            id,
            kind: HardwareTopologyKind::PcieDevice,
            label: gpu_label(gpu),
            parent: Some(BOARD_ID.to_owned()),
            detail,
            presence,
        });
    }
    for disk in &telemetry.disks {
        if !matches!(
            disk.connection().interconnect,
            StorageInterconnect::Pcie | StorageInterconnect::PcieTunnel
        ) {
            continue;
        }
        let detail = (!disk.model.trim().is_empty()).then(|| disk.model.clone());
        graph.push(HardwareTopologyNode {
            id: format!("disk:{}", disk.device_id),
            kind: HardwareTopologyKind::PcieDevice,
            label: disk.name.clone(),
            parent: Some(BOARD_ID.to_owned()),
            detail,
            presence: HardwareTopologyPresence::Known,
        });
    }
}

/// The GPU's stated name: the marketing name, else the brand, else the
/// section noun. An unstated name is never invented.
fn gpu_label(gpu: &GpuMetrics) -> String {
    gpu.marketing_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| Some(gpu.brand.clone()).filter(|brand| !brand.trim().is_empty()))
        .unwrap_or_else(|| t("common.gpu").to_owned())
}

/// The board/socket/PCIe readout card of the folded topology.
///
/// `None` when the graph states no known fact beyond the board: an
/// all-unknown fold stays out of the grid instead of reading as content.
/// The channel rows are not repeated here — the memory topology card owns
/// them — so the card states exactly the facts no other card states.
#[must_use]
pub(crate) fn topology_graph_card_scene(
    graph: &HardwareTopologyGraph,
    width: Option<f32>,
    palette: &UiPalette,
) -> Option<Box<dyn Scene>> {
    if !graph.states_known_facts() {
        return None;
    }
    let rows: Vec<Box<dyn Scene>> = graph
        .nodes
        .iter()
        .filter(|node| node.kind != HardwareTopologyKind::MemoryChannel)
        .map(|node| {
            Box::new(fact_row_scene(
                &SystemFactRow {
                    group: SystemFactGroup::Hardware,
                    label: node.label.clone(),
                    value: node.detail.clone().unwrap_or_else(missing_value),
                },
                palette,
            )) as Box<dyn Scene>
        })
        .collect();
    Some(Box::new(section_card_scene(
        t("cpu.topology").to_owned(),
        rows,
        width,
        palette,
    )))
}

#[cfg(test)]
#[path = "../../../tests/headless/pages/topology_graph.rs"]
mod tests;
