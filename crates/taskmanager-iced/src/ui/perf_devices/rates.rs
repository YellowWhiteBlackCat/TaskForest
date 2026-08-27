//! Device-family throughput scale and unit-pair formatting helpers.
//! Kept separate from the large detail-panel module so rates, graph captions,
//! and hover readouts stay on one preference-aware projection.

use super::*;

/// The graph scale for a device-family throughput mini-graph (per-disk
/// read+write, per-NIC rx+tx), carrying the resolved unit pair so the graph's
/// caption summary and hover pill format through the same persisted Drive /
/// Network preference as the device's scalar rows — the disk call sites pass
/// [`IcedApp::drive_units`], the NIC call sites [`IcedApp::network_units`].
/// Pure so the pair wiring is table-tested headlessly.
pub(crate) fn throughput_scale(units: UnitPrefs) -> device_chart::DeviceMetricScale {
    device_chart::DeviceMetricScale::BytesPerSecond {
        use_bytes: units.use_bytes,
        use_base2: units.use_base2,
    }
}

/// Format a per-second rate honoring the persisted unit preferences (GPUI
/// Settings Units matrix parity): bytes vs bits and base-2 vs base-10.
/// `use_bytes`/`use_base2` come from the resolved drive (disk) or network
/// preference pair the caller passes.
pub(crate) fn rate_text_pref(value: Option<u64>, use_bytes: bool, use_base2: bool) -> String {
    value.map_or_else(missing_value, |value| {
        format!(
            "{}/s",
            super::quantity_text_pref(value, use_bytes, use_base2)
        )
    })
}
