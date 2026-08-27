use super::*;

#[test]
fn cap_plus_one_probe_is_an_explicit_truncation_receipt() {
    let mut probed = Vec::new();
    let inventory = collect_bounded_gpu_items::<u32, ()>(|index| {
        probed.push(index);
        Ok(GpuProbe::Item(index))
    })
    .expect("infallible fixture");

    assert_eq!(probed, (0..=MAX_GPU_ADAPTERS).collect::<Vec<_>>());
    assert_eq!(inventory.items, (0..MAX_GPU_ADAPTERS).collect::<Vec<_>>());
    assert!(inventory.truncated, "the cap+1 probe must be explicit");
    assert_eq!(inventory.items.len(), MAX_GPU_ADAPTERS as usize);
}

#[test]
fn normal_end_and_skipped_rows_do_not_claim_truncation() {
    let inventory = collect_bounded_gpu_items::<u32, ()>(|index| match index {
        1 => Ok(GpuProbe::Skip),
        3 => Ok(GpuProbe::End),
        _ => Ok(GpuProbe::Item(index)),
    })
    .expect("infallible fixture");

    assert_eq!(inventory.items, vec![0, 2]);
    assert!(!inventory.truncated);
}

#[test]
fn unreadable_cap_plus_one_row_still_proves_truncation() {
    let inventory = collect_bounded_gpu_items::<u32, ()>(|index| {
        if index == MAX_GPU_ADAPTERS {
            Ok(GpuProbe::Skip)
        } else {
            Ok(GpuProbe::Item(index))
        }
    })
    .expect("infallible fixture");

    assert_eq!(inventory.items.len(), MAX_GPU_ADAPTERS as usize);
    assert!(inventory.truncated);
}
