//! test-intent: behavior
//!
//! The hardware topology graph is pure data, so these tests pin the fold
//! laws directly: tree shape, node uniqueness, and the explicit unknown
//! nodes a lane that answered without a fact must state. The card test
//! mounts the real scene and reads back the stated text.

use std::collections::HashSet;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::asset::AssetPlugin;
use bevy::scene::{ScenePlugin, WorldSceneExt};
use bevy::text::EditableText;
use bevy::ui::widget::Text;
use taskforest_core::core::hardware::HardwareInfo;
use taskforest_core::core::metrics::{
    DiskMetrics, GpuMetrics, SmbiosMemorySnapshot, SmbiosModuleRow, SystemSnapshot,
};
use taskforest_core::core::storage::{
    StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
};
use taskmanager_application::i18n::t;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::UiSize;

use super::{HardwareTopologyPresence, hardware_topology_graph, topology_graph_card_scene};
use crate::palette::ui_palette;

fn hardware(sockets: Option<u16>) -> HardwareInfo {
    HardwareInfo {
        sockets,
        cpu_brand: Some("Test CPU".into()),
        motherboard_model: Some("Test Board".into()),
        ..Default::default()
    }
}

fn smbios() -> SmbiosMemorySnapshot {
    let modules = vec![
        SmbiosModuleRow {
            slot: 0,
            locator: Some("ChannelA-DIMM0".into()),
            ..Default::default()
        },
        SmbiosModuleRow {
            slot: 1,
            locator: Some("ChannelA-DIMM1".into()),
            ..Default::default()
        },
        SmbiosModuleRow {
            slot: 2,
            locator: Some("ChannelB-DIMM0".into()),
            ..Default::default()
        },
        SmbiosModuleRow {
            slot: 3,
            locator: Some("BareSlot".into()),
            ..Default::default()
        },
        SmbiosModuleRow {
            slot: 4,
            locator: Some(String::new()),
            ..Default::default()
        },
    ];
    SmbiosMemorySnapshot::success(5, 5, modules, None)
}

fn devices() -> SystemSnapshot {
    SystemSnapshot {
        gpu: vec![
            {
                let mut gpu = GpuMetrics::new("gpu-0", "Vendor GPU");
                gpu.marketing_name = Some("Vendor Flagship".into());
                gpu.pci_slot = Some("0000:00:02.0".into());
                gpu
            },
            GpuMetrics::new("gpu-1", "Slotless GPU"),
        ],
        disks: vec![pcie_disk(), sata_disk()],
        ..Default::default()
    }
}

fn pcie_disk() -> DiskMetrics {
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.device_id = "disk-0".into();
    disk.model = "Test NVMe".into();
    disk.apply_connection(StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Pcie,
        StorageDeviceKind::Physical,
    ));
    disk
}

fn sata_disk() -> DiskMetrics {
    let mut disk = DiskMetrics::new("sda");
    disk.device_id = "disk-1".into();
    disk.apply_connection(StorageConnection::new(
        StorageProtocol::Ata,
        StorageInterconnect::Sata,
        StorageDeviceKind::Physical,
    ));
    disk
}

fn node<'graph>(
    graph: &'graph super::HardwareTopologyGraph,
    id: &str,
) -> &'graph super::HardwareTopologyNode {
    graph
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("the {id} node exists"))
}

#[test]
fn socket_count_folds_into_one_known_node_per_socket() {
    let graph = hardware_topology_graph(Some(&hardware(Some(2))), None, None);
    assert_eq!(graph.nodes.len(), 3, "the board plus two sockets");
    let board = node(&graph, "board");
    assert_eq!(board.parent, None, "the board is the root");
    assert_eq!(board.detail.as_deref(), Some("Test Board"));
    for index in 0..2u16 {
        let socket = node(&graph, &format!("socket:{index}"));
        assert_eq!(socket.parent.as_deref(), Some("board"));
        assert_eq!(socket.presence, HardwareTopologyPresence::Known);
    }
    assert_eq!(
        node(&graph, "socket:0").detail.as_deref(),
        Some("Test CPU"),
        "the first socket carries the CPU brand"
    );
    assert_eq!(
        node(&graph, "socket:1").detail.as_deref(),
        None,
        "only the first socket names its occupant"
    );
}

#[test]
fn a_missing_socket_count_states_one_explicit_unknown_socket() {
    let graph = hardware_topology_graph(Some(&hardware(None)), None, None);
    let unknown = node(&graph, "socket:unknown");
    assert_eq!(unknown.presence, HardwareTopologyPresence::Unknown);
    assert_eq!(unknown.detail, None);
    assert_eq!(unknown.parent.as_deref(), Some("board"));
    assert!(
        !graph.states_known_facts(),
        "an all-unknown fold states nothing renderable"
    );
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    assert!(
        topology_graph_card_scene(&graph, None, &palette).is_none(),
        "the all-unknown fold stays out of the grid"
    );
}

