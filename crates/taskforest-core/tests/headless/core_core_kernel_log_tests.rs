use super::*;

#[test]
fn kernel_priority_mapping_is_limited_to_health_error_levels() {
    assert_eq!(
        KernelLogPriority::from_number(0),
        Some(KernelLogPriority::Emergency)
    );
    assert_eq!(
        KernelLogPriority::from_number(3),
        Some(KernelLogPriority::Error)
    );
    assert_eq!(KernelLogPriority::from_number(4), None);
    assert_eq!(KernelLogPriority::Critical.number(), 2);
}

#[test]
fn kernel_entries_trim_and_bound_message_text() {
    let entry =
        KernelLogEntry::new(KernelLogPriority::Error, "  nvme reset  ").expect("non-empty message");
    assert_eq!(entry.message, "nvme reset");
    assert!(KernelLogEntry::new(KernelLogPriority::Error, " \n ").is_none());
    let long =
        KernelLogEntry::new(KernelLogPriority::Error, "x".repeat(600)).expect("bounded message");
    assert_eq!(long.message.chars().count(), 512);
}
