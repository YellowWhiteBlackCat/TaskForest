use super::*;
use taskforest_core::LimitValue;

#[test]
fn format_open_files_saturation_variants() {
    assert_eq!(
        format_open_files_saturation(
            42,
            Some(LimitValue::Value(1024)),
            Some(LimitValue::Value(4096)),
            "∞",
        ),
        Some("42 / 1024 (4%) [max 4096]".to_string())
    );
    assert_eq!(
        format_open_files_saturation(
            512,
            Some(LimitValue::Value(1024)),
            Some(LimitValue::Value(1024)),
            "∞",
        ),
        Some("512 / 1024 (50%)".to_string())
    );
    assert_eq!(
        format_open_files_saturation(
            10,
            Some(LimitValue::Value(100)),
            Some(LimitValue::Unlimited),
            "Unlimited",
        ),
        Some("10 / 100 (10%) [max Unlimited]".to_string())
    );
    assert_eq!(
        format_open_files_saturation(
            10,
            Some(LimitValue::Unlimited),
            Some(LimitValue::Unlimited),
            "∞",
        ),
        Some("10 / ∞".to_string())
    );
    assert_eq!(format_open_files_saturation(10, None, None, "∞"), None);
}

#[test]
fn uninterruptible_ratio_uses_the_observed_inventory_and_keeps_real_zero() {
    let process = |pid: u32, name: &str, status: &str| {
        let mut process = ProcessItem::new(pid, name);
        process.status = status.to_owned();
        process
    };
    let processes = vec![
        process(1, "blocked-a", "D"),
        process(2, "sleeping", "S"),
        process(3, "running", "R"),
        process(4, "blocked-b", "D"),
    ];
    assert_eq!(uninterruptible_process_count(Some(&processes)), Some(2));
    assert_eq!(uninterruptible_process_ratio(Some(&processes)), Some(50.0));
    assert_eq!(
        uninterruptible_process_summary(Some(&processes)),
        Some(format!("{} 2 · 50.0%", t("proc.d_state")))
    );

    let healthy = vec![process(5, "sleeping", "S")];
    assert_eq!(uninterruptible_process_ratio(Some(&healthy)), Some(0.0));
    assert_eq!(
        uninterruptible_process_summary(Some(&healthy)),
        Some(format!("{} 0 · 0.0%", t("proc.d_state")))
    );
    assert_eq!(uninterruptible_process_ratio(Some(&[])), None);
    assert_eq!(uninterruptible_process_summary(None), None);
}
