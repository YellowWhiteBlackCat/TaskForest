use super::container::{ContainerMetadata, ContainerRuntime};
use super::group_aggregate::aggregate_by_container_typed;
use super::*;
use crate::core::{ProcessOwner, ScalarObservation};

fn proc(
    pid: u32,
    name: &str,
    cpu: f32,
    mem: u64,
    container: Option<ContainerMetadata>,
) -> ProcessItem {
    let mut item =
        ProcessItem::new(pid, name).with_scalar_observations(ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(cpu, 1),
            memory_bytes: ScalarObservation::available(mem, 1),
            ..ProcessScalarObservations::default()
        });
    if let Some(metadata) = container {
        item =
            item.with_metadata_observations(ProcessMetadataObservations::current_with_container(
                ProcessOwner::opaque("tester"),
                None,
                Some(metadata),
                1,
            ));
    }
    item
}

fn docker(id: &str) -> ContainerMetadata {
    ContainerMetadata::new(ContainerRuntime::Docker, id)
}

/// Container members fold their CPU%/memory into typed buckets keyed by
/// `(runtime, container_id)`; host processes never join a fabricated bucket.
#[test]
fn groups_only_container_processes_by_runtime_and_id() {
    let procs = [
        proc(1, "nginx", 4.0, 100, Some(docker("a1b2c3d4e5f6"))),
        proc(2, "worker", 3.0, 50, Some(docker("a1b2c3d4e5f6"))),
        proc(3, "redis", 2.0, 30, Some(docker("9f8e7d6c5b4a"))),
        proc(4, "host-shell", 1.0, 10, None),
    ];
    let refs: Vec<&ProcessItem> = procs.iter().collect();
    let groups = aggregate_by_container_typed(&refs, 1);

    assert_eq!(groups.len(), 2, "two container buckets, host omitted");
    let a1 = groups
        .iter()
        .find(|g| g.container_id() == "a1b2c3d4e5f6")
        .expect("first docker bucket");
    assert_eq!(a1.runtime(), ContainerRuntime::Docker);
    assert_eq!(a1.process_count(), 2);
    assert_eq!(a1.cpu().current_value(), Some(&7.0));
    assert_eq!(a1.memory().current_value(), Some(&150));
    let a9 = groups
        .iter()
        .find(|g| g.container_id() == "9f8e7d6c5b4a")
        .expect("second docker bucket");
    assert_eq!(a9.process_count(), 1);
    assert_eq!(a9.cpu().current_value(), Some(&2.0));
    // CPU% desc: the 7% bucket leads the 2% bucket.
    assert_eq!(groups[0].container_id(), "a1b2c3d4e5f6");
}

/// The same container id under two runtimes stays separate; a named bucket
/// keeps the name authority exactly once and never invents one from the id.
#[test]
fn keeps_runtimes_apart_and_preserves_optional_names() {
    let procs = [
        proc(
            1,
            "web",
            1.0,
            10,
            Some(docker("a1b2c3d4e5f6").with_name("web-prod")),
        ),
        proc(
            2,
            "web2",
            1.0,
            10,
            Some(ContainerMetadata::new(
                ContainerRuntime::Podman,
                "a1b2c3d4e5f6",
            )),
        ),
    ];
    let refs: Vec<&ProcessItem> = procs.iter().collect();
    let groups = aggregate_by_container_typed(&refs, 1);

    assert_eq!(groups.len(), 2, "same id, different runtimes stay apart");
    let named = groups
        .iter()
        .find(|g| g.runtime() == ContainerRuntime::Docker)
        .expect("docker bucket");
    assert_eq!(named.name(), Some("web-prod"));
    assert_eq!(named.runtime_label(), "Docker");
    let unnamed = groups
        .iter()
        .find(|g| g.runtime() == ContainerRuntime::Podman)
        .expect("podman bucket");
    assert_eq!(unnamed.name(), None, "no name is never fabricated from id");
}

/// A container observation that is not currently available never joins a
/// bucket: only a *current* metadata fact may claim a container identity.
#[test]
fn omits_processes_without_a_current_container_observation() {
    // An unobserved (Unknown) container fact must not fabricate a bucket.
    let mut item =
        ProcessItem::new(1, "unobserved").with_scalar_observations(ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(1.0, 1),
            memory_bytes: ScalarObservation::available(10, 1),
            ..ProcessScalarObservations::default()
        });
    item.apply_metadata_observations(ProcessMetadataObservations::current_with_container(
        ProcessOwner::opaque("tester"),
        None,
        None,
        1,
    ));
    let refs = vec![&item];
    assert!(
        aggregate_by_container_typed(&refs, 1).is_empty(),
        "an absent container observation yields no bucket"
    );
}
