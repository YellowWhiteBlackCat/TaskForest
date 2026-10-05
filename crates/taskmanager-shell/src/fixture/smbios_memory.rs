//! Synthetic hardware-review facts, accepted through the shared request session.

use taskmanager_application::{
    CorrelatedEvent, PlatformEventBatch, PlatformEventContext, SmbiosMemoryEvent,
};
use taskmanager_core::core::metrics::{SmbiosMemorySnapshot, SmbiosModuleRow};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

use crate::{DirectTrackState, ShellApp};

/// A populated pair, an incomplete populated module and one empty slot.
#[must_use]
pub fn memory_inventory_snapshot() -> SmbiosMemorySnapshot {
    let modules = (0..2)
        .map(|slot| SmbiosModuleRow {
            slot,
            size_mb: Some(16 * 1024),
            speed_mts: Some(5600),
            configured_speed_mts: Some(5200),
            manufacturer: Some("Example Memory".into()),
            part_number: Some("EXAMPLE-DDR5-16G-SODIMM".into()),
            form_factor: Some("SODIMM".into()),
            memory_type: Some("DDR5".into()),
            locator: Some(format!(
                "Channel{}-DIMM0",
                if slot == 0 { "A" } else { "B" }
            )),
            ..SmbiosModuleRow::default()
        })
        .chain([SmbiosModuleRow {
            slot: 2,
            ..SmbiosModuleRow::default()
        }])
        .collect();
    SmbiosMemorySnapshot::success(4, 3, modules, None)
}

fn terminal() -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.smbios_memory_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::MIN,
            capability: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 0,
        },
        SmbiosMemoryEvent::Update(memory_inventory_snapshot()),
    ));
    batch
}

/// Capture/demo-only completion; the normal host never calls this seed.
pub fn seed_shell_memory_inventory(shell: &mut ShellApp) {
    let attempt = shell.begin_smbios_memory_request();
    if shell.accept_smbios_memory_request(attempt, RequestId::MIN) {
        shell.apply_platform_batch(terminal());
    }
}

/// The same request payload for the GPUI direct composition.
pub fn seed_direct_memory_inventory(track: &mut DirectTrackState) {
    let attempt = track.begin_smbios_memory_request();
    if track.accept_smbios_memory_request(attempt, RequestId::MIN) {
        let _ = track.apply_platform_batch(terminal());
    }
}