#[test]
fn memory_channels_fold_from_locators_and_state_unassigned_slots() {
    let graph = hardware_topology_graph(None, Some(&smbios()), None);
    let channel_a = node(&graph, "channel:ChannelA");
    assert_eq!(channel_a.presence, HardwareTopologyPresence::Known);
    assert_eq!(channel_a.detail.as_deref(), Some("DIMM0 · DIMM1"));
    assert_eq!(channel_a.parent.as_deref(), Some("board"));
    assert_eq!(
        node(&graph, "channel:ChannelB").detail.as_deref(),
        Some("DIMM0")
    );
    let unassigned = node(&graph, "channel:unknown");
    assert_eq!(unassigned.presence, HardwareTopologyPresence::Unknown);
    assert_eq!(
        unassigned.detail.as_deref(),
        Some(format!("BareSlot · {} 4", t("system.slot")).as_str()),
        "a locator that names no channel keeps its raw text, an empty one states the slot"
    );
}

#[test]
fn pcie_devices_fold_from_gpu_slots_and_nvme_disks() {
    let graph = hardware_topology_graph(None, None, Some(&devices()));
    let slotted = node(&graph, "pci:0000:00:02.0");
    assert_eq!(slotted.presence, HardwareTopologyPresence::Known);
    assert_eq!(slotted.detail.as_deref(), Some("0000:00:02.0"));
    assert_eq!(slotted.label, "Vendor Flagship");
    let slotless = node(&graph, "gpu:gpu-1");
    assert_eq!(slotless.presence, HardwareTopologyPresence::Unknown);
    assert_eq!(slotless.detail, None);
    assert_eq!(slotless.label, "Slotless GPU");
    let nvme = node(&graph, "disk:disk-0");
    assert_eq!(nvme.detail.as_deref(), Some("Test NVMe"));
    assert!(
        graph.nodes.iter().all(|node| node.id != "disk:disk-1"),
        "a SATA disk is not PCIe topology"
    );
}

#[test]
fn the_tree_is_unique_and_parents_precede_children() {
    let graph =
        hardware_topology_graph(Some(&hardware(Some(2))), Some(&smbios()), Some(&devices()));
    let ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(ids.len(), graph.nodes.len(), "node ids are unique");
    let roots = graph
        .nodes
        .iter()
        .filter(|node| node.parent.is_none())
        .count();
    assert_eq!(roots, 1, "exactly one board root");
    for (index, node) in graph.nodes.iter().enumerate() {
        let Some(parent) = node.parent.as_deref() else {
            continue;
        };
        assert_eq!(parent, "board", "every child hangs off the board");
        let parent_index = graph
            .nodes
            .iter()
            .position(|candidate| candidate.id == parent)
            .expect("the parent node exists");
        assert!(parent_index < index, "parents precede children");
    }
    assert!(
        graph.states_known_facts(),
        "the combined fold states known facts"
    );
}

#[test]
fn the_card_states_the_graph_without_repeating_channel_rows() {
    let graph =
        hardware_topology_graph(Some(&hardware(Some(2))), Some(&smbios()), Some(&devices()));
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let card =
        topology_graph_card_scene(&graph, None, &palette).expect("the combined fold renders");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    crate::text_selection::register(&mut app);
    app.world_mut()
        .spawn_scene(card)
        .expect("the card scene mounts");
    app.update();

    let world = app.world_mut();
    let mut stated: Vec<String> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.clone())
        .collect();
    let mut readouts = world.query::<&EditableText>();
    stated.extend(readouts.iter(world).map(|text| text.value().to_string()));
    assert!(
        stated.iter().any(|text| text == "Test Board"),
        "the board row states its model: {stated:?}"
    );
    assert!(
        stated.iter().any(|text| text.contains("Test CPU")),
        "the socket row carries the CPU detail: {stated:?}"
    );
    assert!(
        stated.iter().any(|text| text.contains("0000:00:02.0")),
        "the PCIe GPU row states its slot: {stated:?}"
    );
    assert!(
        stated.iter().any(|text| text.contains("Test NVMe")),
        "the PCIe NVMe row states its model: {stated:?}"
    );
    assert!(
        !stated.iter().any(|text| text == "ChannelA"),
        "channel rows stay in the memory topology card: {stated:?}"
    );
}
