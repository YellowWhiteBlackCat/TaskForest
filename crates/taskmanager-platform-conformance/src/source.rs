//! Shared discovery-authority coherence contract.

use taskmanager_core::core::source::SourceOutcome;
use taskmanager_platform_contract::DeviceSourceSnapshot;

/// Verify that discovery outcome, stable IDs and item count tell one coherent
/// story. The value-specific assembler remains responsible for proving that
/// its payload rows correspond to these IDs.
pub fn assert_device_discovery_consistent<T>(
    snapshot: &DeviceSourceSnapshot<T>,
) -> Result<(), String> {
    let ids = snapshot.discovered_devices();
    if ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("discovered device IDs are not strictly sorted and unique".to_string());
    }

    let expected_count = ids.len();
    if snapshot.discovery().item_count != expected_count {
        return Err(format!(
            "discovery item_count {} differs from {} stable IDs",
            snapshot.discovery().item_count,
            expected_count
        ));
    }

    match snapshot.discovery().outcome {
        SourceOutcome::Available if ids.is_empty() => {
            Err("available discovery has no stable device IDs; use Empty".to_string())
        }
        SourceOutcome::Empty if !ids.is_empty() => {
            Err("empty discovery contains stable device IDs".to_string())
        }
        SourceOutcome::Unavailable(_) if !ids.is_empty() => {
            Err("unavailable discovery claims stable device IDs".to_string())
        }
        SourceOutcome::Available
        | SourceOutcome::Empty
        | SourceOutcome::Partial(_)
        | SourceOutcome::Unavailable(_) => Ok(()),
    }
}

#[cfg(test)]
#[path = "../tests/headless/source_contract.rs"]
mod tests;
