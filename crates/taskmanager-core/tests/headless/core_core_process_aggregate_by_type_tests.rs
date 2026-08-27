use super::*;
use crate::core::ScalarObservation;

fn proc(pid: u32, name: &str, cpu: f32, mem: u64) -> ProcessItem {
    ProcessItem::new(pid, name).with_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(cpu, 1),
        memory_bytes: ScalarObservation::available(mem, 1),
        ..ProcessScalarObservations::default()
    })
}

/// Kernel (bracketed) and userspace processes split into two groups, each
/// summing its members' CPU%/memory; sorted by CPU% desc.
#[test]
fn splits_kernel_and_userspace_and_sums_resources() {
    let procs = [
        proc(1, "firefox", 10.0, 100),
        proc(2, "editor", 5.0, 50),
        proc(3, "[kworker]", 1.0, 10),
    ];
    let refs: Vec<&ProcessItem> = procs.iter().collect();
    let groups = aggregate_by_type(&refs);

    assert_eq!(groups.len(), 2, "two type groups");
    let userspace = groups
        .iter()
        .find(|g| g.name == "Userspace")
        .expect("Userspace");
    let kernel = groups.iter().find(|g| g.name == "Kernel").expect("Kernel");
    assert_eq!(userspace.process_count, 2);
    assert_eq!(userspace.total_cpu_usage, 15.0);
    assert_eq!(userspace.total_memory_bytes, 150);
    assert_eq!(kernel.process_count, 1);
    assert_eq!(kernel.total_cpu_usage, 1.0);
    assert_eq!(kernel.application_identity, None);
    // CPU% desc ordering: Userspace (15%) before Kernel (1%).
    assert_eq!(groups[0].name, "Userspace");
    assert_eq!(groups[1].name, "Kernel");
}

/// A class with no processes never appears as a fabricated empty group.
#[test]
fn omits_an_empty_class() {
    let procs = [proc(1, "app", 3.0, 30)];
    let refs: Vec<&ProcessItem> = procs.iter().collect();
    let groups = aggregate_by_type(&refs);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Userspace");
}
