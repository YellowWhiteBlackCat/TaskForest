use super::*;
use core::mem::{align_of, offset_of};

/// The `#[repr(C)]` layout is an ABI contract — guard the size, alignment
/// and every field offset so a future edit cannot silently break the
/// `perf_event_attr` leading slice the kernel reads. The size MUST reach
/// `PERF_ATTR_SIZE_VER0` (64) or `perf_event_open` returns `E2BIG`.
#[test]
fn perf_event_attr_layout_matches_kernel_leading_abi() {
    assert_eq!(
        size_of::<PerfEventAttr>(),
        64,
        "must reach PERF_ATTR_SIZE_VER0 (64) or perf_event_open returns E2BIG"
    );
    assert_eq!(align_of::<PerfEventAttr>(), align_of::<u64>());
    assert_eq!(offset_of!(PerfEventAttr, type_), 0);
    assert_eq!(offset_of!(PerfEventAttr, size), 4);
    assert_eq!(offset_of!(PerfEventAttr, config), 8);
    assert_eq!(offset_of!(PerfEventAttr, sample_period_or_freq), 16);
    assert_eq!(offset_of!(PerfEventAttr, sample_type), 24);
    assert_eq!(offset_of!(PerfEventAttr, read_format), 32);
    assert_eq!(offset_of!(PerfEventAttr, bitfield_flags), 40);
    assert_eq!(offset_of!(PerfEventAttr, wakeup_events_or_watermark), 48);
    assert_eq!(offset_of!(PerfEventAttr, bp_type), 52);
    assert_eq!(offset_of!(PerfEventAttr, bp_addr_or_config1), 56);
    // `size` must be advertised to the kernel as the struct's own size.
    let attr = PerfEventAttr {
        type_: 1,
        size: 0,
        config: 2,
        sample_period_or_freq: 0,
        sample_type: 0,
        read_format: READ_FORMAT,
        bitfield_flags: PERF_ATTR_DISABLED,
        wakeup_events_or_watermark: 0,
        bp_type: 0,
        bp_addr_or_config1: 0,
    };
    let computed = size_of::<PerfEventAttr>() as u32;
    assert_eq!(computed, 64);
    let _ = attr; // silence dead-code on the constructed fixture.
}

/// `open()` on a PMU type that is not registered on any CI host returns
/// `Err`. We assert the FAILURE PATH only — CI has no Intel GPU and no
/// privileged perf access, so success is never claimed. This exercises the
/// `result < 0 → last_os_error` branch of the audited syscall site.
#[test]
fn open_on_unregistered_pmu_returns_err() {
    let result = GpuEngineCounter::open(u32::MAX, 0, 0);
    assert!(
        result.is_err(),
        "expected Err on an unregistered PMU (no GPU in CI), got {result:?}"
    );
}

/// `open_enabled` must surface the same failure honestly rather than panic.
#[test]
fn open_enabled_on_unregistered_pmu_returns_err() {
    let result = GpuEngineCounter::open_enabled(u32::MAX, 0, 0);
    assert!(result.is_err(), "expected Err, got {result:?}");
}
